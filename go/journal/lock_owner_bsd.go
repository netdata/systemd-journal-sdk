//go:build darwin || freebsd

package journal

import (
	"context"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const bsdProcessStartLookupTimeout = 2 * time.Second

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
	ctx, cancel := context.WithTimeout(context.Background(), bsdProcessStartLookupTimeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, "ps", "-o", "lstart=", "-p", strconv.Itoa(pid))
	cmd.Env = bsdProcessStartEnv()
	output, err := cmd.Output()
	if ctx.Err() != nil {
		return "", fmt.Errorf("process start time lookup timed out for pid %d: %w", pid, ctx.Err())
	}
	if err != nil {
		return "", err
	}
	start := strings.TrimSpace(string(output))
	if start == "" {
		return "", fmt.Errorf("cannot read process start time for pid %d", pid)
	}
	return start, nil
}

func bsdProcessStartEnv() []string {
	env := os.Environ()
	out := env[:0]
	for _, entry := range env {
		if strings.HasPrefix(entry, "LC_ALL=") {
			continue
		}
		out = append(out, entry)
	}
	return append(out, "LC_ALL=C")
}
