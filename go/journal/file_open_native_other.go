//go:build !linux

package journal

import "os"

func openNativeReaderFile(path string) (*os.File, error) {
	return openReaderFile(path)
}
