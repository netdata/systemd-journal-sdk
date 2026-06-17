//go:build linux

package journalhost

import (
	"fmt"
	"os"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
	"golang.org/x/sys/unix"
)

// loadPlatform is the Linux loader. It reads machine ID, native boot ID,
// and CLOCK_MONOTONIC.
func loadPlatform(opts LoadOptions) (*Provider, *Diagnostics, error) {
	diag := &Diagnostics{
		MachineIDSource: "linux",
		MonotonicSource: "CLOCK_MONOTONIC",
	}
	machineID, err := loadLinuxMachineID()
	if err != nil {
		return nil, nil, fmt.Errorf("journalhost: machine id: %w", err)
	}
	bootID, err := loadLinuxBootID()
	if err != nil {
		reason := fmt.Sprintf("linux boot_id unavailable: %v", err)
		bootID, err = freshDegradedBootID(opts, reason)
		if err != nil {
			return nil, nil, fmt.Errorf("journalhost: boot id: %w", err)
		}
		diag.BootIDSource = BootIDSourceDegraded
		diag.DegradedReason = reason
	} else {
		diag.BootIDSource = BootIDSourceNative
	}
	monotonic := func() uint64 {
		var ts unix.Timespec
		_ = unix.ClockGettime(unix.CLOCK_MONOTONIC, &ts)
		return timespecUsec(ts)
	}
	return &Provider{
		machineID:       machineID,
		bootID:          bootID,
		monotonicSource: monotonic,
		monotonicLabel:  "CLOCK_MONOTONIC",
	}, diag, nil
}

func loadLinuxMachineID() (journal.UUID, error) {
	for _, path := range []string{"/etc/machine-id", "/var/lib/dbus/machine-id"} {
		id, err := readMachineIDFile(path)
		if err == nil {
			return id, nil
		}
	}
	return journal.UUID{}, fmt.Errorf("no machine id found")
}

func readMachineIDFile(path string) (journal.UUID, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return journal.UUID{}, err
	}
	text := strings.TrimSpace(string(data))
	return parseUUIDText(text)
}

func loadLinuxBootID() (journal.UUID, error) {
	data, err := os.ReadFile("/proc/sys/kernel/random/boot_id")
	if err != nil {
		return journal.UUID{}, err
	}
	return parseUUIDText(strings.TrimSpace(string(data)))
}
