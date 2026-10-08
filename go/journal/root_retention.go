package journal

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// RootRetentionFile is header-only metadata for one owned source journal.
// Active means strict active naming, including a retained machine's active file.
// Times are saved journal realtime microseconds, not producer event times.
type RootRetentionFile struct {
	Path                                                               string
	MachineID, SeqnumID                                                UUID
	Bytes, Entries, HeadSeqnum, TailSeqnum, HeadRealtime, TailRealtime uint64
	Active                                                             bool
	header                                                             journalHeader
}

// RootRetentionInventory accounts for directory-visible file lengths, including
// preallocation. Unlinked files pinned by readers are not included.
type RootRetentionInventory struct {
	Files []RootRetentionFile
	Bytes uint64
}

// InspectRootRetention inspects a dedicated root/source using only directory
// entries, stat and fixed-size headers. A missing or inaccessible root returns
// its filesystem error.
// The caller must exclude writers/renames and own all machine directories. This
// validates ownership/header metadata, not payload/index integrity or provenance.
// Verify recovered active files before opening a RootRetention Log. Symlinks,
// quarantine names, malformed source candidates and identity mismatches fail
// the whole inventory; an error never represents empty or partial success.
func InspectRootRetention(dir, source string) (RootRetentionInventory, error) {
	if dir == "" {
		return RootRetentionInventory{}, errInvalidJournal
	}
	source = normalizedLogSource(source)
	if err := validateJournalSource(source); err != nil {
		return RootRetentionInventory{}, err
	}
	rootInfo, err := os.Lstat(dir)
	if err != nil {
		return RootRetentionInventory{}, err
	}
	if !rootInfo.IsDir() {
		return RootRetentionInventory{}, rootCandidateError(dir, "root is not a directory")
	}
	dirs, err := os.ReadDir(dir)
	if err != nil {
		return RootRetentionInventory{}, err
	}
	var inventory RootRetentionInventory
	seen := make(map[UUID]bool)
	for _, entry := range dirs {
		// Dedicated roots may also contain caller-owned lock/status files.
		if !entry.IsDir() {
			if entry.Type()&os.ModeSymlink != 0 || ownedRootName(entry.Name(), source) {
				return RootRetentionInventory{}, rootCandidateError(filepath.Join(dir, entry.Name()), "unexpected root entry")
			}
			continue
		}
		machine, err := ParseUUID(entry.Name())
		if err != nil || machine.String() != entry.Name() || isZeroUUID(machine) {
			children, readErr := os.ReadDir(filepath.Join(dir, entry.Name()))
			if readErr != nil {
				return RootRetentionInventory{}, readErr
			}
			for _, child := range children {
				if ownedRootName(child.Name(), source) {
					return RootRetentionInventory{}, rootCandidateError(filepath.Join(dir, entry.Name()), "invalid machine directory")
				}
			}
			continue
		}
		files, err := inspectRootMachine(filepath.Join(dir, entry.Name()), source, machine)
		if err != nil {
			return RootRetentionInventory{}, err
		}
		for _, file := range files {
			if seen[file.header.fileID] {
				return RootRetentionInventory{}, rootCandidateError(file.Path, "duplicate file identity")
			}
			seen[file.header.fileID] = true
			inventory.Bytes = saturatingAdd(inventory.Bytes, file.Bytes)
			inventory.Files = append(inventory.Files, file)
		}
	}
	return inventory, nil
}

func ownedRootName(name, source string) bool {
	return strings.HasPrefix(name, source+"@") || strings.HasPrefix(name, source+".journal")
}

func inspectRootMachine(dir, source string, machine UUID) ([]RootRetentionFile, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	var files []RootRetentionFile
	targets := make(map[string]bool)
	for _, entry := range entries {
		if !ownedRootName(entry.Name(), source) {
			continue
		}
		path := filepath.Join(dir, entry.Name())
		if !entry.Type().IsRegular() {
			return nil, rootCandidateError(path, "nonregular source candidate")
		}
		file, err := inspectRootFile(path, source, machine)
		if err != nil {
			return nil, err
		}
		target := rootArchiveName(source, file.header)
		if targets[target] {
			return nil, rootCandidateError(path, "ambiguous archive identity")
		}
		targets[target] = true
		files = append(files, file)
	}
	return files, nil
}

func inspectRootFile(path, source string, machine UUID) (RootRetentionFile, error) {
	f, err := openReaderFile(path)
	if err != nil {
		return RootRetentionFile{}, err
	}
	defer f.Close()
	h, err := readAppendHeader(f)
	if err != nil {
		return RootRetentionFile{}, fmt.Errorf("%s: %w", path, err)
	}
	info, err := f.Stat()
	if err != nil {
		return RootRetentionFile{}, err
	}
	if err = validateRootHeader(h, uint64(info.Size()), machine); err != nil {
		return RootRetentionFile{}, fmt.Errorf("%s: %w", path, err)
	}
	active := filepath.Base(path) == source+".journal"
	if !active && (filepath.Base(path) != rootArchiveName(source, h) || h.state != stateArchived || h.nEntries == 0) {
		return RootRetentionFile{}, rootCandidateError(path, "archive name/state disagrees with header")
	}
	return RootRetentionFile{Path: path, MachineID: h.machineID, SeqnumID: h.seqnumID, Bytes: uint64(info.Size()), Entries: h.nEntries, HeadSeqnum: h.headEntrySeqnum, TailSeqnum: h.tailEntrySeqnum, HeadRealtime: h.headEntryRealtime, TailRealtime: h.tailEntryRealtime, Active: active, header: h}, nil
}

func validateRootHeader(h journalHeader, size uint64, machine UUID) error {
	if err := validateAppendHeader(h); err != nil {
		return err
	}
	if _, err := h.validateDeclaredArena(size); err != nil {
		return err
	}
	if err := h.validateEmptyEntryMetadata(); err != nil {
		return err
	}
	if h.machineID != machine || isZeroUUID(h.fileID) || isZeroUUID(h.seqnumID) || h.state > stateArchived {
		return errInvalidJournal
	}
	if h.nEntries > 0 && (h.entryArrayOffset == 0 || h.nEntries > h.nObjects || h.headEntrySeqnum == 0 || h.tailEntrySeqnum < h.headEntrySeqnum || h.tailEntrySeqnum-h.headEntrySeqnum < h.nEntries-1 || h.tailEntryRealtime < h.headEntryRealtime) {
		return errInvalidJournal
	}
	return nil
}

func rootArchiveName(source string, h journalHeader) string {
	return fmt.Sprintf("%s@%s-%016x-%016x.journal", source, h.seqnumID.String(), h.headEntrySeqnum, h.headEntryRealtime)
}

func rootCandidateError(path, why string) error {
	return fmt.Errorf("%w: %s: %s", errInvalidJournal, path, why)
}

// RootRetentionResult describes the latest maintenance attempt. Inventory is a
// successful post-attempt sample only when InventoryValid is true. Err is the
// maintenance error; it is independent of healthy append outcomes. Explicit
// attempts use the supplied time; automatic attempts use time.Now. DeletedFiles
// includes discarded empty actives.
type RootRetentionResult struct {
	AttemptedAt      time.Time
	LastSuccessfulAt time.Time
	Inventory        RootRetentionInventory
	InventoryValid   bool
	DeletedFiles     int
	Err              error
}

// LastRootRetentionResult returns the latest explicit or automatic maintenance
// attempt. The bool is false before an attempt. Log methods require exclusion.
func (l *Log) LastRootRetentionResult() (RootRetentionResult, bool) {
	result := l.rootResult
	result.Inventory.Files = append([]RootRetentionFile(nil), result.Inventory.Files...)
	return result, l.rootAttempted
}

// RootRetentionPolicy returns an independent copy of the effective policy.
func (l *Log) RootRetentionPolicy() RetentionPolicy { return cloneRetentionPolicy(l.retention) }

// SetRootRetentionPolicy validates and installs an independent policy copy.
// It does not enforce old or new limits. Maintenance finalizes the active file
// only if a changed derived file-size limit requires new allocation geometry.
// Explicit rotation limits remain in effect. Invalid policies change nothing.
func (l *Log) SetRootRetentionPolicy(policy RetentionPolicy) error {
	if !l.rootRetention {
		return fmt.Errorf("journal: root retention is not enabled")
	}
	if err := l.writable(); err != nil {
		return err
	}
	if err := validateRetentionPolicy(policy); err != nil {
		return err
	}
	policy = cloneRetentionPolicy(policy)
	rotation := deriveRotationPolicy(l.rootRotation, policy, l.options.Compact)
	oldSize, newSize := rotationSize(l.rotation), rotationSize(rotation)
	l.retention = policy
	l.rotation = rotation
	if l.rootConfiguredMaxSize == 0 {
		l.options.MaxFileSize = newSize
	}
	l.rootPolicyRotate = l.rootPolicyRotate || oldSize != newSize
	return nil
}

func rotationSize(p RotationPolicy) uint64 {
	if p.MaxFileSize != nil {
		return *p.MaxFileSize
	}
	return 0
}

func cloneRetentionPolicy(p RetentionPolicy) RetentionPolicy {
	if p.MaxAge != nil {
		v := *p.MaxAge
		p.MaxAge = &v
	}
	if p.MaxBytes != nil {
		v := *p.MaxBytes
		p.MaxBytes = &v
	}
	if p.MaxFiles != nil {
		v := *p.MaxFiles
		p.MaxFiles = &v
	}
	return p
}
func cloneRotationPolicy(p RotationPolicy) RotationPolicy {
	if p.MaxDuration != nil {
		v := *p.MaxDuration
		p.MaxDuration = &v
	}
	if p.MaxFileSize != nil {
		v := *p.MaxFileSize
		p.MaxFileSize = &v
	}
	if p.MaxEntries != nil {
		v := *p.MaxEntries
		p.MaxEntries = &v
	}
	return p
}

// MaintainRootRetention applies root-wide full-length/tail-time retention.
// All candidates are validated before mutation. Verified retired-machine active
// files are finalized through the normal SDK archive lifecycle. A live active
// file is finalized only for idle age expiry or changed allocation policy; the
// successor stays lazy. Safe inventory/unlink errors are maintenance failures;
// uncertain archive mutation poisons the Log and preserves the remaining files.
func (l *Log) MaintainRootRetention(now time.Time) (RootRetentionResult, error) {
	if !l.rootRetention {
		return RootRetentionResult{}, fmt.Errorf("journal: root retention is not enabled")
	}
	if err := l.writable(); err != nil {
		return RootRetentionResult{}, err
	}
	return l.maintainRootRetention(now, true)
}

func (l *Log) maintainRootRetention(now time.Time, expireLive bool) (result RootRetentionResult, err error) {
	result.AttemptedAt = now
	result.LastSuccessfulAt = l.rootResult.LastSuccessfulAt
	defer func() {
		if err == nil {
			result.LastSuccessfulAt = now
		}
		result.Err = err
		l.rootResult = result
		l.rootAttempted = true
	}()
	inv, err := InspectRootRetention(l.configuredDir, l.source)
	if err != nil {
		return result, err
	}
	if err = l.finalizeRootActives(inv, now, expireLive, &result); err != nil {
		return result, err
	}
	inv, err = InspectRootRetention(l.configuredDir, l.source)
	if err != nil {
		return result, err
	}
	err = l.pruneRootRetention(&inv, now, &result)
	// A failed unlink leaves known files in place, but sync or external I/O errors
	// can make any cached count misleading. Resample; never report failed scans as zero.
	sample, sampleErr := InspectRootRetention(l.configuredDir, l.source)
	if sampleErr == nil {
		result.Inventory = sample
		result.InventoryValid = true
	}
	return result, errors.Join(err, sampleErr)
}

func (l *Log) finalizeRootActives(inv RootRetentionInventory, now time.Time, expireLive bool, result *RootRetentionResult) error {
	for _, file := range inv.Files {
		if !file.Active {
			continue
		}
		live := l.writer != nil && file.Path == l.activePath()
		if live {
			expired := file.Entries > 0 && expireLive && rootExpired(file, l.retention, now)
			if !expired && !l.rootPolicyRotate {
				continue
			}
			if file.Entries == 0 {
				w := l.writer
				l.writer = nil
				l.entriesInFile = 0
				if err := l.discardEmptyOpenedWriter(w); err != nil {
					return err
				}
				l.active = ""
				result.DeletedFiles++
				if err := syncJournalDirectory(l.machineDir); err != nil {
					return err
				}
				continue
			}
			if _, err := l.archiveActive(); err != nil {
				return l.recordFailure(err)
			}
			continue
		}
		// The current machine is opened by NewLog before maintenance. Every other
		// active is retired; callers verified its provenance/index before opt-in.
		if err := l.finalizeRetiredActive(file); err != nil {
			return err
		}
		if file.Entries == 0 {
			result.DeletedFiles++
		}
	}
	l.rootPolicyRotate = false
	return nil
}

func (l *Log) finalizeRetiredActive(file RootRetentionFile) error {
	w, err := OpenWithOptions(file.Path, Options{})
	if err != nil {
		return l.recordFailure(errors.Join(ErrWriterFailed, err))
	}
	if file.Entries == 0 {
		if err = w.Close(); err != nil {
			return l.recordFailure(err)
		}
		if err := os.Remove(file.Path); err != nil {
			return err
		}
		return syncJournalDirectory(filepath.Dir(file.Path))
	}
	target := filepath.Join(filepath.Dir(file.Path), rootArchiveName(l.source, file.header))
	if err = w.archiveTo(target, l.syncOnArchive); err != nil {
		return l.recordFailure(errors.Join(err, w.Close()))
	}
	l.emitLifecycle(LogLifecycleEvent{Type: LogLifecycleRotated, Reason: LogLifecycleReasonRetention, ArchivedPath: target})
	return nil
}

var removeRootRetentionFile = os.Remove

func (l *Log) pruneRootRetention(inv *RootRetentionInventory, now time.Time, result *RootRetentionResult) (err error) {
	sort.Slice(inv.Files, func(i, j int) bool {
		a, b := inv.Files[i], inv.Files[j]
		if a.TailRealtime != b.TailRealtime {
			return a.TailRealtime < b.TailRealtime
		}
		return a.Path < b.Path
	})
	count := len(inv.Files)
	var deleted []string
	var changedDirs []string
	seenDirs := make(map[string]bool)
	defer func() {
		for _, dir := range changedDirs {
			err = errors.Join(err, syncJournalDirectory(dir))
		}
		if len(deleted) > 0 {
			l.emitLifecycle(LogLifecycleEvent{Type: LogLifecycleDeleted, Reason: LogLifecycleReasonRetention, DeletedPaths: deleted})
		}
	}()
	for _, file := range inv.Files {
		if file.Active {
			continue
		}
		over := l.retention.MaxBytes != nil && inv.Bytes > *l.retention.MaxBytes || l.retention.MaxFiles != nil && count > *l.retention.MaxFiles
		if !over && !rootExpired(file, l.retention, now) {
			continue
		}
		if err := removeRootRetentionFile(file.Path); err != nil {
			return err
		}
		deleted = append(deleted, file.Path)
		result.DeletedFiles++
		count--
		inv.Bytes = saturatingSub(inv.Bytes, file.Bytes)
		dir := filepath.Dir(file.Path)
		if !seenDirs[dir] {
			seenDirs[dir] = true
			changedDirs = append(changedDirs, dir)
		}
	}
	return nil
}

func rootExpired(file RootRetentionFile, policy RetentionPolicy, now time.Time) bool {
	if policy.MaxAge == nil || now.UnixMicro() < 0 {
		return false
	}
	age := durationUsec(*policy.MaxAge)
	stamp := uint64(now.UnixMicro())
	return stamp >= age && file.TailRealtime <= stamp-age
}
