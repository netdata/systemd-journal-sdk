//go:build linux

package journal

import (
	"os"
	"syscall"
)

func openNativeReaderFile(path string) (*os.File, error) {
	for {
		fd, err := syscall.Open(path, syscall.O_RDONLY|syscall.O_CLOEXEC, 0)
		if err == syscall.EINTR {
			continue
		}
		if err != nil {
			return nil, &os.PathError{Op: "open", Path: path, Err: err}
		}
		// Native journals use positioned access; a blocking descriptor avoids
		// poller setup that cannot succeed for a regular file.
		f := os.NewFile(uintptr(fd), path)
		if f == nil {
			_ = syscall.Close(fd)
			return nil, &os.PathError{Op: "open", Path: path, Err: syscall.EINVAL}
		}
		return f, nil
	}
}
