package journal

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// File names are bytes on Unix; the Netdata layer lists and classifies
// non-UTF-8 names, as the Rust layer does.
func TestNetdataNonUTF8JournalNamesAreListedAndClassified(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows file names are UTF-16 and do not preserve arbitrary bytes")
	}
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
	if got := journalFileExactSourceName(remote); got != "remote-h\uFFFDst" {
		t.Fatalf("exact source name = %q", got)
	}
	for raw, want := range map[string]string{
		"a\xe2\x82b":     "a\uFFFDb",       // one truncated character
		"a\xff\xfeb":     "a\uFFFD\uFFFDb", // two invalid bytes
		"a\xed\xa0b":     "a\uFFFD\uFFFDb", // surrogate lead is not a prefix
		"a\xf0\x9f\x98":  "a\uFFFD",        // truncated four-byte character
		"a\xe2\x82\xacb": "a\u20ACb",       // valid text is unchanged
	} {
		if got := lossyUTF8(raw); got != want {
			t.Fatalf("lossyUTF8(%q) = %q, want %q", raw, got, want)
		}
	}
	namespaced := filepath.Join("/var/log/journal", "machine.n\xffs", "system.journal")
	if got := journalFileExactSourceName(namespaced); got != "namespace-n\uFFFDs" {
		t.Fatalf("namespace source name = %q", got)
	}
}
