package journal

import (
	"bytes"
	"errors"
	"sort"
)

var ErrMalformedFilter = errors.New("malformed journal filter")

type filterBuilder struct {
	level0         []filterExpr
	level1         []filterExpr
	current        [][]byte
	pending        bool
	expr           filterExpr
	stale          bool
	walkValid      bool
	backward       bool
	retry          bool
	walkOffset     uint64
	overrideOffset uint64
}

type filterExpr interface {
	seek(*Reader, uint64, Direction) (uint64, error)
	reset(*Reader, Direction) error
	refresh(*Reader) (filterExpr, error)
}

type matchExpr struct{ posting filterPostingCursor }
type andExpr []filterExpr
type orExpr []filterExpr
type falseExpr struct{}

func (f *filterBuilder) addMatch(data []byte) {
	f.pending = true
	if bytes.IndexByte(data, '=') >= 0 {
		f.current = append(f.current, append([]byte(nil), data...))
	}
}

func (f *filterBuilder) addDisjunction(r *Reader) error {
	f.pending = true
	return f.commitCurrent(r)
}

func (f *filterBuilder) addConjunction(r *Reader) error {
	f.pending = true
	if err := f.commitCurrent(r); err != nil {
		return err
	}
	f.commitLevel1()
	return nil
}

func (f *filterBuilder) build(r *Reader) error {
	if f.pending {
		if err := f.commitCurrent(r); err != nil {
			return err
		}
		f.commitLevel1()
		if len(f.level0) == 0 {
			return ErrMalformedFilter
		}
		expr := collapseFilterAnd(f.level0)
		if r.recovery.Incomplete() || f.stale {
			var err error
			expr, err = refreshStableFilter(r, expr)
			if err != nil {
				return err
			}
		}
		f.expr = expr
		f.level0 = nil
		f.pending = false
		f.stale = false
		f.walkValid = false
	} else if f.stale {
		expr, err := refreshStableFilter(r, f.expr)
		if err != nil {
			return err
		}
		f.expr = expr
		f.stale = false
		f.walkValid = false
	}
	return nil
}

func (f *filterBuilder) commitCurrent(r *Reader) error {
	if len(f.current) != 0 {
		if err := r.prepareFilterGroup(); err != nil {
			return err
		}
	}
	expr, err := buildCurrentFilterExpr(r, f.current)
	if err != nil {
		return err
	}
	if expr != nil {
		f.level1 = append(f.level1, expr)
	}
	f.current = nil
	return nil
}

func (f *filterBuilder) commitLevel1() {
	if expr := buildLevel1FilterExpr(f.level1); expr != nil {
		f.level0 = append(f.level0, expr)
	}
	f.level1 = nil
}

func collapseFilterAnd(parts []filterExpr) filterExpr {
	if len(parts) == 1 {
		return parts[0]
	}
	return andExpr(parts)
}

func buildLevel1FilterExpr(parts []filterExpr) filterExpr {
	if len(parts) == 0 {
		return nil
	}
	if len(parts) == 1 {
		return parts[0]
	}
	return orExpr(parts)
}

func buildCurrentFilterExpr(r *Reader, matches [][]byte) (filterExpr, error) {
	if len(matches) == 0 {
		return nil, nil
	}
	sort.SliceStable(matches, func(i, j int) bool {
		return bytes.Compare(filterMatchKey(matches[i]), filterMatchKey(matches[j])) < 0
	})
	var parts []filterExpr
	for start := 0; start < len(matches); {
		end := start + 1
		for end < len(matches) && bytes.Equal(filterMatchKey(matches[start]), filterMatchKey(matches[end])) {
			end++
		}
		values := make([]filterExpr, 0, end-start)
		for _, payload := range matches[start:end] {
			if r.header.dataHashTableOffset == 0 || r.header.dataHashTableSize < hashItemSize {
				return nil, errCorruptObject
			}
			offset, found, err := r.findDataOffsetByPayload(payload)
			if err != nil {
				return nil, err
			}
			var expr filterExpr = falseExpr{}
			if found {
				posting, err := r.newFilterPostingCursor(offset)
				if err != nil {
					return nil, err
				}
				if posting.declared != 0 {
					expr = &matchExpr{posting: posting}
				}
			}
			values = append(values, expr)
		}
		parts = append(parts, buildLevel1FilterExpr(values))
		start = end
	}
	return collapseFilterAnd(parts), nil
}

func filterMatchKey(payload []byte) []byte { return payload[:bytes.IndexByte(payload, '=')] }

func (m *matchExpr) seek(r *Reader, needle uint64, direction Direction) (uint64, error) {
	return m.posting.seek(r, needle, direction)
}
func (m *matchExpr) reset(r *Reader, direction Direction) error { return m.posting.reset(r, direction) }
func (m *matchExpr) refresh(r *Reader) (filterExpr, error) {
	posting, err := r.newFilterPostingCursor(m.posting.dataOffset)
	if err != nil {
		return nil, err
	}
	return &matchExpr{posting: posting}, nil
}

func refreshStableFilter(r *Reader, expr filterExpr) (filterExpr, error) {
	for {
		before := r.recovery
		next, err := expr.refresh(r)
		if err != nil {
			return nil, err
		}
		expr = next
		if sameRecoveryState(before, r.recovery) {
			return expr, nil
		}
	}
}

// Ordinary retries retain the installed DATA population while rereading nodes.
func invalidateFilterWalk(expr filterExpr) {
	switch value := expr.(type) {
	case *matchExpr:
		p := &value.posting
		p.nodes = p.nodes[:0]
		p.highest = 0
		p.node, p.slot, p.value = -1, 0, p.inline
	case andExpr:
		for _, child := range value {
			invalidateFilterWalk(child)
		}
	case orExpr:
		for _, child := range value {
			invalidateFilterWalk(child)
		}
	}
}

func prepareResolvedFilterWalk(expr filterExpr, direction Direction) {
	switch value := expr.(type) {
	case *matchExpr:
		p := &value.posting
		p.node, p.slot, p.value = -1, 0, p.inline
		if direction == DirectionBackward {
			// Resolved backward expressions prepare only the leaves they visit.
			p.node, p.value = -2, 0
		}
	case andExpr:
		for _, child := range value {
			prepareResolvedFilterWalk(child, direction)
		}
	case orExpr:
		for _, child := range value {
			prepareResolvedFilterWalk(child, direction)
		}
	}
}

func (a andExpr) seek(r *Reader, needle uint64, direction Direction) (uint64, error) {
	for {
		previous := needle
		for i := range a {
			index := i
			if direction == DirectionBackward {
				index = len(a) - 1 - i
			}
			value, err := a[index].seek(r, needle, direction)
			if err != nil || value == 0 {
				return 0, err
			}
			needle = value
		}
		if needle == previous {
			return needle, nil
		}
	}
}

func (o orExpr) seek(r *Reader, needle uint64, direction Direction) (uint64, error) {
	var best uint64
	for _, expr := range o {
		value, err := expr.seek(r, needle, direction)
		if err != nil {
			return 0, err
		}
		if value != 0 && (best == 0 || direction == DirectionForward && value < best || direction == DirectionBackward && value > best) {
			best = value
		}
	}
	return best, nil
}

func resetFilterChildren(r *Reader, children []filterExpr, direction Direction) error {
	for _, child := range children {
		if err := child.reset(r, direction); err != nil {
			return err
		}
	}
	return nil
}
func (a andExpr) reset(r *Reader, direction Direction) error {
	return resetFilterChildren(r, a, direction)
}
func (o orExpr) reset(r *Reader, direction Direction) error {
	return resetFilterChildren(r, o, direction)
}

func refreshFilterChildren(r *Reader, children []filterExpr) ([]filterExpr, error) {
	next := make([]filterExpr, len(children))
	for i, child := range children {
		var err error
		next[i], err = child.refresh(r)
		if err != nil {
			return nil, err
		}
	}
	return next, nil
}
func (a andExpr) refresh(r *Reader) (filterExpr, error) {
	children, err := refreshFilterChildren(r, a)
	return andExpr(children), err
}
func (o orExpr) refresh(r *Reader) (filterExpr, error) {
	children, err := refreshFilterChildren(r, o)
	return orExpr(children), err
}
func (falseExpr) seek(*Reader, uint64, Direction) (uint64, error) { return 0, nil }
func (falseExpr) reset(*Reader, Direction) error                  { return nil }
func (f falseExpr) refresh(*Reader) (filterExpr, error)           { return f, nil }
