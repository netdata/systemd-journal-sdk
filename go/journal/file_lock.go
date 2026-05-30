package journal

import (
	"errors"
)

var errUnsupportedFileLock = errors.New("journal writer file locking is unsupported on this platform")
