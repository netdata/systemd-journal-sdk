package journal

import (
	"errors"
	"os"
)

func unlockAndClose(f *os.File) error {
	err1 := unlockFile(f)
	err2 := f.Close()
	return errors.Join(err1, err2)
}
