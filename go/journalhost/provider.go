package journalhost

import (
	"fmt"
	"sync"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

// BootIDSource classifies how the boot ID was obtained.
type BootIDSource int

const (
	// BootIDSourceUnknown is the zero value before Load returns.
	BootIDSourceUnknown BootIDSource = iota
	// BootIDSourceNative means a kernel/native boot UUID was read.
	BootIDSourceNative
	// BootIDSourceStateBacked means a synthesized boot ID was read from a
	// healthy helper state file.
	BootIDSourceStateBacked
	// BootIDSourceDegraded means a fresh boot ID was generated because
	// state or native discovery failed. Degraded boot IDs are valid
	// 128-bit UUIDs but the helper does not claim cross-process
	// same-boot stability for them.
	BootIDSourceDegraded
)

// String returns a stable diagnostic label.
func (s BootIDSource) String() string {
	switch s {
	case BootIDSourceNative:
		return "native"
	case BootIDSourceStateBacked:
		return "state-backed"
	case BootIDSourceDegraded:
		return "degraded"
	default:
		return "unknown"
	}
}

// Diagnostics describes how each helper value was obtained.
type Diagnostics struct {
	// MachineIDSource describes how the machine ID was obtained.
	MachineIDSource string
	// BootIDSource classifies the boot ID source.
	BootIDSource BootIDSource
	// BootIDPath is the state file path used for state-backed boot IDs.
	// Empty for native boot IDs.
	BootIDPath string
	// MonotonicSource describes the monotonic clock source.
	MonotonicSource string
	// MonotonicSourceDetail is platform-specific source detail
	// (for example "CLOCK_MONOTONIC" or "QueryUnbiasedInterruptTime").
	MonotonicSourceDetail string
	// DegradedReason records the most recent state or discovery failure
	// that forced a degraded boot ID. Empty when not degraded.
	DegradedReason string
}

// Provider returns local-host values for callers that intentionally want
// the collector host as the event identity source. A Provider is safe to
// use from multiple goroutines, but the monotonic_usec() call is intended
// to be cheap so each goroutine can call it independently.
type Provider struct {
	machineID   journal.UUID
	bootID      journal.UUID
	diagnostics Diagnostics

	monotonicSource func() uint64
	monotonicLabel  string

	mu            sync.Mutex
	lastMonotonic uint64
}

// MachineID returns the local host machine ID.
func (p *Provider) MachineID() journal.UUID {
	return p.machineID
}

// BootID returns the local host boot ID. The returned UUID is stable for
// the OS boot when Diagnostics.BootIDSource is Native or StateBacked. When
// BootIDSource is Degraded the helper does not claim cross-process
// stability.
func (p *Provider) BootID() journal.UUID {
	return p.bootID
}

// MonotonicUsec returns a per-entry boot-anchored monotonic microsecond
// timestamp. The same Provider instance guarantees strictly increasing
// values across calls.
func (p *Provider) MonotonicUsec() uint64 {
	now := p.monotonicSource()
	p.mu.Lock()
	defer p.mu.Unlock()
	if now <= p.lastMonotonic {
		now = p.lastMonotonic + 1
	}
	p.lastMonotonic = now
	return now
}

// RealtimeUsec returns the current wall-clock microsecond timestamp.
func (p *Provider) RealtimeUsec() uint64 {
	return uint64(time.Now().UnixMicro())
}

// EntryOptions returns an EntryOptions value the caller can pass directly
// to journal.Writer.Append / journal.Log.Append. BootID, MonotonicUsec, and
// RealtimeUsec are filled in. MonotonicUsecSet is set so the core writer
// records the helper value even when the underlying monotonic clock would
// otherwise produce zero.
func (p *Provider) EntryOptions() journal.EntryOptions {
	return journal.EntryOptions{
		RealtimeUsec:     p.RealtimeUsec(),
		RealtimeUsecSet:  true,
		MonotonicUsec:    p.MonotonicUsec(),
		MonotonicUsecSet: true,
		BootID:           p.BootID(),
	}
}

// Diagnostics returns the source diagnostics for the helper.
func (p *Provider) Diagnostics() Diagnostics {
	return p.diagnostics
}

// MonotonicSource returns a stable label describing the monotonic clock
// source ("CLOCK_MONOTONIC", "CLOCK_UPTIME", "CLOCK_UPTIME_RAW",
// "QueryUnbiasedInterruptTime", or a platform-specific equivalent).
func (p *Provider) MonotonicSource() string {
	return p.monotonicLabel
}

// String returns a one-line diagnostic summary.
func (p *Provider) String() string {
	return fmt.Sprintf(
		"journalhost{machine_id_source=%s boot_id_source=%s monotonic=%s degraded=%t}",
		p.diagnostics.MachineIDSource,
		p.diagnostics.BootIDSource,
		p.monotonicLabel,
		p.diagnostics.BootIDSource == BootIDSourceDegraded,
	)
}
