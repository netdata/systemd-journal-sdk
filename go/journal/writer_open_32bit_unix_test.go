//go:build unix && (386 || arm || mips || mipsle)

package journal

import (
	"bytes"
	"errors"
	"os"
	"testing"
	"time"
)

func TestRootRetentionOversizedRetiredFileLeavesWriterHealthy(t *testing.T) {
	dir := t.TempDir()
	log := rootOpen(t, dir, rootTestConfig())
	rootAppend(t, log, uint64(time.Now().UnixMicro()))
	path := rootFixture(t, dir, 23, []uint64{1}, true)
	// A sparse file keeps valid journal metadata but exceeds a 32-bit mapping.
	const fileSize = int64(1 << 31)
	if err := os.Truncate(path, fileSize); err != nil {
		t.Fatal(err)
	}
	file, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	before := make([]byte, headerSize)
	if _, err := file.ReadAt(before, 0); err != nil {
		t.Fatal(err)
	}
	_, err = log.MaintainRootRetention(time.Now())
	if !errors.Is(err, errInvalidJournal) || errors.Is(err, ErrWriterFailed) {
		t.Fatalf("nonmutating mapping size rejection: %v", err)
	}
	after := make([]byte, headerSize)
	if _, err := file.ReadAt(after, 0); err != nil || !bytes.Equal(before, after) {
		t.Fatalf("size rejection changed header: %v", err)
	}
	if info, err := file.Stat(); err != nil || info.Size() != fileSize {
		t.Fatalf("size rejection changed file length: %v", err)
	}
	rootAppend(t, log, uint64(time.Now().UnixMicro()))
	if err := log.Sync(); err != nil {
		t.Fatal(err)
	}
}
