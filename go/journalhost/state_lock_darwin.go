//go:build darwin

package journalhost

import "os"

// acquirePlatformStateLock is not used on macOS. The state-backed path
// is the FreeBSD fallback and Windows primary; macOS uses
// kern.bootsessionuuid natively.
func acquirePlatformStateLock(_ string) (func() error, error) {
	return func() error { return nil }, nil
}

func fsyncDirectoryBestEffort(dir string) error {
	f, err := os.Open(dir)
	if err != nil {
		return nil
	}
	defer f.Close()
	_ = f.Sync()
	return nil
}
