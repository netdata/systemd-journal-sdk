package journal

import (
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

type bootTestEntry struct {
	boot      byte
	realtime  uint64
	monotonic uint64
}

func bootTestID(b byte) UUID {
	var u UUID
	for i := range u {
		u[i] = b
	}
	return u
}

// writeBootTestJournal writes entries whose header boot ID is boot and, when
// withBootData, a matching _BOOT_ID field.
func writeBootTestJournal(t *testing.T, path string, seqnum byte, withBootData bool, entries []bootTestEntry) {
	t.Helper()
	var bootData func(byte) string
	if withBootData {
		bootData = func(b byte) string { return bootTestID(b).String() }
	}
	writeBootTestJournalData(t, path, seqnum, bootData, entries)
}

// writeBootTestJournalData writes entries whose header boot ID is boot and,
// when bootData is set, the _BOOT_ID value it returns.
func writeBootTestJournalData(t *testing.T, path string, seqnum byte, bootData func(byte) string, entries []bootTestEntry) {
	t.Helper()
	w, err := Create(path, Options{
		MachineID:       bootTestID(0x01),
		BootID:          bootTestID(entries[0].boot),
		SeqnumID:        bootTestID(seqnum),
		FileID:          bootTestID(seqnum + 1),
		FieldNamePolicy: FieldNamePolicyRaw,
	})
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		payloads := [][]byte{[]byte("MESSAGE=boot test")}
		if bootData != nil {
			payloads = append(payloads, []byte("_BOOT_ID="+bootData(e.boot)))
		}
		if err := w.AppendRaw(payloads, EntryOptions{RealtimeUsec: e.realtime, MonotonicUsec: e.monotonic, BootID: bootTestID(e.boot)}); err != nil {
			t.Fatal(err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
}

func bootInfo(index int64, boot byte, first, last int64) BootInfo {
	return BootInfo{Index: index, BootID: bootTestID(boot).String(), FirstEntry: first, LastEntry: last}
}

// ListBoots reports every boot of a multi-boot journal, as journalctl
// --list-boots does, not one boot per file.
func TestListBootsMultiBoot(t *testing.T) {
	dir := t.TempDir()
	first := filepath.Join(dir, "system.journal")
	writeBootTestJournal(t, first, 0x10, true, []bootTestEntry{
		{0xa0, 100, 1}, {0xa0, 110, 2},
		{0xb0, 200, 1}, {0xb0, 210, 2},
		{0xa0, 300, 3},
	})
	second := filepath.Join(dir, "user-1000.journal")
	writeBootTestJournal(t, second, 0x20, true, []bootTestEntry{
		{0xc0, 150, 1}, {0xc0, 160, 2},
		{0xb0, 220, 3},
	})

	r := mustOpenReaderFile(t, first)
	defer r.Close()
	got, err := r.ListBoots()
	if err != nil {
		t.Fatal(err)
	}
	want := []BootInfo{bootInfo(-1, 0xb0, 200, 210), bootInfo(0, 0xa0, 100, 300)}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("file ListBoots\n got: %+v\nwant: %+v", got, want)
	}

	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	got, err = dr.ListBoots()
	if err != nil {
		t.Fatal(err)
	}
	want = []BootInfo{bootInfo(-2, 0xc0, 150, 160), bootInfo(-1, 0xb0, 200, 220), bootInfo(0, 0xa0, 100, 300)}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("directory ListBoots\n got: %+v\nwant: %+v", got, want)
	}
}

// Journals without _BOOT_ID DATA fall back to entry header boot IDs.
func TestListBootsFallsBackToEntryHeaders(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeBootTestJournal(t, path, 0x30, false, []bootTestEntry{{0xd0, 500, 1}, {0xd0, 510, 2}, {0xe0, 600, 1}})
	r := mustOpenReaderFile(t, path)
	defer r.Close()
	got, err := r.ListBoots()
	if err != nil {
		t.Fatal(err)
	}
	want := []BootInfo{bootInfo(-1, 0xd0, 500, 510), bootInfo(0, 0xe0, 600, 600)}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("fallback ListBoots\n got: %+v\nwant: %+v", got, want)
	}
}

// Only canonical _BOOT_ID values name a boot: journalctl matches the
// canonical spelling, so an uppercase value is not one of its boots.
func TestListBootsIgnoresNonCanonicalBootIDData(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeBootTestJournalData(t, path, 0x50, func(b byte) string {
		if b == 0xbb {
			return strings.ToUpper(bootTestID(b).String())
		}
		return bootTestID(b).String()
	}, []bootTestEntry{{0xa0, 100, 1}, {0xbb, 200, 1}})
	r := mustOpenReaderFile(t, path)
	defer r.Close()
	got, err := r.ListBoots()
	if err != nil {
		t.Fatal(err)
	}
	if want := []BootInfo{bootInfo(0, 0xa0, 100, 100)}; !reflect.DeepEqual(got, want) {
		t.Fatalf("ListBoots\n got: %+v\nwant: %+v", got, want)
	}
}

// ListBoots leaves the caller's matches and read position alone.
func TestListBootsKeepsMatchesAndPosition(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeBootTestJournal(t, path, 0x40, true, []bootTestEntry{{0xa0, 100, 1}, {0xb0, 200, 1}, {0xb0, 210, 2}})
	dr, err := OpenFiles([]string{path})
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	dr.AddMatch([]byte("_BOOT_ID=" + bootTestID(0xb0).String()))
	if ok, err := dr.Step(); err != nil || !ok {
		t.Fatalf("first Step = %v, %v", ok, err)
	}
	if _, err := dr.ListBoots(); err != nil {
		t.Fatal(err)
	}
	ok, err := dr.Step()
	if err != nil || !ok {
		t.Fatalf("Step after ListBoots = %v, %v", ok, err)
	}
	entry, err := dr.GetEntry()
	if err != nil {
		t.Fatal(err)
	}
	if entry.Realtime != 210 {
		t.Fatalf("Step after ListBoots reached realtime %d, want 210", entry.Realtime)
	}
	if ok, err := dr.Step(); err != nil || ok {
		t.Fatalf("match was lost: third Step = %v, %v", ok, err)
	}
}

// Dirty systemd archives list the boots the reader sees, from a fresh reader
// (a prior full read would already have activated tail recovery).
func TestListBootsDirtyArchivesFreshReader(t *testing.T) {
	dr, err := OpenDirectory(filepath.Join("..", "..", "fixtures", "systemd", "test-data", "no-rtc"))
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	got, err := dr.ListBoots()
	if err != nil {
		t.Fatal(err)
	}
	want := []BootInfo{
		{Index: -3, BootID: "5ea5fc4f82a14186b5332a788ef9435e", FirstEntry: 1666569600994371, LastEntry: 1666584266223608},
		{Index: -2, BootID: "bea6864f21ad4c9594c04a99d89948b0", FirstEntry: 1666569601005945, LastEntry: 1666584347230411},
		{Index: -1, BootID: "4c708e1fd0744336be16f3931aa861fb", FirstEntry: 1666569601017222, LastEntry: 1666584354649355},
		{Index: 0, BootID: "35e8501129134edd9df5267c49f744a4", FirstEntry: 1666569601009823, LastEntry: 1666584438086856},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ListBoots\n got: %+v\nwant: %+v", got, want)
	}
}
