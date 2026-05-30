//go:build !unix && !windows

package journal

import "os"

func openWriterFile(path string, create bool, perm os.FileMode) (*os.File, error) {
	flags := os.O_RDWR
	if create {
		flags |= os.O_CREATE
	}
	return os.OpenFile(path, flags, perm)
}

func openReaderFile(path string) (*os.File, error) {
	return os.Open(path)
}
