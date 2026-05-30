package journal

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLockFileIsStaleCurrentOwner(t *testing.T) {
	path := filepath.Join(t.TempDir(), "current.lock")
	owner, err := currentLockOwner()
	if err != nil {
		t.Fatalf("currentLockOwner() error = %v", err)
	}
	writeTestLockOwner(t, path, owner)

	stale, holder := lockFileIsStale(path)
	if stale {
		t.Fatalf("lockFileIsStale() = stale, holder %q; want current owner", holder)
	}
}

func TestLockFileIsStaleDeadPID(t *testing.T) {
	path := filepath.Join(t.TempDir(), "dead.lock")
	owner, err := currentLockOwner()
	if err != nil {
		t.Fatalf("currentLockOwner() error = %v", err)
	}
	owner.PID = 1 << 30
	owner.StartTime = "dead-process-token"
	writeTestLockOwner(t, path, owner)

	stale, holder := lockFileIsStale(path)
	if !stale {
		t.Fatalf("lockFileIsStale() = not stale, holder %q; want stale", holder)
	}
}

func writeTestLockOwner(t *testing.T, path string, owner lockOwner) {
	t.Helper()
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, lockFilePerm)
	if err != nil {
		t.Fatalf("OpenFile(%q) error = %v", path, err)
	}
	if err := writeLockOwner(f, owner); err != nil {
		_ = f.Close()
		t.Fatalf("writeLockOwner() error = %v", err)
	}
	if err := f.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}
