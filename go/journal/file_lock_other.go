//go:build !unix && !windows

package journal

import "os"

func lockFile(_ *os.File) error {
	return errUnsupportedFileLock
}

func unlockFile(_ *os.File) error {
	return nil
}
