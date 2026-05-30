package journal

import (
	"errors"
	"os"
)

var errUnsupportedFileLock = errors.New("journal writer file locking is unsupported on this platform")

func unlockAndClose(f *os.File) error {
	err1 := unlockFile(f)
	err2 := f.Close()
	return errors.Join(err1, err2)
}
