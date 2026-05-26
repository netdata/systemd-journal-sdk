package journal

import (
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
)

var syncJournalDirectory = syncParentDir

// RotationPolicy controls when a directory writer starts a new journal file.
type RotationPolicy struct {
	MaxFileSize *uint64
	MaxEntries  *int
	MaxDuration *time.Duration
}

// WithMaxFileSize returns a policy that rotates after the active file reaches
// size bytes. The entry that crosses the limit remains in the current file; the
// next append rotates first. Zero is rejected by NewLog. Values smaller than the
// journal header and hash table overhead rotate after every non-empty append.
func (p RotationPolicy) WithMaxFileSize(size uint64) RotationPolicy {
	p.MaxFileSize = &size
	return p
}

// WithMaxEntries returns a policy that rotates after n entries in the active
// file. The next append creates the successor file. Values at or below zero are
// rejected by NewLog.
func (p RotationPolicy) WithMaxEntries(n int) RotationPolicy {
	p.MaxEntries = &n
	return p
}

// WithMaxDuration returns a policy that rotates before appending an entry whose
// realtime timestamp is at least d after the active file head timestamp. Values
// at or below zero are rejected by NewLog.
func (p RotationPolicy) WithMaxDuration(d time.Duration) RotationPolicy {
	p.MaxDuration = &d
	return p
}

// RetentionPolicy controls deletion of old archived files owned by a Log.
type RetentionPolicy struct {
	MaxFiles *int
	MaxBytes *uint64
	MaxAge   *time.Duration
}

// WithMaxFiles returns a policy that keeps at most n tracked journal files. The
// active/current file is counted but is never deleted to satisfy this limit.
// Values at or below zero are rejected by NewLog.
func (p RetentionPolicy) WithMaxFiles(n int) RetentionPolicy {
	p.MaxFiles = &n
	return p
}

// WithMaxBytes returns a policy that deletes oldest archived files until the
// active plus archived files fit within size bytes, or no archived files remain.
// The active file is counted in the total but is never deleted to satisfy this
// limit. Zero is rejected by NewLog.
func (p RetentionPolicy) WithMaxBytes(size uint64) RetentionPolicy {
	p.MaxBytes = &size
	return p
}

// WithMaxAge returns a policy that deletes archived files whose head realtime
// timestamp is older than d. The active/current file is counted but is never
// deleted to satisfy this limit. Values at or below zero are rejected by
// NewLog.
func (p RetentionPolicy) WithMaxAge(d time.Duration) RetentionPolicy {
	p.MaxAge = &d
	return p
}

// LogOpenMode controls whether NewLog creates/opens the active file
// immediately or waits until the first append.
type LogOpenMode int

const (
	// LogOpenLazy validates the directory and existing chain state at NewLog()
	// time, but creates a new active file only when the first entry is appended.
	LogOpenLazy LogOpenMode = iota
	// LogOpenEager creates or opens the active file during NewLog(), proving
	// file creation/open, writer lock acquisition, and configured writer
	// options before the caller accepts work.
	LogOpenEager
)

// LogIdentityMode controls how missing machine and boot IDs are handled.
type LogIdentityMode int

const (
	// LogIdentityAuto loads host IDs when available and generates missing IDs.
	LogIdentityAuto LogIdentityMode = iota
	// LogIdentityStrict requires Options.MachineID and Options.BootID to be
	// provided explicitly.
	LogIdentityStrict
)

// LogLifecycleEventType identifies a high-level journal file lifecycle event.
type LogLifecycleEventType string

const (
	LogLifecycleCreated LogLifecycleEventType = "created"
	LogLifecycleRotated LogLifecycleEventType = "rotated"
	LogLifecycleDeleted LogLifecycleEventType = "deleted"
)

// LogLifecycleReason identifies why a lifecycle event happened.
type LogLifecycleReason string

const (
	LogLifecycleReasonAppend    LogLifecycleReason = "append"
	LogLifecycleReasonEagerOpen LogLifecycleReason = "eager_open"
	LogLifecycleReasonRotation  LogLifecycleReason = "rotation"
	LogLifecycleReasonRetention LogLifecycleReason = "retention"
)

// LogLifecycleEvent describes a journal file lifecycle change.
type LogLifecycleEvent struct {
	Type         LogLifecycleEventType
	Reason       LogLifecycleReason
	ActivePath   string
	ArchivedPath string
	DeletedPaths []string
}

// LogLifecycleObserver receives synchronous journal lifecycle notifications.
type LogLifecycleObserver interface {
	OnLogLifecycleEvent(LogLifecycleEvent)
}

// LogLifecycleObserverFunc adapts a function to LogLifecycleObserver.
type LogLifecycleObserverFunc func(LogLifecycleEvent)

// OnLogLifecycleEvent implements LogLifecycleObserver.
func (f LogLifecycleObserverFunc) OnLogLifecycleEvent(event LogLifecycleEvent) {
	f(event)
}

// LogArtifactSizer returns consumer-owned bytes associated with a journal file.
type LogArtifactSizer interface {
	JournalArtifactSize(journalPath string) (uint64, error)
}

// LogArtifactSizeFunc adapts a function to LogArtifactSizer.
type LogArtifactSizeFunc func(journalPath string) (uint64, error)

// JournalArtifactSize implements LogArtifactSizer.
func (f LogArtifactSizeFunc) JournalArtifactSize(journalPath string) (uint64, error) {
	return f(journalPath)
}

// LogConfig configures a high-level directory journal writer.
type LogConfig struct {
	Options         Options
	Source          string
	RotationPolicy  RotationPolicy
	RetentionPolicy RetentionPolicy
	OpenMode        LogOpenMode
	IdentityMode    LogIdentityMode
	Lifecycle       LogLifecycleObserver
	ArtifactSizer   LogArtifactSizer
	// StrictSystemdNaming uses <source>.journal as the active filename.
	// The default false value matches the Netdata Rust writer and uses
	// <source>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal for the
	// active file.
	StrictSystemdNaming bool
}

// Log writes journal entries to a systemd-compatible journal directory. Log is
// not safe for concurrent method calls; callers must serialize writes to the
// single writer instance.
type Log struct {
	configuredDir string
	machineDir    string
	source        string
	active        string

	options   Options
	rotation  RotationPolicy
	retention RetentionPolicy
	strict    bool
	lifecycle LogLifecycleObserver
	artifacts LogArtifactSizer
	remaps    map[string]string

	writer        *Writer
	entriesInFile int
	closed        bool
	openRetention bool
	lastRealtime  uint64
	lastMonotonic uint64
}

type archivedJournalFile struct {
	path         string
	headSeqnum   uint64
	headRealtime uint64
	size         uint64
}

type chainState struct {
	tailSeqnum         uint64
	seqnumID           UUID
	hasTail            bool
	activePath         string
	activeTailSeqnum   uint64
	activeHeadRealtime uint64
	tailRealtime       uint64
	tailMonotonic      uint64
}

// NewLog creates a high-level directory writer. Files are stored below
// dir/<machine-id>/ using Netdata-compatible chain naming by default, with
// opt-in strict systemd active naming through StrictSystemdNaming.
func NewLog(dir string, config LogConfig) (*Log, error) {
	if dir == "" {
		return nil, errInvalidJournal
	}
	if config.OpenMode != LogOpenLazy && config.OpenMode != LogOpenEager {
		return nil, fmt.Errorf("%w: unsupported log open mode %d", errInvalidJournal, config.OpenMode)
	}
	if config.IdentityMode != LogIdentityAuto && config.IdentityMode != LogIdentityStrict {
		return nil, fmt.Errorf("%w: unsupported log identity mode %d", errInvalidJournal, config.IdentityMode)
	}

	source := config.Source
	if source == "" {
		source = "system"
	}
	if err := validateJournalSource(source); err != nil {
		return nil, err
	}
	if err := validateRotationPolicy(config.RotationPolicy); err != nil {
		return nil, err
	}
	if err := validateRetentionPolicy(config.RetentionPolicy); err != nil {
		return nil, err
	}
	if config.IdentityMode == LogIdentityStrict {
		if isZeroUUID(config.Options.MachineID) {
			return nil, fmt.Errorf("%w: strict identity requires machine id", errInvalidJournal)
		}
		if isZeroUUID(config.Options.BootID) {
			return nil, fmt.Errorf("%w: strict identity requires boot id", errInvalidJournal)
		}
	}

	explicitHeadSeqnum := config.Options.HeadSeqnum != 0
	explicitSeqnumID := !isZeroUUID(config.Options.SeqnumID)
	opts, err := normalizeLogOptions(config.Options, config.IdentityMode)
	if err != nil {
		return nil, err
	}

	machineDir := filepath.Join(dir, opts.MachineID.String())
	if err := os.MkdirAll(machineDir, 0o750); err != nil {
		return nil, err
	}

	l := &Log{
		configuredDir: dir,
		machineDir:    machineDir,
		source:        source,
		options:       opts,
		rotation:      config.RotationPolicy,
		retention:     config.RetentionPolicy,
		strict:        config.StrictSystemdNaming,
		lifecycle:     config.Lifecycle,
		artifacts:     config.ArtifactSizer,
		remaps:        make(map[string]string),
		entriesInFile: 0,
	}

	if l.strict {
		state, err := l.scanChainState()
		if err != nil {
			return nil, err
		}
		if state.hasTail {
			if !explicitHeadSeqnum {
				l.options.HeadSeqnum = state.tailSeqnum + 1
			}
			if !explicitSeqnumID {
				l.options.SeqnumID = state.seqnumID
			}
			l.lastRealtime = state.tailRealtime
			l.lastMonotonic = state.tailMonotonic
		}
		if state.activePath != "" {
			if err := l.archiveOnlineChainActive(state.activePath); err != nil {
				return nil, err
			}
		}
		activePath := l.systemdActivePath()
		if _, err := os.Stat(activePath); err == nil {
			l.active = activePath
			w, err := Open(activePath)
			if err != nil {
				return nil, err
			}
			if w.header.nEntries == 0 {
				if err := l.discardEmptyOpenedWriter(w); err != nil {
					return nil, err
				}
			} else {
				l.attachOpenedWriter(w)
			}
		} else if !errors.Is(err, os.ErrNotExist) {
			return nil, err
		}
	} else {
		state, err := l.scanChainState()
		if err != nil {
			return nil, err
		}
		if state.hasTail {
			if !explicitHeadSeqnum {
				l.options.HeadSeqnum = state.tailSeqnum + 1
			}
			if !explicitSeqnumID {
				l.options.SeqnumID = state.seqnumID
			}
			l.lastRealtime = state.tailRealtime
			l.lastMonotonic = state.tailMonotonic
		}
		if state.activePath != "" {
			l.active = state.activePath
			w, err := Open(l.active)
			if err != nil {
				return nil, err
			}
			if w.header.nEntries == 0 {
				if err := l.discardEmptyOpenedWriter(w); err != nil {
					return nil, err
				}
			} else {
				l.attachOpenedWriter(w)
			}
		}
	}
	if config.OpenMode == LogOpenEager && l.writer == nil {
		if err := l.ensureWriter(l.entryOptionsForAppend(EntryOptions{}), LogLifecycleReasonEagerOpen); err != nil {
			return nil, err
		}
	}
	if err := l.enforceRetentionOnOpen(); err != nil {
		return nil, err
	}

	return l, nil
}

func (l *Log) archiveOnlineChainActive(path string) error {
	w, err := Open(path)
	if err != nil {
		return err
	}
	if w.header.nEntries == 0 {
		closeErr := w.Close()
		removeErr := os.Remove(path)
		if errors.Is(removeErr, os.ErrNotExist) {
			removeErr = nil
		}
		return errors.Join(closeErr, removeErr)
	}
	return w.archiveTo(path)
}

func (l *Log) attachOpenedWriter(w *Writer) {
	l.writer = w
	l.options.SeqnumID = w.header.seqnumID
	l.options.BootID = w.bootID
	l.options.HeadSeqnum = w.nextSeqnum
	l.entriesInFile = int(w.header.nEntries)
	l.lastRealtime = w.header.tailEntryRealtime
	l.lastMonotonic = w.header.tailEntryMonotonic
}

func (l *Log) discardEmptyOpenedWriter(w *Writer) error {
	closeErr := w.Close()
	removeErr := os.Remove(l.active)
	if errors.Is(removeErr, os.ErrNotExist) {
		removeErr = nil
	}
	if !l.strict {
		l.active = ""
	}
	return errors.Join(closeErr, removeErr)
}

// Append appends one entry, rotating first if the current active file already
// satisfies a configured rotation limit.
func (l *Log) Append(fields []Field, opts EntryOptions) error {
	if l.closed {
		return errWriterClosed
	}
	if err := validateEntryFields(fields); err != nil {
		return err
	}
	opts = l.entryOptionsForAppend(opts)
	if err := l.enforceRetentionOnOpen(); err != nil {
		return err
	}
	if l.writer != nil && l.shouldRotate(opts.RealtimeUsec) {
		if err := l.rotate(opts); err != nil {
			return err
		}
	}
	if err := l.ensureWriter(opts, LogLifecycleReasonAppend); err != nil {
		return err
	}
	if err := l.enforceRetentionOnOpen(); err != nil {
		return err
	}
	fields, mappings := remapLogFields(fields, l.remaps)
	if len(mappings) > 0 {
		if err := l.writer.Append(l.remappingEntryFields(mappings), opts); err != nil {
			return err
		}
		for _, mapping := range mappings {
			l.remaps[mapping.original] = mapping.mapped
		}
		l.captureAppendState()
		opts = l.entryOptionsForAppend(opts)
	}
	fields = appendSourceRealtimeField(fields, opts.SourceRealtimeUsec)
	if err := l.writer.Append(fields, opts); err != nil {
		return err
	}
	l.captureAppendState()
	return nil
}

// AppendMap appends a string-valued entry through the directory writer.
func (l *Log) AppendMap(fields map[string]string) error {
	return l.AppendMapWithOptions(fields, EntryOptions{})
}

// AppendMapWithOptions appends a string-valued entry through the directory
// writer with timestamp and boot ID options.
func (l *Log) AppendMapWithOptions(fields map[string]string, opts EntryOptions) error {
	keys := make([]string, 0, len(fields))
	for k := range fields {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	entry := make([]Field, 0, len(keys))
	for _, k := range keys {
		entry = append(entry, StringField(k, fields[k]))
	}
	return l.Append(entry, opts)
}

// Sync flushes the active journal file.
func (l *Log) Sync() error {
	if l.closed {
		return errWriterClosed
	}
	if l.writer == nil {
		return nil
	}
	return l.writer.Sync()
}

// EnforceRetention applies the configured retention policy without requiring a
// rotation or close. The current active file is counted in retention envelopes
// and protected from deletion.
func (l *Log) EnforceRetention() error {
	if l.closed {
		return errWriterClosed
	}
	return l.enforceRetention(l.activePath())
}

func (l *Log) enforceRetentionOnOpen() error {
	if l.openRetention || l.writer == nil {
		return nil
	}
	if err := l.enforceRetention(l.activePath()); err != nil {
		return err
	}
	l.openRetention = true
	return nil
}

// Close archives the active file and applies retention.
func (l *Log) Close() error {
	if l.closed {
		return nil
	}
	if l.writer == nil {
		l.closed = true
		return nil
	}
	if l.writer.header.nEntries == 0 && l.strict {
		err1 := l.writer.Close()
		err2 := os.Remove(l.activePath())
		if errors.Is(err2, os.ErrNotExist) {
			err2 = nil
		}
		l.writer = nil
		l.active = ""
		if err := errors.Join(err1, err2); err != nil {
			l.closed = true
			return err
		}
		l.closed = true
		return nil
	}
	protectedPath := l.activePath()
	if l.strict {
		protectedPath = l.archivePathFor(l.writer.header)
	}
	if _, err := l.archiveActive(); err != nil {
		if l.writer == nil {
			l.closed = true
		}
		return err
	}
	if err := l.enforceRetention(protectedPath); err != nil {
		l.closed = true
		return err
	}
	l.closed = true
	return nil
}

func validateEntryFields(fields []Field) error {
	if len(fields) == 0 {
		return errEntryEmpty
	}
	return nil
}

// ActivePath returns the active journal path for this log directory.
func (l *Log) ActivePath() string {
	return l.active
}

// JournalDirectory returns the machine-id directory containing this log's
// journal files.
func (l *Log) JournalDirectory() string {
	return l.machineDir
}

// ConfiguredDirectory returns the directory passed to NewLog before the
// machine-id child path is appended.
func (l *Log) ConfiguredDirectory() string {
	return l.configuredDir
}

// MachineID returns the machine ID used for the journal directory and files.
func (l *Log) MachineID() UUID {
	return l.options.MachineID
}

// BootID returns the boot ID used for entries that do not override it.
func (l *Log) BootID() UUID {
	return l.options.BootID
}

// Source returns the journal source filename prefix.
func (l *Log) Source() string {
	return l.source
}

func (l *Log) ensureWriter(entryOpts EntryOptions, reason LogLifecycleReason) error {
	if l.writer != nil {
		return nil
	}
	opts := l.options
	opts.FileID = UUID{}
	if opts.HeadSeqnum == 0 {
		opts.HeadSeqnum = 1
	}
	if l.strict {
		l.active = l.systemdActivePath()
	} else {
		headRealtime := entryOpts.RealtimeUsec
		if headRealtime == 0 {
			headRealtime = uint64(time.Now().UnixMicro())
		}
		l.active = l.chainPathFor(opts.SeqnumID, opts.HeadSeqnum, headRealtime)
	}
	w, err := Create(l.activePath(), opts)
	if err != nil {
		return err
	}
	l.writer = w
	l.entriesInFile = 0
	if reason != LogLifecycleReasonRotation {
		l.emitLifecycle(LogLifecycleEvent{
			Type:       LogLifecycleCreated,
			Reason:     reason,
			ActivePath: l.activePath(),
		})
	}
	return nil
}

func (l *Log) shouldRotate(nextRealtimeUsec uint64) bool {
	if l.writer == nil {
		return false
	}
	if l.rotation.MaxEntries != nil && l.entriesInFile >= *l.rotation.MaxEntries {
		return true
	}
	if l.writer.header.nEntries > 0 &&
		l.rotation.MaxFileSize != nil &&
		l.writer.CurrentSize() >= *l.rotation.MaxFileSize {
		return true
	}
	if l.writer.header.nEntries == 0 || l.rotation.MaxDuration == nil {
		return false
	}
	maxDurationUsec := durationUsec(*l.rotation.MaxDuration)
	// Keep the explicit comparison before subtraction to avoid uint64
	// underflow if a caller supplies a timestamp older than the active head.
	return nextRealtimeUsec >= l.writer.header.headEntryRealtime &&
		nextRealtimeUsec-l.writer.header.headEntryRealtime >= maxDurationUsec
}

func (l *Log) rotate(entryOpts EntryOptions) error {
	if l.writer == nil {
		return l.ensureWriter(entryOpts, LogLifecycleReasonAppend)
	}
	nextSeqnum := l.writer.nextSeqnum
	seqnumID := l.writer.header.seqnumID
	bootID := l.writer.bootID
	archivedPath, err := l.archiveActive()
	if err != nil {
		return err
	}
	l.options.SeqnumID = seqnumID
	l.options.BootID = bootID
	l.options.HeadSeqnum = nextSeqnum
	if err := l.ensureWriter(entryOpts, LogLifecycleReasonRotation); err != nil {
		return err
	}
	l.remaps = make(map[string]string)
	l.emitLifecycle(LogLifecycleEvent{
		Type:         LogLifecycleRotated,
		Reason:       LogLifecycleReasonRotation,
		ArchivedPath: archivedPath,
		ActivePath:   l.activePath(),
	})
	return l.enforceRetention(l.activePath())
}

func (l *Log) captureAppendState() {
	l.options.HeadSeqnum = l.writer.nextSeqnum
	l.entriesInFile = int(l.writer.header.nEntries)
	l.lastRealtime = l.writer.header.tailEntryRealtime
	l.lastMonotonic = l.writer.header.tailEntryMonotonic
}

func (l *Log) remappingEntryFields(mappings []remappedFieldMapping) []Field {
	fields := make([]Field, 0, len(mappings)+2)
	fields = append(fields, StringField("_BOOT_ID", l.options.BootID.String()))
	fields = append(fields, StringField(remappingMarker, "1"))
	for _, mapping := range mappings {
		fields = append(fields, Field{Name: mapping.mapped, Value: []byte(mapping.original)})
	}
	return fields
}

func (l *Log) archiveActive() (string, error) {
	if l.writer == nil {
		return "", nil
	}
	nextSeqnum := l.writer.nextSeqnum
	seqnumID := l.writer.header.seqnumID
	bootID := l.writer.bootID
	archivePath := l.activePath()
	if l.strict {
		archivePath = l.archivePathFor(l.writer.header)
	}
	if err := l.writer.archiveTo(archivePath); err != nil {
		if l.writer.closed {
			l.options.SeqnumID = seqnumID
			l.options.BootID = bootID
			l.options.HeadSeqnum = nextSeqnum
			l.writer = nil
			l.entriesInFile = 0
			l.active = ""
		}
		return archivePath, err
	}
	l.writer = nil
	l.entriesInFile = 0
	l.active = ""
	return archivePath, nil
}

func (l *Log) enforceRetention(protectedPath string) error {
	files, _, err := l.archivedFiles()
	if err != nil {
		return err
	}
	activePath := protectedPath
	if activePath == "" {
		activePath = l.activePath()
	}
	var total uint64
	activeInFiles := false
	for _, file := range files {
		if activePath != "" && file.path == activePath {
			activeInFiles = true
		}
		total = saturatingAdd(total, file.size)
	}
	activeExtraFile := false
	if activePath != "" && !activeInFiles {
		if activeInfo, err := os.Stat(activePath); err == nil {
			activeExtraFile = true
			activeSize, err := l.retainedSize(activePath, uint64(activeInfo.Size()))
			if err != nil {
				return err
			}
			total = saturatingAdd(total, activeSize)
		} else if !errors.Is(err, os.ErrNotExist) {
			return err
		}
	}

	sort.Slice(files, func(i, j int) bool {
		if files[i].headRealtime != files[j].headRealtime {
			return files[i].headRealtime < files[j].headRealtime
		}
		if files[i].headSeqnum != files[j].headSeqnum {
			return files[i].headSeqnum < files[j].headSeqnum
		}
		return files[i].path < files[j].path
	})

	fileCount := len(files)
	if activeExtraFile {
		fileCount++
	}
	var deletedPaths []string
	for l.retention.MaxFiles != nil && fileCount > *l.retention.MaxFiles {
		deleteIndex := -1
		for i, file := range files {
			if activePath == "" || file.path != activePath {
				deleteIndex = i
				break
			}
		}
		if deleteIndex == -1 {
			break
		}
		deleted := files[deleteIndex]
		if err := os.Remove(deleted.path); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		deletedPaths = append(deletedPaths, deleted.path)
		total = saturatingSub(total, deleted.size)
		files = append(files[:deleteIndex], files[deleteIndex+1:]...)
		fileCount--
	}
	for l.retention.MaxBytes != nil && total > *l.retention.MaxBytes && len(files) > 0 {
		deleteIndex := -1
		for i, file := range files {
			if activePath == "" || file.path != activePath {
				deleteIndex = i
				break
			}
		}
		if deleteIndex == -1 {
			break
		}
		deleted := files[deleteIndex]
		if err := os.Remove(deleted.path); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		deletedPaths = append(deletedPaths, deleted.path)
		total = saturatingSub(total, deleted.size)
		files = append(files[:deleteIndex], files[deleteIndex+1:]...)
	}
	if l.retention.MaxAge != nil {
		cutoff := uint64(time.Now().UnixMicro())
		maxAgeUsec := durationUsec(*l.retention.MaxAge)
		if cutoff >= maxAgeUsec {
			cutoff -= maxAgeUsec
		} else {
			cutoff = 0
		}
		for len(files) > 0 {
			deleteIndex := -1
			for i, file := range files {
				if file.headRealtime > cutoff {
					break
				}
				if activePath == "" || file.path != activePath {
					deleteIndex = i
					break
				}
			}
			if deleteIndex == -1 {
				break
			}
			deleted := files[deleteIndex]
			if err := os.Remove(deleted.path); err != nil && !errors.Is(err, os.ErrNotExist) {
				return err
			}
			deletedPaths = append(deletedPaths, deleted.path)
			total = saturatingSub(total, deleted.size)
			files = append(files[:deleteIndex], files[deleteIndex+1:]...)
		}
	}
	if err := syncJournalDirectory(l.machineDir); err != nil {
		return err
	}
	if len(deletedPaths) > 0 {
		l.emitLifecycle(LogLifecycleEvent{
			Type:         LogLifecycleDeleted,
			Reason:       LogLifecycleReasonRetention,
			DeletedPaths: deletedPaths,
		})
	}
	return nil
}

func (l *Log) entryOptionsForAppend(opts EntryOptions) EntryOptions {
	if opts.RealtimeUsec == 0 {
		opts.RealtimeUsec = uint64(time.Now().UnixMicro())
	}
	if opts.RealtimeUsec <= l.lastRealtime {
		opts.RealtimeUsec = l.lastRealtime + 1
	}
	if opts.MonotonicUsec != 0 && opts.MonotonicUsec <= l.lastMonotonic {
		opts.MonotonicUsec = l.lastMonotonic + 1
	}
	return opts
}

func durationUsec(d time.Duration) uint64 {
	if d <= 0 {
		return 0
	}
	usec := d / time.Microsecond
	if usec <= 0 {
		return 1
	}
	return uint64(usec)
}

func (l *Log) archivedFiles() ([]archivedJournalFile, uint64, error) {
	entries, err := os.ReadDir(l.machineDir)
	if err != nil {
		return nil, 0, err
	}
	var files []archivedJournalFile
	var total uint64
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		archived, ok := parseArchivedJournalName(entry.Name(), l.source)
		if !ok {
			continue
		}
		info, err := entry.Info()
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return nil, 0, err
		}
		archived.path = filepath.Join(l.machineDir, entry.Name())
		archived.size, err = l.retainedSize(archived.path, uint64(info.Size()))
		if err != nil {
			return nil, 0, err
		}
		files = append(files, archived)
		total = saturatingAdd(total, archived.size)
	}
	return files, total, nil
}

func (l *Log) retainedSize(path string, fallback uint64) (uint64, error) {
	size := committedJournalSize(path, fallback)
	if l.artifacts == nil {
		return size, nil
	}
	artifactSize, err := l.artifacts.JournalArtifactSize(path)
	if err != nil {
		return 0, err
	}
	return saturatingAdd(size, artifactSize), nil
}

func (l *Log) activePath() string {
	if l.active != "" {
		return l.active
	}
	if l.strict {
		return l.systemdActivePath()
	}
	return l.chainPathFor(l.options.SeqnumID, l.options.HeadSeqnum, 0)
}

func (l *Log) systemdActivePath() string {
	return filepath.Join(l.machineDir, l.source+".journal")
}

func (l *Log) chainPathFor(seqnumID UUID, headSeqnum, headRealtime uint64) string {
	name := fmt.Sprintf("%s@%s-%016x-%016x.journal",
		l.source,
		seqnumID.String(),
		headSeqnum,
		headRealtime)
	return filepath.Join(l.machineDir, name)
}

func (l *Log) archivePathFor(header journalHeader) string {
	return l.chainPathFor(header.seqnumID, header.headEntrySeqnum, header.headEntryRealtime)
}

func (l *Log) scanChainState() (chainState, error) {
	files, _, err := l.archivedFiles()
	if err != nil {
		return chainState{}, err
	}
	var state chainState
	for _, file := range files {
		header, err := readJournalHeader(file.path)
		if err != nil {
			continue
		}
		if !state.hasTail || header.tailEntrySeqnum > state.tailSeqnum {
			state.hasTail = true
			state.tailSeqnum = header.tailEntrySeqnum
			state.seqnumID = header.seqnumID
			state.tailRealtime = header.tailEntryRealtime
			state.tailMonotonic = header.tailEntryMonotonic
		}
		if header.state == stateOnline &&
			(state.activePath == "" ||
				header.tailEntrySeqnum > state.activeTailSeqnum ||
				(header.tailEntrySeqnum == state.activeTailSeqnum &&
					header.headEntryRealtime > state.activeHeadRealtime)) {
			state.activePath = file.path
			state.activeTailSeqnum = header.tailEntrySeqnum
			state.activeHeadRealtime = header.headEntryRealtime
		}
	}
	return state, nil
}

func readJournalHeader(path string) (journalHeader, error) {
	f, err := os.Open(path)
	if err != nil {
		return journalHeader{}, err
	}
	defer f.Close()
	buf := make([]byte, headerSize)
	if _, err := f.ReadAt(buf, 0); err != nil {
		return journalHeader{}, err
	}
	return parseHeader(buf)
}

func committedJournalSize(path string, fallback uint64) uint64 {
	f, err := os.Open(path)
	if err != nil {
		return fallback
	}
	defer f.Close()

	buf := make([]byte, headerSize)
	if _, err := f.ReadAt(buf, 0); err != nil {
		return fallback
	}
	header, err := parseHeader(buf)
	if err != nil || header.tailObjectOffset == 0 {
		return fallback
	}
	tail, err := readObjectHeaderAt(f, header.tailObjectOffset)
	if err != nil {
		return fallback
	}
	if tail.size > ^uint64(0)-header.tailObjectOffset {
		return align8Saturating(^uint64(0))
	}
	return align8Saturating(header.tailObjectOffset + tail.size)
}

func align8Saturating(v uint64) uint64 {
	if v > ^uint64(0)-(objectAlignment-1) {
		return ^uint64(0) &^ (objectAlignment - 1)
	}
	return align8(v)
}

func normalizeLogOptions(opts Options, mode LogIdentityMode) (Options, error) {
	if mode == LogIdentityStrict {
		return normalizeOptions(opts), nil
	}
	if isZeroUUID(opts.MachineID) {
		if machineID, err := readUUIDFile("/etc/machine-id"); err == nil {
			opts.MachineID = machineID
		}
	}
	if isZeroUUID(opts.BootID) {
		if bootID, err := readUUIDFile("/proc/sys/kernel/random/boot_id"); err == nil {
			opts.BootID = bootID
		}
	}
	return normalizeOptions(opts), nil
}

func validateRotationPolicy(policy RotationPolicy) error {
	if policy.MaxFileSize != nil && *policy.MaxFileSize == 0 {
		return fmt.Errorf("%w: rotation max file size must be greater than 0", errInvalidJournal)
	}
	if policy.MaxEntries != nil && *policy.MaxEntries <= 0 {
		return fmt.Errorf("%w: rotation max entries must be greater than 0", errInvalidJournal)
	}
	if policy.MaxDuration != nil && *policy.MaxDuration <= 0 {
		return fmt.Errorf("%w: rotation max duration must be greater than 0", errInvalidJournal)
	}
	return nil
}

func validateRetentionPolicy(policy RetentionPolicy) error {
	if policy.MaxFiles != nil && *policy.MaxFiles <= 0 {
		return fmt.Errorf("%w: retention max files must be greater than 0", errInvalidJournal)
	}
	if policy.MaxBytes != nil && *policy.MaxBytes == 0 {
		return fmt.Errorf("%w: retention max bytes must be greater than 0", errInvalidJournal)
	}
	if policy.MaxAge != nil && *policy.MaxAge <= 0 {
		return fmt.Errorf("%w: retention max age must be greater than 0", errInvalidJournal)
	}
	return nil
}

func appendSourceRealtimeField(fields []Field, sourceRealtimeUsec uint64) []Field {
	if sourceRealtimeUsec == 0 {
		return fields
	}
	withSource := make([]Field, 0, len(fields)+1)
	withSource = append(withSource, fields...)
	withSource = append(withSource, StringField("_SOURCE_REALTIME_TIMESTAMP", strconv.FormatUint(sourceRealtimeUsec, 10)))
	return withSource
}

func (l *Log) emitLifecycle(event LogLifecycleEvent) {
	if l.lifecycle != nil {
		l.lifecycle.OnLogLifecycleEvent(event)
	}
}

func readUUIDFile(path string) (UUID, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return UUID{}, err
	}
	return ParseUUID(strings.TrimSpace(string(content)))
}

// ParseUUID parses a 32-character or dashed 36-character UUID string.
func ParseUUID(s string) (UUID, error) {
	clean := strings.ReplaceAll(strings.TrimSpace(s), "-", "")
	if len(clean) != 32 {
		return UUID{}, fmt.Errorf("invalid UUID length")
	}
	bytes, err := hex.DecodeString(clean)
	if err != nil {
		return UUID{}, err
	}
	var id UUID
	copy(id[:], bytes)
	return id, nil
}

func parseArchivedJournalName(name, source string) (archivedJournalFile, bool) {
	stem, ok := strings.CutSuffix(name, ".journal")
	if !ok {
		return archivedJournalFile{}, false
	}
	suffix, ok := strings.CutPrefix(stem, source+"@")
	if !ok {
		return archivedJournalFile{}, false
	}
	parts := strings.Split(suffix, "-")
	if len(parts) != 3 || len(parts[0]) != 32 {
		return archivedJournalFile{}, false
	}
	if _, err := ParseUUID(parts[0]); err != nil {
		return archivedJournalFile{}, false
	}
	headSeqnum, err := strconv.ParseUint(parts[1], 16, 64)
	if err != nil {
		return archivedJournalFile{}, false
	}
	headRealtime, err := strconv.ParseUint(parts[2], 16, 64)
	if err != nil {
		return archivedJournalFile{}, false
	}
	return archivedJournalFile{headSeqnum: headSeqnum, headRealtime: headRealtime}, true
}

func validateJournalSource(source string) error {
	if source == "" || source == "." || source == ".." {
		return errInvalidJournal
	}
	for i := 0; i < len(source); i++ {
		c := source[i]
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
			(c >= '0' && c <= '9') || c == '_' || c == '-' || c == '.' {
			continue
		}
		return errInvalidJournal
	}
	return nil
}

func syncParentDir(path string) error {
	dir := path
	if info, err := os.Stat(path); err == nil && !info.IsDir() {
		dir = filepath.Dir(path)
	}
	f, err := os.Open(dir)
	if err != nil {
		return err
	}
	err1 := f.Sync()
	err2 := f.Close()
	return errors.Join(err1, err2)
}

func saturatingSub(value, other uint64) uint64 {
	if other > value {
		return 0
	}
	return value - other
}

func saturatingAdd(value, other uint64) uint64 {
	if other > ^uint64(0)-value {
		return ^uint64(0)
	}
	return value + other
}
