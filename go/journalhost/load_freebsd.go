//go:build freebsd

package journalhost

import (
	"fmt"

	"github.com/netdata/systemd-journal-sdk/go/journal"
	"golang.org/x/sys/unix"
)

// loadPlatform is the FreeBSD loader. It reads kern.hostuuid for the
// machine ID, kern.boot_id for the boot ID (FreeBSD 13+), and
// CLOCK_UPTIME for the per-entry monotonic clock. When kern.boot_id is
// unavailable (FreeBSD 12 or stripped kernels) the helper falls back to
// a state-backed synthesis.
func loadPlatform(opts LoadOptions) (*Provider, *Diagnostics, error) {
	diag := &Diagnostics{MonotonicSource: "CLOCK_UPTIME"}
	machineID, err := loadFreeBSDHostUUID()
	if err != nil {
		return nil, nil, fmt.Errorf("journalhost: machine id: %w", err)
	}
	bootID, bootSource, bootPath, degraded, err := loadFreeBSDBootID(opts)
	if err != nil && isZeroUUID(bootID) {
		return nil, nil, fmt.Errorf("journalhost: boot id: %w", err)
	}
	diag.BootIDSource = bootSource
	diag.BootIDPath = bootPath
	if degraded || err != nil {
		diag.DegradedReason = degradedReasonFromError(err)
	}
	diag.MachineIDSource = "freebsd:kern.hostuuid"
	monotonic := func() uint64 {
		var ts unix.Timespec
		_ = unix.ClockGettime(unix.CLOCK_UPTIME, &ts)
		return timespecUsec(ts)
	}
	return &Provider{
		machineID:       machineID,
		bootID:          bootID,
		monotonicSource: monotonic,
		monotonicLabel:  "CLOCK_UPTIME",
	}, diag, nil
}

// loadFreeBSDHostUUID reads kern.hostuuid and rejects the all-zero jail
// sentinel that some FreeBSD jails expose.
func loadFreeBSDHostUUID() (journal.UUID, error) {
	text, err := unix.Sysctl("kern.hostuuid")
	if err != nil {
		return journal.UUID{}, fmt.Errorf("kern.hostuuid read: %w", err)
	}
	text = trimTrailingNul(text)
	if text == "" {
		return journal.UUID{}, fmt.Errorf("kern.hostuuid empty")
	}
	id, err := parseUUIDText(text)
	if err != nil {
		return journal.UUID{}, fmt.Errorf("kern.hostuuid parse: %w", err)
	}
	if isZeroUUID(id) {
		return journal.UUID{}, fmt.Errorf("kern.hostuuid is all zeros")
	}
	return id, nil
}

// loadFreeBSDBootID returns (bootID, source, path, degraded, err).
// source is one of BootIDSourceNative, BootIDSourceStateBacked,
// BootIDSourceDegraded. err is non-nil only when degraded state also
// failed; native and healthy state-backed paths return nil err.
func loadFreeBSDBootID(opts LoadOptions) (journal.UUID, BootIDSource, string, bool, error) {
	if id, ok := tryFreeBSDKernBootID(); ok {
		return id, BootIDSourceNative, "", false, nil
	}
	id, path, degraded, err := loadStateBackedBootID(opts, stateBackedProbe{
		markerNow: func() uint64 {
			var ts unix.Timespec
			_ = unix.ClockGettime(unix.CLOCK_MONOTONIC, &ts)
			return timespecUsec(ts)
		},
		realtimeNow:     loadHelperRealtimeNow(opts),
		defaultStateDir: defaultFreeBSDStateDir(),
		defaultFileName: defaultFreeBSDStateFileName(),
	})
	if err != nil {
		return id, BootIDSourceDegraded, path, true, err
	}
	if degraded {
		return id, BootIDSourceDegraded, path, true, nil
	}
	return id, BootIDSourceStateBacked, path, false, nil
}

// tryFreeBSDKernBootID returns the native kern.boot_id sysctl on
// FreeBSD 13+ or (id, false) on older kernels / stripped configurations.
func tryFreeBSDKernBootID() (journal.UUID, bool) {
	buf, err := unix.SysctlRaw("kern.boot_id")
	if err != nil || len(buf) < 16 {
		return journal.UUID{}, false
	}
	var id journal.UUID
	copy(id[:], buf[:16])
	if isZeroUUID(id) {
		return journal.UUID{}, false
	}
	return id, true
}

func defaultFreeBSDStateDir() string {
	return "/var/run/systemd-journal-sdk"
}

func defaultFreeBSDStateFileName() string {
	uid := unix.Geteuid()
	return fmt.Sprintf("bootid.%d.state", uid)
}

func degradedReasonFromError(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}
