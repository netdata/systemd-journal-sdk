//go:build unix

package journal

import (
	"errors"
	"os"
	"path/filepath"
)

func syncParentDir(path string) error {
	dir := path
	if info, err := os.Stat(path); err == nil && !info.IsDir() {
		dir = filepath.Dir(path)
	}
	f, err := os.Open(dir)
	if err != nil {
		return err
	}
	err1 := f.Sync()
	err2 := f.Close()
	return errors.Join(err1, err2)
}
