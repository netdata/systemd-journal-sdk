//go:build cgo

package main

import (
	"reflect"
	"testing"
)

func TestVacuumHistoryOrder(t *testing.T) {
	// A<B by sequence, B<C by time, C<A by time: the old comparator cycles.
	rows := []vacuumCandidate{
		{name: "A", haveSeqno: true, seqnumID: "1", seqnum: 1, realtime: 300},
		{name: "B", haveSeqno: true, seqnumID: "1", seqnum: 2, realtime: 100},
		{name: "C", haveSeqno: true, seqnumID: "2", seqnum: 1, realtime: 200},
		{name: "D", realtime: 200},
		{name: "E", haveSeqno: true, seqnumID: "1", seqnum: 2, realtime: 50},
	}
	var permutations func(int)
	permutations = func(i int) {
		if i < len(rows) {
			for j := i; j < len(rows); j++ {
				rows[i], rows[j] = rows[j], rows[i]
				permutations(i + 1)
				rows[i], rows[j] = rows[j], rows[i]
			}
			return
		}
		copyRows := append([]vacuumCandidate(nil), rows...)
		order := newVacuumOrder(copyRows)
		var got []string
		for c := order.next(); c != nil; c = order.next() {
			got = append(got, c.name)
		}
		if !reflect.DeepEqual(got, []string{"C", "D", "A", "B", "E"}) {
			t.Fatalf("order=%v", got)
		}
	}
	permutations(0)
	if newVacuumOrder(nil).next() != nil {
		t.Fatal("nonempty order")
	}
}
