//go:build windows

package journal

import (
	"os"
	"syscall"
	"unsafe"
)

const (
	lockFileExclusiveLock   = 0x00000002
	lockFileFailImmediately = 0x00000001
	// Lock far beyond valid journal sizes so readers can read journal bytes.
	lockFileOffsetHigh = 0x80000000
)

var (
	kernel32             = syscall.NewLazyDLL("kernel32.dll")
	kernel32LockFileEx   = kernel32.NewProc("LockFileEx")
	kernel32UnlockFileEx = kernel32.NewProc("UnlockFileEx")
)

func lockFile(f *os.File) error {
	return callLockFileEx(f, lockFileExclusiveLock|lockFileFailImmediately)
}

func unlockFile(f *os.File) error {
	var overlapped syscall.Overlapped
	overlapped.OffsetHigh = lockFileOffsetHigh
	r1, _, err := kernel32UnlockFileEx.Call(
		f.Fd(),
		0,
		1,
		0,
		uintptr(unsafe.Pointer(&overlapped)),
	)
	if r1 == 0 {
		if err != syscall.Errno(0) {
			return err
		}
		return syscall.EINVAL
	}
	return nil
}

func callLockFileEx(f *os.File, flags uint32) error {
	var overlapped syscall.Overlapped
	overlapped.OffsetHigh = lockFileOffsetHigh
	r1, _, err := kernel32LockFileEx.Call(
		f.Fd(),
		uintptr(flags),
		0,
		1,
		0,
		uintptr(unsafe.Pointer(&overlapped)),
	)
	if r1 == 0 {
		if err != syscall.Errno(0) {
			return err
		}
		return syscall.EINVAL
	}
	return nil
}
