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

const derivedRotationFraction = 20

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
	// file creation/open and configured writer options before the caller accepts
	// work.
	LogOpenEager
)

// LogIdentityMode is retained for backward compatibility. The strict writer
// contract always requires explicit machine ID, boot ID, and generated-entry
// monotonic timestamps. SDK-generated fallbacks are no longer accepted.
//
// LogIdentityStrict is the only supported mode and the zero value. The
// previous LogIdentityAuto symbol has been retired; callers that used it must
// supply explicit IDs.
type LogIdentityMode int

const (
	// LogIdentityStrict requires Options.MachineID and Options.BootID to be
	// provided explicitly. This is the default and only supported mode.
	LogIdentityStrict LogIdentityMode = iota
)

// ErrUnsupportedLogIdentityMode is returned when a caller supplies any
// non-strict identity mode. New callers should leave IdentityMode at the zero
// value (LogIdentityStrict) or remove the field from their LogConfig.
var ErrUnsupportedLogIdentityMode = fmt.Errorf("journal: only LogIdentityStrict is supported; supply explicit machine and boot id")

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

	options         Options
	rotation        RotationPolicy
	retention       RetentionPolicy
	strict          bool
	lifecycle       LogLifecycleObserver
	artifacts       LogArtifactSizer
	fieldNamePolicy FieldNamePolicy

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
	tailBootID         UUID
}

// NewLog creates a high-level directory writer. Files are stored below
// dir/<machine-id>/ using Netdata-compatible chain naming by default, with
// opt-in strict systemd active naming through StrictSystemdNaming.
func NewLog(dir string, config LogConfig) (*Log, error) {
	prepared, err := prepareNewLogConfig(dir, config)
	if err != nil {
		return nil, err
	}

	machineDir := filepath.Join(dir, prepared.options.MachineID.String())
	if err := os.MkdirAll(machineDir, 0o750); err != nil {
		return nil, err
	}

	l := &Log{
		configuredDir:   dir,
		machineDir:      machineDir,
		source:          prepared.source,
		options:         prepared.options,
		rotation:        prepared.rotation,
		retention:       config.RetentionPolicy,
		strict:          config.StrictSystemdNaming,
		lifecycle:       config.Lifecycle,
		artifacts:       config.ArtifactSizer,
		fieldNamePolicy: prepared.logFieldPolicy,
		entriesInFile:   0,
	}

	if err := l.openExistingChain(prepared.explicitHeadSeqnum, prepared.explicitSeqnumID); err != nil {
		return nil, err
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

type preparedLogConfig struct {
	source             string
	options            Options
	rotation           RotationPolicy
	logFieldPolicy     FieldNamePolicy
	explicitHeadSeqnum bool
	explicitSeqnumID   bool
}

func prepareNewLogConfig(dir string, config LogConfig) (preparedLogConfig, error) {
	if err := validateNewLogConfig(dir, config); err != nil {
		return preparedLogConfig{}, err
	}
	source := normalizedLogSource(config.Source)
	rotation := deriveRotationPolicy(config.RotationPolicy, config.RetentionPolicy, config.Options.Compact)
	options := config.Options
	options.FieldNamePolicy = logWriterFieldNamePolicy(config.Options.FieldNamePolicy)
	if options.MaxFileSize == 0 && rotation.MaxFileSize != nil {
		options.MaxFileSize = *rotation.MaxFileSize
	}
	opts, err := normalizeLogOptions(options, config.IdentityMode)
	if err != nil {
		return preparedLogConfig{}, err
	}
	return preparedLogConfig{
		source:             source,
		options:            opts,
		rotation:           rotation,
		logFieldPolicy:     config.Options.FieldNamePolicy,
		explicitHeadSeqnum: config.Options.HeadSeqnum != 0,
		explicitSeqnumID:   !isZeroUUID(config.Options.SeqnumID),
	}, nil
}

func validateNewLogConfig(dir string, config LogConfig) error {
	if dir == "" {
		return errInvalidJournal
	}
	if config.OpenMode != LogOpenLazy && config.OpenMode != LogOpenEager {
		return fmt.Errorf("%w: unsupported log open mode %d", errInvalidJournal, config.OpenMode)
	}
	if config.IdentityMode != LogIdentityStrict {
		return ErrUnsupportedLogIdentityMode
	}
	if err := validateFieldNamePolicy(config.Options.FieldNamePolicy); err != nil {
		return err
	}
	if err := validateFileMode(config.Options.FileMode); err != nil {
		return err
	}
	if err := validateJournalSource(normalizedLogSource(config.Source)); err != nil {
		return err
	}
	if err := validateRotationPolicy(config.RotationPolicy); err != nil {
		return err
	}
	if err := validateRetentionPolicy(config.RetentionPolicy); err != nil {
		return err
	}
	return validateStrictLogIdentity(config)
}

func normalizedLogSource(source string) string {
	if source == "" {
		return "system"
	}
	return source
}

func validateStrictLogIdentity(config LogConfig) error {
	if isZeroUUID(config.Options.MachineID) {
		return fmt.Errorf("%w: strict identity requires machine id", errInvalidJournal)
	}
	if isZeroUUID(config.Options.BootID) {
		return fmt.Errorf("%w: strict identity requires boot id", errInvalidJournal)
	}
	return nil
}

func (l *Log) openExistingChain(explicitHeadSeqnum, explicitSeqnumID bool) error {
	state, err := l.scanChainState()
	if err != nil {
		return err
	}
	l.applyChainTailState(state, explicitHeadSeqnum, explicitSeqnumID)
	if l.strict {
		return l.openStrictChainActive(state)
	}
	return l.openDefaultChainActive(state)
}

func (l *Log) applyChainTailState(state chainState, explicitHeadSeqnum, explicitSeqnumID bool) {
	if !state.hasTail {
		return
	}
	if !explicitHeadSeqnum {
		l.options.HeadSeqnum = state.tailSeqnum + 1
	}
	if !explicitSeqnumID {
		l.options.SeqnumID = state.seqnumID
	}
	l.lastRealtime = state.tailRealtime
	if state.tailBootID == l.options.BootID {
		l.lastMonotonic = state.tailMonotonic
	}
}

func (l *Log) openStrictChainActive(state chainState) error {
	if state.activePath != "" {
		if err := l.archiveOnlineChainActive(state.activePath); err != nil {
			return err
		}
	}
	activePath := l.systemdActivePath()
	if _, err := os.Stat(activePath); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		return err
	}
	l.active = activePath
	return l.openActivePath(activePath)
}

func (l *Log) openDefaultChainActive(state chainState) error {
	if state.activePath == "" {
		return nil
	}
	l.active = state.activePath
	return l.openActivePath(l.active)
}

func (l *Log) openActivePath(path string) error {
	w, err := OpenWithOptions(path, l.options)
	if err != nil {
		if !replaceableActiveOpenError(err) {
			return err
		}
		return l.replaceActiveFile(path)
	}
	if w.header.nEntries == 0 {
		return l.discardEmptyOpenedWriter(w)
	}
	l.attachOpenedWriter(w)
	return nil
}

func (l *Log) archiveOnlineChainActive(path string) error {
	w, err := OpenWithOptions(path, l.options)
	if err != nil {
		if replaceableActiveOpenError(err) {
			return l.replaceActiveFile(path)
		}
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

func replaceableActiveOpenError(err error) bool {
	return errors.Is(err, errUnsupportedJournal)
}

func (l *Log) replaceActiveFile(path string) error {
	header, err := readJournalHeader(path)
	if err != nil {
		return l.disposeActiveFile(path)
	}
	currentTail := uint64(0)
	if l.options.HeadSeqnum > 0 {
		currentTail = l.options.HeadSeqnum - 1
	}
	if header.nEntries > 0 && header.tailEntrySeqnum >= currentTail {
		l.options.SeqnumID = header.seqnumID
		l.options.HeadSeqnum = header.tailEntrySeqnum + 1
		if !isZeroUUID(header.tailEntryBootID) {
			l.options.BootID = header.tailEntryBootID
		}
		l.lastRealtime = header.tailEntryRealtime
		l.lastMonotonic = header.tailEntryMonotonic
	}

	return l.disposeActiveFile(path)
}

func (l *Log) disposeActiveFile(path string) error {
	target := disposedJournalPath(path, 0)
	for attempt := 1; ; attempt++ {
		if _, err := os.Stat(target); errors.Is(err, os.ErrNotExist) {
			break
		} else if err != nil {
			return err
		}
		target = disposedJournalPath(path, attempt)
	}
	if err := os.Rename(path, target); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		return err
	}
	if l.active == path {
		l.active = ""
	}
	return syncJournalDirectory(target)
}

func disposedJournalPath(path string, attempt int) string {
	stem := strings.TrimSuffix(path, ".journal")
	return fmt.Sprintf(
		"%s@%016x-%016x.journal~",
		stem,
		uint64(time.Now().UnixNano()),
		uint64(os.Getpid())^uint64(attempt),
	)
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
	preparedFields, err := prepareFieldsForPolicy(fields, l.fieldNamePolicy)
	if err != nil {
		return err
	}
	if err := validateEntryMonotonicOptions(opts); err != nil {
		return err
	}
	fields = preparedFields
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
	fields = appendLogMetadataFields(fields, l.entryBootIDForAppend(opts), opts.SourceRealtimeUsec)
	if err := l.writer.Append(fields, opts); err != nil {
		return err
	}
	l.captureAppendState()
	return nil
}

// AppendRaw appends one entry from complete KEY=value byte payloads through
// the directory writer. The first '=' byte separates the field name from the
// value; later '=' bytes and arbitrary value bytes are preserved.
func (l *Log) AppendRaw(payloads [][]byte, opts EntryOptions) error {
	if l.closed {
		return errWriterClosed
	}
	preparedPayloads, err := prepareRawPayloadsForPolicy(payloads, l.fieldNamePolicy)
	if err != nil {
		return err
	}
	if err := validateEntryMonotonicOptions(opts); err != nil {
		return err
	}
	payloads = preparedPayloads
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
	payloads = appendLogMetadataPayloads(payloads, l.entryBootIDForAppend(opts), opts.SourceRealtimeUsec)
	if err := l.writer.AppendRaw(payloads, opts); err != nil {
		return err
	}
	l.captureAppendState()
	return nil
}

// AppendMap appends a string-valued entry through the directory writer.
// Under the strict writer contract this compatibility wrapper returns
// ErrMissingMonotonicUsec; use AppendMapWithOptions for new code.
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

func (l *Log) entryOptionsForAppend(opts EntryOptions) EntryOptions {
	realtimeSet := opts.RealtimeUsecSet || opts.RealtimeUsec != 0
	if !realtimeSet {
		opts.RealtimeUsec = uint64(time.Now().UnixMicro())
	}
	if opts.RealtimeUsec <= l.lastRealtime {
		opts.RealtimeUsec = l.lastRealtime + 1
	}
	monotonicSet := opts.MonotonicUsecSet || opts.MonotonicUsec != 0
	if monotonicSet && opts.MonotonicUsec <= l.lastMonotonic {
		opts.MonotonicUsec = l.lastMonotonic + 1
		opts.MonotonicUsecSet = true
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
			state.tailBootID = header.tailEntryBootID
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
	f, err := openReaderFile(path)
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
	f, err := openReaderFile(path)
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
	_ = mode
	return normalizeOptions(opts)
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

func deriveRotationPolicy(rotation RotationPolicy, retention RetentionPolicy, compact bool) RotationPolicy {
	resolved := rotation
	if resolved.MaxFileSize == nil && retention.MaxBytes != nil {
		size := *retention.MaxBytes / derivedRotationFraction
		if size == 0 {
			size = 1
		}
		size = normalizeJournalMaxFileSize(size, compact)
		resolved.MaxFileSize = &size
	}
	if resolved.MaxDuration == nil && retention.MaxAge != nil {
		micros := durationUsec(*retention.MaxAge)
		derivedMicros := (micros + derivedRotationFraction - 1) / derivedRotationFraction
		if derivedMicros == 0 {
			derivedMicros = 1
		}
		duration := time.Duration(derivedMicros) * time.Microsecond
		resolved.MaxDuration = &duration
	}
	return resolved
}

func (l *Log) entryBootIDForAppend(opts EntryOptions) UUID {
	if !isZeroUUID(opts.BootID) {
		return opts.BootID
	}
	return l.options.BootID
}

func appendLogMetadataFields(fields []Field, bootID UUID, sourceRealtimeUsec uint64) []Field {
	extra := 1
	if sourceRealtimeUsec != 0 {
		extra++
	}
	withMetadata := make([]Field, 0, len(fields)+extra)
	withMetadata = append(withMetadata, StringField("_BOOT_ID", bootID.String()))
	if sourceRealtimeUsec != 0 {
		withMetadata = append(withMetadata, StringField("_SOURCE_REALTIME_TIMESTAMP", strconv.FormatUint(sourceRealtimeUsec, 10)))
	}
	withMetadata = append(withMetadata, fields...)
	return withMetadata
}

func appendLogMetadataPayloads(payloads [][]byte, bootID UUID, sourceRealtimeUsec uint64) [][]byte {
	extra := 1
	if sourceRealtimeUsec != 0 {
		extra++
	}
	withMetadata := make([][]byte, 0, len(payloads)+extra)
	bootPayload := []byte("_BOOT_ID=")
	bootPayload = append(bootPayload, bootID.String()...)
	withMetadata = append(withMetadata, bootPayload)
	if sourceRealtimeUsec != 0 {
		sourcePayload := []byte("_SOURCE_REALTIME_TIMESTAMP=")
		sourcePayload = strconv.AppendUint(sourcePayload, sourceRealtimeUsec, 10)
		withMetadata = append(withMetadata, sourcePayload)
	}
	withMetadata = append(withMetadata, payloads...)
	return withMetadata
}

func (l *Log) emitLifecycle(event LogLifecycleEvent) {
	if l.lifecycle != nil {
		l.lifecycle.OnLogLifecycleEvent(event)
	}
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
		if !validJournalSourceByte(source[i]) {
			return errInvalidJournal
		}
	}
	return nil
}

func validJournalSourceByte(c byte) bool {
	return (c >= 'a' && c <= 'z') ||
		(c >= 'A' && c <= 'Z') ||
		(c >= '0' && c <= '9') ||
		c == '_' || c == '-' || c == '.'
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
