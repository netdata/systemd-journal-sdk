package journal

import (
	"os"
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

func TestReaderJournalZstdUsesTempAccessorAndCleansUp(t *testing.T) {
	src := createReaderMessageJournal(t, 1, []Field{StringField("MESSAGE", "zst temp")})
	zstPath := filepath.Join(t.TempDir(), "generated.journal.zst")
	writeZstdFile(t, src, zstPath)

	r, err := OpenFileWithOptions(
		zstPath,
		DefaultReaderOptions().
			WithWindowSize(1024).
			WithMaxWindows(1),
	)
	if err != nil {
		t.Fatalf("OpenFileWithOptions(%s) error = %v", zstPath, err)
	}
	cleanupPath := r.cleanupPath
	if cleanupPath == "" {
		t.Fatal("cleanupPath is empty; want streamed .journal.zst temp file")
	}
	if _, err := os.Stat(cleanupPath); err != nil {
		_ = r.Close()
		t.Fatalf("temporary decompressed journal is not present: %v", err)
	}
	if ok, err := r.Step(); err != nil || !ok {
		_ = r.Close()
		t.Fatalf("Step() = %v, %v; want true, nil", ok, err)
	}
	entry, err := r.GetEntry()
	if err != nil {
		_ = r.Close()
		t.Fatalf("GetEntry() error = %v", err)
	}
	if got := string(entry.Fields["MESSAGE"]); got != "zst temp" {
		_ = r.Close()
		t.Fatalf("MESSAGE = %q, want zst temp", got)
	}
	if err := r.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	if _, err := os.Stat(cleanupPath); !os.IsNotExist(err) {
		t.Fatalf("temporary decompressed journal still exists after close: %v", err)
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
