package journal

import (
	"errors"
	"math"
	"path/filepath"
	"reflect"
	"testing"
	"time"
)

const freshSeedPast = uint64(1_600_000_000_000_000)

type realtimeBounds struct {
	entries, head, tail uint64
}

func freshSeedConfig(seed *uint64) LogConfig {
	return LogConfig{Options: testOptions(), Source: "system", FreshRealtimeSeedUsec: seed}
}

func openFreshSeedLog(t *testing.T, root string, config LogConfig) *Log {
	t.Helper()
	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog() error = %v", err)
	}
	return log
}

func appendAt(t *testing.T, log *Log, message string, opts EntryOptions) {
	t.Helper()
	if err := log.Append([]Field{StringField("MESSAGE", message)}, opts); err != nil {
		t.Fatalf("Append(%s) error = %v", message, err)
	}
}

func closeFreshSeedLog(t *testing.T, log *Log) {
	t.Helper()
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func freshSeedBounds(t *testing.T, root string, config LogConfig) []realtimeBounds {
	t.Helper()
	var bounds []realtimeBounds
	for _, path := range journalFiles(t, filepath.Join(root, config.Options.MachineID.String())) {
		header := readJournalSnapshot(t, path).header
		bounds = append(bounds, realtimeBounds{header.nEntries, header.headEntryRealtime, header.tailEntryRealtime})
	}
	return bounds
}

func TestLogFreshSeedZeroMatchesUnsetForPastEventTimes(t *testing.T) {
	for name, seed := range map[string]*uint64{"unset": nil, "zero": FreshRealtimeSeed(0)} {
		t.Run(name, func(t *testing.T) {
			root := t.TempDir()
			config := freshSeedConfig(seed)
			log := openFreshSeedLog(t, root, config)
			appendAt(t, log, "event 1", EntryOptions{RealtimeUsec: freshSeedPast, MonotonicUsec: 1})
			appendAt(t, log, "event 2", EntryOptions{RealtimeUsec: freshSeedPast + 5, MonotonicUsec: 2})
			closeFreshSeedLog(t, log)
			want := []realtimeBounds{{2, freshSeedPast, freshSeedPast + 5}}
			if got := freshSeedBounds(t, root, config); !reflect.DeepEqual(got, want) {
				t.Fatalf("bounds = %v, want %v", got, want)
			}
		})
	}
}

func TestLogFreshSeedIsAFloor(t *testing.T) {
	root := t.TempDir()
	config := freshSeedConfig(FreshRealtimeSeed(freshSeedPast))
	log := openFreshSeedLog(t, root, config)
	appendAt(t, log, "below the seed", EntryOptions{RealtimeUsec: freshSeedPast - 10, MonotonicUsec: 1})
	appendAt(t, log, "above the seed", EntryOptions{RealtimeUsec: freshSeedPast + 100, MonotonicUsec: 2})
	closeFreshSeedLog(t, log)
	want := []realtimeBounds{{2, freshSeedPast + 1, freshSeedPast + 100}}
	if got := freshSeedBounds(t, root, config); !reflect.DeepEqual(got, want) {
		t.Fatalf("bounds = %v, want %v", got, want)
	}
}

func TestLogExistingTailWinsOverFreshSeed(t *testing.T) {
	root := t.TempDir()
	config := freshSeedConfig(FreshRealtimeSeed(0))
	log := openFreshSeedLog(t, root, config)
	appendAt(t, log, "first run", EntryOptions{RealtimeUsec: freshSeedPast, MonotonicUsec: 1})
	closeFreshSeedLog(t, log)

	log = openFreshSeedLog(t, root, config)
	appendAt(t, log, "second run, older", EntryOptions{RealtimeUsec: freshSeedPast - 10, MonotonicUsec: 2})
	closeFreshSeedLog(t, log)

	var maxTail uint64
	for _, b := range freshSeedBounds(t, root, config) {
		maxTail = max(maxTail, b.tail)
	}
	if maxTail != freshSeedPast+1 {
		t.Fatalf("max tail realtime = %d, want %d", maxTail, freshSeedPast+1)
	}
}

func TestLogFreshSeedKeepsDuplicateAndOutOfOrderClamping(t *testing.T) {
	root := t.TempDir()
	config := freshSeedConfig(FreshRealtimeSeed(0))
	log := openFreshSeedLog(t, root, config)
	appendAt(t, log, "a", EntryOptions{RealtimeUsec: freshSeedPast, MonotonicUsec: 1})
	appendAt(t, log, "b duplicate", EntryOptions{RealtimeUsec: freshSeedPast, MonotonicUsec: 2})
	appendAt(t, log, "c older", EntryOptions{RealtimeUsec: freshSeedPast - 50, MonotonicUsec: 3})
	closeFreshSeedLog(t, log)
	want := []realtimeBounds{{3, freshSeedPast, freshSeedPast + 2}}
	if got := freshSeedBounds(t, root, config); !reflect.DeepEqual(got, want) {
		t.Fatalf("bounds = %v, want %v", got, want)
	}
}

func TestLogFreshSeedZeroKeepsWallClockForOmittedRealtime(t *testing.T) {
	root := t.TempDir()
	config := freshSeedConfig(FreshRealtimeSeed(0))
	before := uint64(time.Now().UnixMicro())
	log := openFreshSeedLog(t, root, config)
	appendAt(t, log, "no realtime", EntryOptions{MonotonicUsec: 1})
	closeFreshSeedLog(t, log)
	bounds := freshSeedBounds(t, root, config)
	if len(bounds) != 1 || bounds[0].head < before {
		t.Fatalf("bounds = %v, want one file with head realtime >= %d", bounds, before)
	}
}

func TestLogFreshSeedMaximumIsRejected(t *testing.T) {
	_, err := NewLog(t.TempDir(), freshSeedConfig(FreshRealtimeSeed(math.MaxUint64)))
	if !errors.Is(err, errInvalidJournal) {
		t.Fatalf("NewLog(MaxUint64 seed) error = %v, want errInvalidJournal", err)
	}
}
