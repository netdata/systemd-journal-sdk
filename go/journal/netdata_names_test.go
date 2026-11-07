package journal

import (
	"os"
	"path/filepath"
	"testing"
)

// File names are bytes on Unix; the Netdata layer lists and classifies
// non-UTF-8 names, as the Rust layer does.
func TestNetdataNonUTF8JournalNamesAreListedAndClassified(t *testing.T) {
	machine := filepath.Join(t.TempDir(), "machine")
	if err := os.Mkdir(machine, 0o755); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(machine, "system\xff.journal")
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Skipf("filesystem rejects non-UTF-8 names: %v", err)
	}
	collection, err := collectNetdataJournalFiles(filepath.Dir(machine))
	if err != nil {
		t.Fatal(err)
	}
	if len(collection.Files) != 1 || collection.Files[0] != path {
		t.Fatalf("files = %q, want [%q]", collection.Files, path)
	}
	want := uint64(netdataSourceTypeAll | netdataSourceTypeLocalAll | netdataSourceTypeLocalSystem)
	if got := journalFileSourceType(path); got != want {
		t.Fatalf("source type = %d, want %d", got, want)
	}
	remote := "/var/log/journal/remote/remote-h\xffst@x.journal"
	if got := journalFileExactSourceName(remote); got != "remote-h\xffst" {
		t.Fatalf("exact source name = %q", got)
	}
}
