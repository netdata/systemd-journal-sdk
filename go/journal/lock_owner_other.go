//go:build !unix && !windows

package journal

import "fmt"

func currentBootID() string {
	return "unknown"
}

func processStartTime(pid int) (string, error) {
	if pid <= 0 {
		return "", fmt.Errorf("invalid pid %d", pid)
	}
	return "unknown", nil
}
