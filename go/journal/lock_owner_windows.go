//go:build windows

package journal

import (
	"fmt"
	"syscall"
)

const processQueryLimitedInformation = 0x1000

func currentBootID() string {
	return "windows"
}

func processStartTime(pid int) (string, error) {
	if pid <= 0 {
		return "", fmt.Errorf("invalid pid %d", pid)
	}
	handle, err := syscall.OpenProcess(processQueryLimitedInformation, false, uint32(pid))
	if err != nil {
		return "", err
	}
	defer syscall.CloseHandle(handle)

	var creationTime syscall.Filetime
	var exitTime syscall.Filetime
	var kernelTime syscall.Filetime
	var userTime syscall.Filetime
	if err := syscall.GetProcessTimes(handle, &creationTime, &exitTime, &kernelTime, &userTime); err != nil {
		return "", err
	}
	return fmt.Sprintf("%08x%08x", creationTime.HighDateTime, creationTime.LowDateTime), nil
}
