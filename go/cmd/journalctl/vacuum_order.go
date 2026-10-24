//go:build cgo

package main

import "sort"

type vacuumRun struct{ next, end int }

type vacuumOrder struct {
	candidates []vacuumCandidate
	heads      []vacuumRun
}

func newVacuumOrder(candidates []vacuumCandidate) *vacuumOrder {
	// A pairwise sequence/time comparator can cycle after a clock rollback.
	sort.Slice(candidates, func(i, j int) bool {
		a, b := candidates[i], candidates[j]
		if a.haveSeqno != b.haveSeqno {
			return a.haveSeqno
		}
		if a.haveSeqno {
			if a.seqnumID != b.seqnumID {
				return a.seqnumID < b.seqnumID
			}
			if a.seqnum != b.seqnum {
				return a.seqnum < b.seqnum
			}
		}
		return a.name < b.name
	})
	o := &vacuumOrder{candidates: candidates}
	for start := 0; start < len(candidates); {
		end := start + 1
		if candidates[start].haveSeqno {
			for end < len(candidates) && candidates[end].haveSeqno && candidates[end].seqnumID == candidates[start].seqnumID {
				end++
			}
		}
		o.heads = append(o.heads, vacuumRun{start, end})
		start = end
	}
	for i := len(o.heads)/2 - 1; i >= 0; i-- {
		o.down(i)
	}
	return o
}

func (o *vacuumOrder) less(i, j int) bool {
	a, b := o.candidates[o.heads[i].next], o.candidates[o.heads[j].next]
	if a.realtime != b.realtime {
		return a.realtime < b.realtime
	}
	if a.haveSeqno != b.haveSeqno {
		return a.haveSeqno
	}
	if a.haveSeqno && a.seqnumID != b.seqnumID {
		return a.seqnumID < b.seqnumID
	}
	return a.name < b.name
}

func (o *vacuumOrder) down(i int) {
	for {
		child := 2*i + 1
		if child >= len(o.heads) {
			return
		}
		if child+1 < len(o.heads) && o.less(child+1, child) {
			child++
		}
		if !o.less(child, i) {
			return
		}
		o.heads[i], o.heads[child] = o.heads[child], o.heads[i]
		i = child
	}
}

func (o *vacuumOrder) next() *vacuumCandidate {
	if len(o.heads) == 0 {
		return nil
	}
	candidate := &o.candidates[o.heads[0].next]
	o.heads[0].next++
	if o.heads[0].next == o.heads[0].end {
		o.heads[0] = o.heads[len(o.heads)-1]
		o.heads = o.heads[:len(o.heads)-1]
	}
	// The newly exposed timestamp may move backward; only the root changed.
	o.down(0)
	return candidate
}
