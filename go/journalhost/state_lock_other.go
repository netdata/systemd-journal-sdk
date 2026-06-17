//go:build !linux && !freebsd && !darwin && !windows

package journalhost

// acquirePlatformStateLock is a degraded fallback for unsupported targets.
func acquirePlatformStateLock(_ string) (func() error, error) {
	return func() error { return nil }, nil
}

func fsyncDirectoryBestEffort(_ string) error {
	return nil
}
