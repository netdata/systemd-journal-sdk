//go:build darwin

package journalhost

import (
	"fmt"
	"syscall"
	"unsafe"

	"github.com/netdata/systemd-journal-sdk/go/journal"
	"golang.org/x/sys/unix"
)

// loadPlatform is the macOS loader. It uses gethostuuid for the machine
// ID, kern.bootsessionuuid for the boot ID, and CLOCK_UPTIME_RAW for the
// per-entry monotonic clock.
func loadPlatform(opts LoadOptions) (*Provider, *Diagnostics, error) {
	diag := &Diagnostics{MonotonicSource: "CLOCK_UPTIME_RAW"}
	machineID, err := loadDarwinMachineID()
	if err != nil {
		return nil, nil, fmt.Errorf("journalhost: machine id: %w", err)
	}
	bootID, err := loadDarwinBootID()
	if err != nil {
		reason := fmt.Sprintf("kern.bootsessionuuid unavailable: %v", err)
		bootID, err = freshDegradedBootID(opts, reason)
		if err != nil {
			return nil, nil, fmt.Errorf("journalhost: boot id: %w", err)
		}
		diag.BootIDSource = BootIDSourceDegraded
		diag.DegradedReason = reason
	} else {
		diag.BootIDSource = BootIDSourceNative
	}
	diag.MachineIDSource = "darwin:gethostuuid"
	monotonic := func() uint64 {
		var ts unix.Timespec
		_ = unix.ClockGettime(unix.CLOCK_UPTIME_RAW, &ts)
		return timespecUsec(ts)
	}
	return &Provider{
		machineID:       machineID,
		bootID:          bootID,
		monotonicSource: monotonic,
		monotonicLabel:  "CLOCK_UPTIME_RAW",
	}, diag, nil
}

// loadDarwinMachineID uses the documented gethostuuid(3) macOS API to
// return the local host's stable hardware UUID.
func loadDarwinMachineID() (journal.UUID, error) {
	var bytes [16]byte
	var timeout syscall.Timeval
	timeout.Sec = 1
	rc, _, _ := syscall.Syscall(
		syscall.SYS_GETHOSTUUID,
		uintptr(unsafe.Pointer(&bytes[0])),
		uintptr(unsafe.Pointer(&timeout)),
		0,
	)
	if rc != 0 {
		return journal.UUID{}, fmt.Errorf("gethostuuid failed: rc=%d", rc)
	}
	if isZeroBytes(bytes[:]) {
		return journal.UUID{}, fmt.Errorf("gethostuuid returned zero bytes")
	}
	var id journal.UUID
	copy(id[:], bytes[:])
	return id, nil
}

// loadDarwinBootID reads the native kern.bootsessionuuid sysctl. The
// sysctl is read-only and exposed by XNU but is not a public Apple API;
// journald-style collectors depend on it as the standard macOS boot ID
// source. The call has no CGO dependency.
func loadDarwinBootID() (journal.UUID, error) {
	text, err := unix.Sysctl("kern.bootsessionuuid")
	if err != nil {
		return journal.UUID{}, fmt.Errorf("kern.bootsessionuuid read: %w", err)
	}
	text = trimTrailingNul(text)
	if text == "" {
		return journal.UUID{}, fmt.Errorf("kern.bootsessionuuid returned empty")
	}
	id, err := parseUUIDText(text)
	if err != nil {
		return journal.UUID{}, fmt.Errorf("kern.bootsessionuuid parse: %w", err)
	}
	return id, nil
}

func isZeroBytes(b []byte) bool {
	for _, v := range b {
		if v != 0 {
			return false
		}
	}
	return true
}
