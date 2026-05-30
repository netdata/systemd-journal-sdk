//go:build linux

package journal

import (
	"fmt"
	"os"
	"strings"
)

func currentBootID() string {
	data, err := os.ReadFile("/proc/sys/kernel/random/boot_id")
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}

func processStartTime(pid int) (string, error) {
	data, err := os.ReadFile(fmt.Sprintf("/proc/%d/stat", pid))
	if err != nil {
		return "", err
	}
	text := string(data)
	end := strings.LastIndexByte(text, ')')
	if end < 0 || end+2 >= len(text) {
		return "", fmt.Errorf("cannot parse /proc/%d/stat", pid)
	}
	fields := strings.Fields(text[end+2:])
	if len(fields) < 20 {
		return "", fmt.Errorf("cannot parse start time from /proc/%d/stat", pid)
	}
	return fields[19], nil
}
