//go:build !unix && !windows

package journal

import (
	"fmt"
	"os"
)

func newMmapReaderBackend(*os.File) (readerWindowBackend, error) {
	return nil, fmt.Errorf("%w: mmap reader access unsupported on this platform", errInvalidJournal)
}
