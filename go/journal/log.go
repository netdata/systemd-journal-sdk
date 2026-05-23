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
)

var syncJournalDirectory = syncParentDir

// RotationPolicy controls when a directory writer starts a new journal file.
type RotationPolicy struct {
	MaxFileSize uint64
	MaxEntries  int
}

// WithMaxFileSize returns a policy that rotates after the active file reaches
// size bytes. The entry that crosses the limit remains in the current file; the
// next append rotates first. Values smaller than the journal header and hash
// table overhead rotate after every non-empty append.
func (p RotationPolicy) WithMaxFileSize(size uint64) RotationPolicy {
	p.MaxFileSize = size
	return p
}

// WithMaxEntries returns a policy that rotates after n entries in the active
// file. The next append creates the successor file.
func (p RotationPolicy) WithMaxEntries(n int) RotationPolicy {
	p.MaxEntries = n
	return p
}

// RetentionPolicy controls deletion of old archived files owned by a Log.
type RetentionPolicy struct {
	MaxFiles int
	MaxBytes uint64
}

// WithMaxFiles returns a policy that keeps at most n archived files. The active
// file is not counted and is never deleted to satisfy this limit.
func (p RetentionPolicy) WithMaxFiles(n int) RetentionPolicy {
	p.MaxFiles = n
	return p
}

// WithMaxBytes returns a policy that deletes oldest archived files until the
// active plus archived files fit within size bytes, or no archived files remain.
// The active file is counted in the total but is never deleted to satisfy this
// limit.
func (p RetentionPolicy) WithMaxBytes(size uint64) RetentionPolicy {
	p.MaxBytes = size
	return p
}

// LogConfig configures a high-level directory journal writer.
type LogConfig struct {
	Options         Options
	Source          string
	RotationPolicy  RotationPolicy
	RetentionPolicy RetentionPolicy
}

// Log writes journal entries to a systemd-compatible journal directory. Log is
// not safe for concurrent method calls; callers must serialize writes to the
// single writer instance.
type Log struct {
	machineDir string
	source     string

	options   Options
	rotation  RotationPolicy
	retention RetentionPolicy

	writer        *Writer
	entriesInFile int
	closed        bool
}

type archivedJournalFile struct {
	path         string
	headSeqnum   uint64
	headRealtime uint64
	size         uint64
}

// NewLog creates a high-level directory writer. Files are stored below
// dir/<machine-id>/ using systemd journal naming.
func NewLog(dir string, config LogConfig) (*Log, error) {
	if dir == "" {
		return nil, errInvalidJournal
	}

	source := config.Source
	if source == "" {
		source = "system"
	}
	if err := validateJournalSource(source); err != nil {
		return nil, err
	}

	opts, err := normalizeLogOptions(config.Options)
	if err != nil {
		return nil, err
	}

	machineDir := filepath.Join(dir, opts.MachineID.String())
	if err := os.MkdirAll(machineDir, 0o750); err != nil {
		return nil, err
	}

	l := &Log{
		machineDir:    machineDir,
		source:        source,
		options:       opts,
		rotation:      config.RotationPolicy,
		retention:     config.RetentionPolicy,
		entriesInFile: 0,
	}

	activePath := l.activePath()
	if _, err := os.Stat(activePath); err == nil {
		w, err := Open(activePath)
		if err != nil {
			return nil, err
		}
		l.writer = w
		l.options.SeqnumID = w.header.seqnumID
		l.options.BootID = w.bootID
		l.entriesInFile = int(w.header.nEntries)
	} else if !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}

	if err := l.enforceRetention(); err != nil {
		if l.writer != nil {
			_ = l.writer.Close()
			l.writer = nil
		}
		return nil, err
	}
	return l, nil
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
	if err := l.ensureWriter(); err != nil {
		return err
	}
	if l.shouldRotate() {
		if err := l.rotate(); err != nil {
			return err
		}
	}
	if err := l.writer.Append(fields, opts); err != nil {
		return err
	}
	l.entriesInFile++
	return nil
}

// AppendMap appends a string-valued entry through the directory writer.
func (l *Log) AppendMap(fields map[string]string) error {
	keys := make([]string, 0, len(fields))
	for k := range fields {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	entry := make([]Field, 0, len(keys))
	for _, k := range keys {
		entry = append(entry, StringField(k, fields[k]))
	}
	return l.Append(entry, EntryOptions{})
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
	if l.writer.header.nEntries == 0 {
		err1 := l.writer.Close()
		err2 := os.Remove(l.activePath())
		if errors.Is(err2, os.ErrNotExist) {
			err2 = nil
		}
		l.writer = nil
		if err := errors.Join(err1, err2); err != nil {
			l.closed = true
			return err
		}
		l.closed = true
		return nil
	}
	if err := l.archiveActive(); err != nil {
		if l.writer == nil {
			l.closed = true
		}
		return err
	}
	if err := l.enforceRetention(); err != nil {
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
	for _, field := range fields {
		if err := validateFieldName(field.Name); err != nil {
			return err
		}
	}
	return nil
}

// ActivePath returns the active journal path for this log directory.
func (l *Log) ActivePath() string {
	return l.activePath()
}

// JournalDirectory returns the machine-id directory containing this log's
// journal files.
func (l *Log) JournalDirectory() string {
	return l.machineDir
}

func (l *Log) ensureWriter() error {
	if l.writer != nil {
		return nil
	}
	opts := l.options
	opts.FileID = UUID{}
	if opts.HeadSeqnum == 0 {
		opts.HeadSeqnum = 1
	}
	w, err := Create(l.activePath(), opts)
	if err != nil {
		return err
	}
	l.writer = w
	l.entriesInFile = 0
	return nil
}

func (l *Log) shouldRotate() bool {
	if l.writer == nil {
		return false
	}
	if l.rotation.MaxEntries > 0 && l.entriesInFile >= l.rotation.MaxEntries {
		return true
	}
	return l.writer.header.nEntries > 0 &&
		l.rotation.MaxFileSize > 0 &&
		l.writer.CurrentSize() >= l.rotation.MaxFileSize
}

func (l *Log) rotate() error {
	if l.writer == nil {
		return l.ensureWriter()
	}
	nextSeqnum := l.writer.nextSeqnum
	seqnumID := l.writer.header.seqnumID
	bootID := l.writer.bootID
	if err := l.archiveActive(); err != nil {
		return err
	}
	l.options.SeqnumID = seqnumID
	l.options.BootID = bootID
	l.options.HeadSeqnum = nextSeqnum
	if err := l.enforceRetention(); err != nil {
		return err
	}
	return l.ensureWriter()
}

func (l *Log) archiveActive() error {
	if l.writer == nil {
		return nil
	}
	archivePath := l.archivePathFor(l.writer.header)
	if err := l.writer.archiveTo(archivePath); err != nil {
		if l.writer.closed {
			l.writer = nil
			l.entriesInFile = 0
		}
		return err
	}
	l.writer = nil
	l.entriesInFile = 0
	return nil
}

func (l *Log) enforceRetention() error {
	files, total, err := l.archivedFiles()
	if err != nil {
		return err
	}
	if activeInfo, err := os.Stat(l.activePath()); err == nil {
		total += uint64(activeInfo.Size())
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
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

	for l.retention.MaxFiles > 0 && len(files) > l.retention.MaxFiles {
		deleted := files[0]
		if err := os.Remove(deleted.path); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		total = saturatingSub(total, deleted.size)
		files = files[1:]
	}
	for l.retention.MaxBytes > 0 && total > l.retention.MaxBytes && len(files) > 0 {
		deleted := files[0]
		if err := os.Remove(deleted.path); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		total = saturatingSub(total, deleted.size)
		files = files[1:]
	}
	return syncJournalDirectory(l.machineDir)
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
		if err != nil {
			return nil, 0, err
		}
		archived.path = filepath.Join(l.machineDir, entry.Name())
		archived.size = uint64(info.Size())
		files = append(files, archived)
		total += archived.size
	}
	return files, total, nil
}

func (l *Log) activePath() string {
	return filepath.Join(l.machineDir, l.source+".journal")
}

func (l *Log) archivePathFor(header journalHeader) string {
	name := fmt.Sprintf("%s@%s-%016x-%016x.journal",
		l.source,
		header.seqnumID.String(),
		header.headEntrySeqnum,
		header.headEntryRealtime)
	return filepath.Join(l.machineDir, name)
}

func normalizeLogOptions(opts Options) (Options, error) {
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
