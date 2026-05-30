package journal

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

const (
	lockVersion       = "systemd-journal-sdk-lock-v1"
	lockStaleGrace    = 2 * time.Second
	lockFilePerm      = 0o600
	lockDirectoryPerm = 0o750
)

type writerLock struct {
	path  string
	owner lockOwner
}

// WriterLock is an optional cooperating-writer lock helper.
//
// The journal file format does not define a lock protocol. Callers that want
// SDK-to-SDK writer exclusion can acquire this helper before opening a Writer
// and release it after closing that Writer.
type WriterLock struct {
	lock *writerLock
}

// AcquireWriterLock acquires the optional cooperating-writer lock for path.
func AcquireWriterLock(journalPath string) (*WriterLock, error) {
	lock, err := acquireWriterLock(journalPath)
	if err != nil {
		return nil, err
	}
	return &WriterLock{lock: lock}, nil
}

// Release releases the optional cooperating-writer lock.
func (l *WriterLock) Release() error {
	if l == nil || l.lock == nil {
		return nil
	}
	err := l.lock.release()
	l.lock = nil
	return err
}

type lockOwner struct {
	PID       int
	BootID    string
	StartTime string
}

func acquireWriterLock(journalPath string) (*writerLock, error) {
	lockPath := journalPath + ".lock"
	owner, err := currentLockOwner()
	if err != nil {
		return nil, err
	}

	for {
		if err := os.MkdirAll(filepath.Dir(lockPath), lockDirectoryPerm); err != nil {
			return nil, err
		}
		f, err := os.OpenFile(lockPath, os.O_WRONLY|os.O_CREATE|os.O_EXCL, lockFilePerm)
		if err == nil {
			lock := &writerLock{path: lockPath, owner: owner}
			if err := writeLockOwner(f, owner); err != nil {
				_ = f.Close()
				_ = os.Remove(lockPath)
				return nil, err
			}
			if err := f.Close(); err != nil {
				_ = os.Remove(lockPath)
				return nil, err
			}
			return lock, nil
		}
		if !errors.Is(err, os.ErrExist) {
			return nil, err
		}

		stale, holder := lockFileIsStale(lockPath)
		if !stale {
			return nil, fmt.Errorf("journal writer lock held by %s", holder)
		}
		if err := os.Remove(lockPath); err != nil && !errors.Is(err, os.ErrNotExist) {
			return nil, err
		}
	}
}

func writeLockOwner(f *os.File, owner lockOwner) error {
	text := fmt.Sprintf("%s\npid=%d\nboot_id=%s\nstart_time=%s\n", lockVersion, owner.PID, owner.BootID, owner.StartTime)
	if _, err := f.WriteString(text); err != nil {
		return err
	}
	return f.Sync()
}

func (l *writerLock) release() error {
	if l == nil || l.path == "" {
		return nil
	}
	current, err := currentLockOwner()
	if err != nil {
		return err
	}
	owner, err := readLockOwner(l.path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			l.path = ""
			return nil
		}
		return err
	}
	if owner == current {
		err = os.Remove(l.path)
		if err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
	}
	l.path = ""
	return nil
}

func lockFileIsStale(path string) (bool, string) {
	owner, err := readLockOwner(path)
	if err != nil {
		info, statErr := os.Stat(path)
		if statErr == nil && time.Since(info.ModTime()) <= lockStaleGrace {
			return false, "partially-created lock"
		}
		return true, "malformed stale lock"
	}
	if owner.BootID != currentBootID() {
		return true, fmt.Sprintf("pid %d from previous boot", owner.PID)
	}
	start, err := processStartTime(owner.PID)
	if err != nil || start != owner.StartTime {
		return true, fmt.Sprintf("stale pid %d", owner.PID)
	}
	return false, fmt.Sprintf("pid %d", owner.PID)
}

func currentLockOwner() (lockOwner, error) {
	pid := os.Getpid()
	start, err := processStartTime(pid)
	if err != nil {
		return lockOwner{}, err
	}
	return lockOwner{PID: pid, BootID: currentBootID(), StartTime: start}, nil
}

func readLockOwner(path string) (lockOwner, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return lockOwner{}, err
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) < 4 || lines[0] != lockVersion {
		return lockOwner{}, fmt.Errorf("invalid lock metadata")
	}
	var owner lockOwner
	for _, line := range lines[1:] {
		key, value, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		switch key {
		case "pid":
			pid, err := strconv.Atoi(value)
			if err != nil {
				return lockOwner{}, err
			}
			owner.PID = pid
		case "boot_id":
			owner.BootID = value
		case "start_time":
			owner.StartTime = value
		}
	}
	if owner.PID <= 0 || owner.StartTime == "" {
		return lockOwner{}, fmt.Errorf("incomplete lock metadata")
	}
	return owner, nil
}
