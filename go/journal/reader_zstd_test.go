package journal

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestReaderSystemdZstdFixture(t *testing.T) {
	path := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "no-rtc", "system.journal.zst")
	r := mustOpenReaderFile(t, path)
	defer r.Close()

	count, sawTransport := scanReaderZstdFixture(t, r, 100)
	if count == 0 {
		t.Fatal("systemd fixture produced no entries")
	}
	if !sawTransport {
		t.Fatal("systemd fixture did not expose _TRANSPORT in first 100 entries")
	}
}

func scanReaderZstdFixture(t *testing.T, r *Reader, limit int) (int, bool) {
	t.Helper()
	count := 0
	var sawTransport bool
	for count < limit {
		ok, err := r.Step()
		if err != nil {
			t.Fatalf("Step error: %v", err)
		}
		if !ok {
			return count, sawTransport
		}
		entry, err := r.GetEntry()
		if err != nil {
			t.Fatalf("GetEntry error: %v", err)
		}
		sawTransport = sawTransport || string(entry.Fields["_TRANSPORT"]) != ""
		if count == 0 {
			assertFirstZstdEntry(t, entry)
		}
		count++
	}
	return count, sawTransport
}

func assertFirstZstdEntry(t *testing.T, entry *Entry) {
	t.Helper()
	if got := string(entry.Fields["_TRANSPORT"]); got != "kernel" {
		t.Fatalf("first _TRANSPORT = %q, want kernel", got)
	}
	if got := string(entry.Fields["MESSAGE"]); !strings.HasPrefix(got, "Booting Linux") {
		t.Fatalf("first MESSAGE = %q, want Booting Linux prefix", got)
	}
}
