//go:build freebsd

package journalhost

import (
	"os"

	"golang.org/x/sys/unix"
)

// acquirePlatformStateLock uses flock on FreeBSD.
func acquirePlatformStateLock(lockPath string) (func() error, error) {
	fd, err := unix.Open(lockPath, unix.O_RDWR|unix.O_CREAT|unix.O_CLOEXEC, 0o600)
	if err != nil {
		return nil, err
	}
	if err := unix.Flock(fd, unix.LOCK_EX); err != nil {
		_ = unix.Close(fd)
		return nil, err
	}
	unlock := func() error {
		err1 := unix.Flock(fd, unix.LOCK_UN)
		err2 := unix.Close(fd)
		if err1 != nil {
			return err1
		}
		return err2
	}
	return unlock, nil
}

func fsyncDirectoryBestEffort(dir string) error {
	f, err := os.Open(dir)
	if err != nil {
		return nil
	}
	defer f.Close()
	_ = f.Sync()
	return nil
}
