package journalhost

import (
	"fmt"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

// LoadOptions configures how the helper discovers local-host identity and
// monotonic timestamps. The zero value uses platform defaults.
type LoadOptions struct {
	// StateDir overrides the default state file directory. When empty the
	// helper uses the platform default (Windows: per-user AppData; FreeBSD
	// fallback: /var/run/systemd-journal-sdk). State is only used on
	// Windows and on FreeBSD when kern.boot_id is unavailable.
	StateDir string
	// StateFileName overrides the state file name. When empty the helper
	// uses the platform default. The default embeds the effective UID on
	// FreeBSD so multiple users share /var/run safely.
	StateFileName string
	// Now is the time source used for estimated boot time and the new-boot
	// threshold. The zero value uses time.Now.
	Now func() (sec int64, nsec int64)
	// NewUUID returns a fresh random UUID. The zero value uses crypto/rand.
	NewUUID func() (journal.UUID, error)
	// BootMarkerNow is the suspend-inclusive elapsed-since-start source
	// used for the new-boot comparison on Windows. The zero value uses
	// the platform default (GetTickCount64 on Windows, CLOCK_MONOTONIC
	// on FreeBSD). The provider instance does not read BootMarkerNow on
	// the per-entry hot path.
	BootMarkerNow func() uint64
	// MonotonicNow is the per-entry monotonic source. The zero value uses
	// the platform default (CLOCK_MONOTONIC / CLOCK_UPTIME /
	// CLOCK_UPTIME_RAW / QueryUnbiasedInterruptTime).
	MonotonicNow func() uint64
	// MonotonicLabel is a stable diagnostic label for MonotonicNow.
	// The zero value is filled in by the platform loader.
	MonotonicLabel string
	// HostFilesystemPrefix resolves Linux host identity from a mounted host
	// filesystem prefix. When set on Linux, machine-id resolution checks
	// <prefix>/etc/machine-id and <prefix>/var/lib/dbus/machine-id before
	// container-local paths. The zero value keeps default behavior unchanged.
	HostFilesystemPrefix string
}

// Load returns a Provider that returns local-host machine ID, boot ID,
// and a boot-anchored monotonic timestamp source. The returned Provider is
// safe for concurrent use.
func Load(opts LoadOptions) (*Provider, error) {
	p, diag, err := loadPlatform(opts)
	if err != nil {
		return nil, err
	}
	if diag != nil {
		p.diagnostics = *diag
	}
	monotonic := opts.MonotonicNow
	if monotonic == nil {
		monotonic = p.monotonicSource
	}
	label := opts.MonotonicLabel
	if label == "" {
		label = p.monotonicLabel
	}
	p.monotonicSource = monotonic
	p.monotonicLabel = label
	p.diagnostics.MonotonicSource = label
	p.diagnostics.MonotonicSourceDetail = label
	return p, nil
}

// MustLoad wraps Load and panics on error. Useful for tests and short
// programs that have already validated the host at startup.
func MustLoad(opts LoadOptions) *Provider {
	p, err := Load(opts)
	if err != nil {
		panic(fmt.Sprintf("journalhost: %v", err))
	}
	return p
}

func freshDegradedBootID(opts LoadOptions, reason string) (journal.UUID, error) {
	fresh, err := loadHelperNewUUID(opts)()
	if err != nil {
		return journal.UUID{}, fmt.Errorf("%s; new uuid: %w", reason, err)
	}
	return fresh, nil
}

func loadHelperRealtimeNow(opts LoadOptions) func() uint64 {
	if opts.Now != nil {
		return func() uint64 {
			sec, nsec := opts.Now()
			if sec < 0 || nsec < 0 {
				return 0
			}
			return uint64(sec)*1_000_000 + uint64(nsec)/1_000
		}
	}
	return func() uint64 {
		return uint64(time.Now().UnixMicro())
	}
}
