package journalhost

import (
	"testing"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func TestProviderMonotonicUsecStrictlyIncreases(t *testing.T) {
	values := []uint64{5, 5, 4, 10}
	index := 0
	p := &Provider{
		bootID: testStateBootA,
		monotonicSource: func() uint64 {
			v := values[index]
			index++
			return v
		},
	}

	got := []uint64{p.MonotonicUsec(), p.MonotonicUsec(), p.MonotonicUsec(), p.MonotonicUsec()}
	want := []uint64{5, 6, 7, 10}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("monotonic[%d] = %d, want %d; all=%v", i, got[i], want[i], got)
		}
	}
}

func TestProviderEntryOptionsUseJournalTypes(t *testing.T) {
	p := &Provider{
		bootID:          testStateBootA,
		monotonicSource: func() uint64 { return 42 },
	}
	opts := p.EntryOptions()
	var _ journal.EntryOptions = opts
	if opts.BootID != testStateBootA {
		t.Fatalf("EntryOptions boot id = %s, want %s", opts.BootID, testStateBootA)
	}
	if opts.MonotonicUsec != 42 || !opts.MonotonicUsecSet {
		t.Fatalf("EntryOptions monotonic = %d set=%v, want 42 true", opts.MonotonicUsec, opts.MonotonicUsecSet)
	}
}
