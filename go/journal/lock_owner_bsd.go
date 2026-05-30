//go:build darwin || freebsd

package journal

import (
	"encoding/hex"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"syscall"
)

func currentBootID() string {
	bootTime, err := syscall.Sysctl("kern.boottime")
	if err != nil || bootTime == "" {
		return "bsd"
	}
	return "bsd:" + hex.EncodeToString([]byte(bootTime))
}

func processStartTime(pid int) (string, error) {
	if pid <= 0 {
		return "", fmt.Errorf("invalid pid %d", pid)
	}
	output, err := exec.Command("ps", "-o", "lstart=", "-p", strconv.Itoa(pid)).Output()
	if err != nil {
		return "", err
	}
	start := strings.TrimSpace(string(output))
	if start == "" {
		return "", fmt.Errorf("cannot read process start time for pid %d", pid)
	}
	return start, nil
}
