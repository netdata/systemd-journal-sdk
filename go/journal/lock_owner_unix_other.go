//go:build unix && !linux && !darwin && !freebsd

package journal

import (
	"errors"
	"fmt"
	"os"
	"syscall"
)

func currentBootID() string {
	return "unix"
}

func processStartTime(pid int) (string, error) {
	if pid <= 0 {
		return "", fmt.Errorf("invalid pid %d", pid)
	}
	process, err := os.FindProcess(pid)
	if err != nil {
		return "", err
	}
	defer process.Release()
	err = process.Signal(syscall.Signal(0))
	if err == nil || errors.Is(err, syscall.EPERM) {
		return "unknown", nil
	}
	return "", err
}
