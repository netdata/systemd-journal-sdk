package journalhost

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

// newBootIDUUID returns a fresh random UUID. crypto/rand is the only
// source; the helper never derives boot IDs from wall clock or process
// state.
func newBootIDUUID() (journal.UUID, error) {
	return journal.NewUUID()
}

// stateBackedProbe groups the injected sources the state-backed path
// needs. When a field is zero, loadStateBackedBootID uses platform
// defaults.
type stateBackedProbe struct {
	markerNow       func() uint64
	realtimeNow     func() uint64
	newUUID         func() (journal.UUID, error)
	defaultStateDir string
	defaultFileName string
}

// stateBackedResult is what loadStateBackedBootID returns.
type stateBackedResult struct {
	id       journal.UUID
	path     string
	degraded bool
	freshErr error
}

// loadStateBackedBootID implements the locked, atomic state file
// contract for the Windows primary path and the FreeBSD fallback. It
// returns the current boot ID and a path the caller can echo in
// diagnostics.
//
// Behaviour:
//   - Locked initialization serializes cross-process read/write.
//   - Same boot does not rewrite the state file.
//   - New boot (marker > last + 30s) writes new state.
//   - Missing/corrupt state writes a clean state when possible and
//     preserves a .corrupt copy when safe.
//   - Any open/lock/read/parse/copy/write/fsync/rename/permission
//     failure returns degraded=true and a fresh boot ID for this
//     provider instance.
func loadStateBackedBootID(opts LoadOptions, probe stateBackedProbe) (journal.UUID, string, bool, error) {
	if probe.newUUID == nil {
		probe.newUUID = loadHelperNewUUID(opts)
	}
	if opts.BootMarkerNow != nil {
		probe.markerNow = opts.BootMarkerNow
	}
	if opts.Now != nil {
		probe.realtimeNow = loadHelperRealtimeNow(opts)
	}
	probe = fillStateBackedProbe(probe)
	stateDir := opts.StateDir
	if stateDir == "" {
		stateDir = probe.defaultStateDir
	}
	stateName := opts.StateFileName
	if stateName == "" {
		stateName = probe.defaultFileName
	}
	path := filepath.Join(stateDir, stateName)
	lockPath := path + ".lock"
	corruptPath := path + ".corrupt"

	// Best-effort state directory creation with 0700 permissions.
	if err := os.MkdirAll(stateDir, 0o700); err != nil {
		// Cannot create state dir; fall through to degraded path.
		return degradedStateBackedResult(path, probe, fmt.Errorf("mkdir state dir: %w", err))
	}

	// Take an exclusive lock. We use a sidecar file with a flock-style
	// advisory lock when supported by the platform. The lock acquisition
	// failure path also degrades.
	unlock, lockErr := acquireStateLock(lockPath)
	if lockErr != nil {
		return degradedStateBackedResult(path, probe, fmt.Errorf("acquire lock: %w", lockErr))
	}
	defer func() { _ = unlock() }()

	// Read existing state if present.
	existing, err := readStateFile(path)
	if err != nil {
		// Try to preserve a corrupt copy before bailing.
		if !os.IsNotExist(err) {
			_ = preserveCorruptState(path, corruptPath)
		}
		return writeFreshStateBacked(path, probe, fmt.Errorf("read state: %w", err))
	}
	if existing == nil {
		// No state file.
		return writeFreshStateBacked(path, probe, nil)
	}
	estimatedBoottime := estimateBoottimeUsec(probe)
	if estimatedBoottime > existing.estimatedBoottime+30*1_000_000 {
		// New boot detected.
		return writeFreshStateBacked(path, probe, nil)
	}
	// Same boot.
	return existing.bootID, path, false, nil
}

type stateFileContent struct {
	estimatedBoottime uint64
	bootID            journal.UUID
}

func readStateFile(path string) (*stateFileContent, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	text := strings.TrimSpace(string(data))
	lines := strings.Split(text, "\n")
	var boottime uint64
	var bootID journal.UUID
	var sawBoottime, sawBootID bool
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		if strings.HasPrefix(line, "last_estimated_boottime=") {
			v, perr := strconv.ParseUint(strings.TrimPrefix(line, "last_estimated_boottime="), 10, 64)
			if perr != nil {
				return nil, fmt.Errorf("parse boottime: %w", perr)
			}
			boottime = v
			sawBoottime = true
		} else if strings.HasPrefix(line, "last_boot_id=") {
			raw := strings.TrimPrefix(line, "last_boot_id=")
			id, perr := parseUUIDText(raw)
			if perr != nil {
				return nil, fmt.Errorf("parse boot id: %w", perr)
			}
			bootID = id
			sawBootID = true
		} else {
			return nil, fmt.Errorf("unknown field %q", line)
		}
	}
	if !sawBoottime || !sawBootID {
		return nil, fmt.Errorf("missing required fields")
	}
	if isZeroUUID(bootID) {
		return nil, fmt.Errorf("boot id is all zeros")
	}
	return &stateFileContent{estimatedBoottime: boottime, bootID: bootID}, nil
}

func writeFreshStateBacked(path string, probe stateBackedProbe, cause error) (journal.UUID, string, bool, error) {
	fresh, err := probe.newUUID()
	if err != nil {
		// If we cannot generate a UUID at all, fail hard because we
		// cannot satisfy the strict writer contract.
		return journal.UUID{}, path, true, fmt.Errorf("new uuid: %w", err)
	}
	estimatedBoottime := estimateBoottimeUsec(probe)
	contents := fmt.Sprintf(
		"last_estimated_boottime=%d\nlast_boot_id=%s\n",
		estimatedBoottime,
		fresh.String(),
	)
	if err := writeStateFileAtomic(path, []byte(contents)); err != nil {
		// Could not write clean state; still return fresh and mark
		// degraded. The strict writer contract receives a valid
		// 128-bit UUID.
		return fresh, path, true, err
	}
	if cause != nil {
		// Caller passed an error from a prior read. Still return the
		// fresh boot ID but signal degraded.
		return fresh, path, true, cause
	}
	return fresh, path, false, nil
}

func estimateBoottimeUsec(probe stateBackedProbe) uint64 {
	nowRealtime := probe.realtimeNow()
	markerNow := probe.markerNow()
	if markerNow > nowRealtime {
		return 0
	}
	return nowRealtime - markerNow
}

func writeStateFileAtomic(path string, contents []byte) error {
	dir := filepath.Dir(path)
	base := filepath.Base(path)
	tmp, err := os.CreateTemp(dir, "."+base+".tmp.")
	if err != nil {
		return err
	}
	tmpPath := tmp.Name()
	cleanup := true
	defer func() {
		if cleanup {
			_ = os.Remove(tmpPath)
		}
	}()
	if err := tmp.Chmod(0o600); err != nil {
		_ = tmp.Close()
		return err
	}
	if _, err := tmp.Write(contents); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Sync(); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Rename(tmpPath, path); err != nil {
		return err
	}
	cleanup = false
	return fsyncDirectoryBestEffort(dir)
}

func preserveCorruptState(path, corruptPath string) error {
	if _, err := os.Stat(path); err != nil {
		return err
	}
	_ = os.Remove(corruptPath)
	return os.Rename(path, corruptPath)
}

func fillStateBackedProbe(p stateBackedProbe) stateBackedProbe {
	if p.markerNow == nil {
		start := time.Now()
		p.markerNow = func() uint64 {
			return uint64(time.Since(start).Microseconds())
		}
	}
	if p.realtimeNow == nil {
		p.realtimeNow = func() uint64 {
			return uint64(time.Now().UnixMicro())
		}
	}
	if p.newUUID == nil {
		p.newUUID = newBootIDUUID
	}
	return p
}

func degradedStateBackedResult(path string, probe stateBackedProbe, cause error) (journal.UUID, string, bool, error) {
	fresh, err := probe.newUUID()
	if err != nil {
		return journal.UUID{}, path, true, fmt.Errorf("new uuid: %w", err)
	}
	return fresh, path, true, cause
}

func loadHelperNewUUID(opts LoadOptions) func() (journal.UUID, error) {
	if opts.NewUUID != nil {
		return opts.NewUUID
	}
	return newBootIDUUID
}

// acquireStateLock is a stub for the per-platform lock implementation.
// Linux/FreeBSD use flock on the sidecar lock file; Windows uses
// LockFileEx. The implementation lives in the build-tagged files.
func acquireStateLock(lockPath string) (func() error, error) {
	return acquirePlatformStateLock(lockPath)
}
