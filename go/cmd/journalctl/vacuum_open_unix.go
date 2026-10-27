//go:build cgo && unix

package main

import (
	"os"

	"golang.org/x/sys/unix"
)

func openVacuumCandidate(path string) (*os.File, error) {
	flags := unix.O_RDONLY | unix.O_CLOEXEC | unix.O_NOFOLLOW | unix.O_NONBLOCK
	fd, err := unix.Open(path, flags|vacuumNoAtime, 0)
	if err != nil && vacuumNoAtime != 0 {
		// Native retries when preserving atime is unavailable to the caller.
		fd, err = unix.Open(path, flags, 0)
	}
	if err != nil {
		return nil, err
	}
	return os.NewFile(uintptr(fd), path), nil
}
