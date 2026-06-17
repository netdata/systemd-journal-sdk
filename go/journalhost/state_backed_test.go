package journalhost

import (
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

var (
	testStateBootA = journal.UUID{0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf}
	testStateBootB = journal.UUID{0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf}
)

func TestStateBackedBootIDReusesSameBoot(t *testing.T) {
	dir := t.TempDir()
	var now, marker uint64 = 100_000_000, 10_000_000
	nextUUID := fixedUUIDs(testStateBootA, testStateBootB)
	opts := LoadOptions{StateDir: dir, StateFileName: "boot.state", NewUUID: nextUUID}
	probe := stateBackedProbe{
		realtimeNow: func() uint64 { return now },
		markerNow:   func() uint64 { return marker },
	}

	first, path, degraded, err := loadStateBackedBootID(opts, probe)
	if err != nil || degraded {
		t.Fatalf("first load id=%s path=%s degraded=%v err=%v", first, path, degraded, err)
	}
	now += 5_000_000
	marker += 5_000_000
	second, _, degraded, err := loadStateBackedBootID(opts, probe)
	if err != nil || degraded {
		t.Fatalf("second load id=%s degraded=%v err=%v", second, degraded, err)
	}
	if first != testStateBootA || second != testStateBootA {
		t.Fatalf("ids = %s, %s; want reused %s", first, second, testStateBootA)
	}
}

func TestStateBackedBootIDChangesAfterMinimumRebootTime(t *testing.T) {
	dir := t.TempDir()
	var now, marker uint64 = 100_000_000, 10_000_000
	opts := LoadOptions{StateDir: dir, StateFileName: "boot.state", NewUUID: fixedUUIDs(testStateBootA, testStateBootB)}
	probe := stateBackedProbe{
		realtimeNow: func() uint64 { return now },
		markerNow:   func() uint64 { return marker },
	}
	first, _, degraded, err := loadStateBackedBootID(opts, probe)
	if err != nil || degraded {
		t.Fatalf("first load id=%s degraded=%v err=%v", first, degraded, err)
	}
	now = 200_000_000
	marker = 79_000_000
	second, _, degraded, err := loadStateBackedBootID(opts, probe)
	if err != nil || degraded {
		t.Fatalf("second load id=%s degraded=%v err=%v", second, degraded, err)
	}
	if first != testStateBootA || second != testStateBootB {
		t.Fatalf("ids = %s, %s; want %s then %s", first, second, testStateBootA, testStateBootB)
	}
}

func TestStateBackedBootIDRecoversCorruptStateWithDegradedDiagnostics(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "boot.state")
	if err := os.WriteFile(path, []byte("not-state\n"), 0o600); err != nil {
		t.Fatalf("write corrupt state: %v", err)
	}
	opts := LoadOptions{StateDir: dir, StateFileName: "boot.state", NewUUID: fixedUUIDs(testStateBootA)}
	probe := stateBackedProbe{
		realtimeNow: func() uint64 { return 100_000_000 },
		markerNow:   func() uint64 { return 10_000_000 },
	}
	id, _, degraded, err := loadStateBackedBootID(opts, probe)
	if err == nil || !degraded {
		t.Fatalf("corrupt load id=%s degraded=%v err=%v; want degraded error", id, degraded, err)
	}
	if id != testStateBootA {
		t.Fatalf("corrupt recovery id = %s, want %s", id, testStateBootA)
	}
	if _, statErr := os.Stat(path + ".corrupt"); statErr != nil {
		t.Fatalf("corrupt backup missing: %v", statErr)
	}
	state, readErr := readStateFile(path)
	if readErr != nil {
		t.Fatalf("recovered state unreadable: %v", readErr)
	}
	if state.bootID != testStateBootA {
		t.Fatalf("recovered state boot id = %s, want %s", state.bootID, testStateBootA)
	}
}

func fixedUUIDs(ids ...journal.UUID) func() (journal.UUID, error) {
	index := 0
	return func() (journal.UUID, error) {
		if index >= len(ids) {
			return journal.UUID{}, errors.New("no test UUID left")
		}
		id := ids[index]
		index++
		return id, nil
	}
}
