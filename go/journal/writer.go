package journal

import (
	"bytes"
	"crypto/rand"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"os"
	"sort"
	"sync/atomic"
	"time"
)

// UUID is a 128-bit identifier stored in journal files.
type UUID [16]byte

// NewUUID returns a random UUID-shaped identifier.
func NewUUID() (UUID, error) {
	var id UUID
	_, err := io.ReadFull(rand.Reader, id[:])
	return id, err
}

// Options controls journal file creation.
type Options struct {
	MachineID UUID
	BootID    UUID
	SeqnumID  UUID
	FileID    UUID
	// HeadSeqnum is the sequence number assigned to the first entry in a newly
	// created file. It defaults to 1. Directory writers use it to continue
	// sequence numbers across rotated files.
	HeadSeqnum uint64
	// MaxFileSize controls systemd-compatible hash-table sizing for newly
	// created files. A zero value uses the SDK default 128 MiB sizing.
	MaxFileSize           uint64
	DataHashTableBuckets  int
	FieldHashTableBuckets int
	// Compression specifies the compression algorithm for DATA objects.
	// Defaults to CompressionNone.
	Compression int
	// CompressThresholdBytes is the minimum uncompressed payload size in bytes
	// required before compression is attempted. Defaults to systemd's 512-byte
	// threshold. A zero value uses the default; non-zero values below 8 bytes
	// are clamped to 8.
	CompressThresholdBytes int
	// Compact writes the systemd compact journal object layout. Regular layout
	// remains the default for existing deterministic fixtures.
	Compact bool
	// Seal enables Forward Secure Sealing with deterministic synthetic keys.
	// When non-nil, the writer appends TAG objects and sets sealed header flags.
	Seal *SealOptions
	// LivePublishEveryEntries controls explicit live-reader publication cadence.
	// Nil uses the default systemd-compatible cadence of 1. A value of 0 disables
	// explicit per-entry publication; values greater than 1 publish after every N
	// appended entries.
	LivePublishEveryEntries *uint64
	// FieldNamePolicy controls validation for caller-provided fields. The zero
	// value is FieldNamePolicyJournald.
	FieldNamePolicy FieldNamePolicy
	// FileMode controls permissions for newly created journal files on platforms
	// that support POSIX file modes. Nil uses systemd journald's 0640 default.
	FileMode *os.FileMode
}

// EntryOptions controls timestamps and boot ID for one appended entry.
type EntryOptions struct {
	RealtimeUsec uint64
	// RealtimeUsecSet marks RealtimeUsec as caller-provided even when it is zero.
	RealtimeUsecSet bool
	MonotonicUsec   uint64
	// MonotonicUsecSet marks MonotonicUsec as caller-provided even when it is zero.
	MonotonicUsecSet bool
	BootID           UUID
	// Seqnum is an optional low-level ENTRY seqnum override for exact journal
	// regeneration. Leave zero for the normal auto-incrementing sequence.
	Seqnum uint64
	// SourceRealtimeUsec is consumed by the high-level Log writer, which injects
	// _SOURCE_REALTIME_TIMESTAMP. The low-level Writer accepts prebuilt fields
	// and does not inject this field.
	SourceRealtimeUsec uint64
}

// Field is one FIELD=value item in a journal entry.
type Field struct {
	Name  string
	Value []byte
}

// StringField creates a Field from a string value.
func StringField(name, value string) Field {
	return Field{Name: name, Value: []byte(value)}
}

// Writer appends entries to a systemd journal file.
type Writer struct {
	file              *os.File
	path              string
	arena             *mappedArena
	header            journalHeader
	appendOffset      uint64
	nextSeqnum        uint64
	bootID            UUID
	closed            bool
	failure           error
	appendMutated     bool
	compression       int
	compressThreshold int
	compact           bool
	seal              *sealState
	fieldCache        fieldCache
	payloadScratch    []byte
	entryItemsScratch []entryItem
	// Full memory-ordering point before same-size ftruncate wakes stock follow readers.
	postChangeFence             atomic.Uint64
	livePublishEveryEntries     uint64
	entriesSinceLivePublication uint64
	fieldNamePolicy             FieldNamePolicy
}

// ErrWriterFailed marks an uncertain mutating failure. The writer cannot be
// reused; Close only releases resources. Run VerifyIndex before recovering the
// file with a new writer. The original failure remains available via errors.Is.
var ErrWriterFailed = errors.New("journal: writer failed; verify before recovery")

func (w *Writer) writable() error {
	if w.failure != nil {
		return w.failure
	}
	if w.closed {
		return errWriterClosed
	}
	return nil
}

func (w *Writer) fail(err error) error {
	if err != nil && w.failure == nil {
		w.failure = errors.Join(ErrWriterFailed, err)
	}
	return w.failure
}

var syncArchiveJournalFile = func(w *Writer) error {
	return w.syncArena()
}

// PublishEveryEntries returns a pointer suitable for Options.LivePublishEveryEntries.
func PublishEveryEntries(entries uint64) *uint64 {
	return &entries
}

// SyncOnArchive returns a pointer suitable for LogConfig.SyncOnArchive.
func SyncOnArchive(enabled bool) *bool {
	return &enabled
}

// JournalFileMode returns a pointer suitable for Options.FileMode.
func JournalFileMode(mode os.FileMode) *os.FileMode {
	return &mode
}

func validateFileMode(mode *os.FileMode) error {
	if mode == nil || *mode&^os.ModePerm == 0 {
		return nil
	}
	return fmt.Errorf("%w: journal file mode must contain only permission bits", errInvalidJournal)
}

// Create creates or truncates a journal file.
//
// The strict writer contract requires explicit non-zero Options.MachineID and
// Options.BootID. A zero value in either field returns ErrMissingMachineID or
// ErrMissingBootID before any file mutation.
func Create(path string, opts Options) (*Writer, error) {
	opts, err := normalizeOptions(opts)
	if err != nil {
		return nil, err
	}
	if !validCompression(opts.Compression) {
		return nil, fmt.Errorf("unsupported journal compression: %d", opts.Compression)
	}
	if err := validateFieldNamePolicy(opts.FieldNamePolicy); err != nil {
		return nil, err
	}
	if err := validateFileMode(opts.FileMode); err != nil {
		return nil, err
	}

	f, err := openWriterFile(path, true, *opts.FileMode)
	if err != nil {
		return nil, err
	}
	if err := f.Truncate(0); err != nil {
		return nil, errors.Join(ErrWriterFailed, err, f.Close())
	}

	w := &Writer{
		file: f, path: path, bootID: opts.BootID,
		compression: opts.Compression, compressThreshold: opts.CompressThresholdBytes, compact: opts.Compact,
		livePublishEveryEntries: livePublishEveryEntries(opts),
		fieldNamePolicy:         opts.FieldNamePolicy,
	}
	if err := w.initialize(opts); err != nil {
		return nil, errors.Join(w.fail(err), w.release())
	}
	return w, nil
}

// Open opens a journal file created by this package for appending.
func Open(path string) (*Writer, error) {
	return OpenWithOptions(path, Options{})
}

// OpenWithOptions opens a journal file created by this package for appending,
// using options that affect future appends.
//
// The strict writer contract does not require an explicit machine ID or boot
// ID at open time. On-disk tail state supplies them when the file has
// entries. For files without a tail boot ID, callers must supply an explicit
// non-zero Options.BootID before the first append; the writer rejects the
// append with ErrMissingBootID instead of inventing a value.
// Errors before mutation leave the file unchanged. Once writable mapping
// (which may resize the file) begins, failures include ErrWriterFailed because
// the file may have changed and must be inspected before recovery.
func OpenWithOptions(path string, opts Options) (*Writer, error) {
	opts = normalizeOpenOptions(opts)
	if err := validateOpenOptions(opts); err != nil {
		return nil, err
	}
	f, err := openWriterFile(path, false, 0)
	if err != nil {
		return nil, err
	}
	w, err := newAppendWriter(path, f, opts)
	if err != nil {
		return nil, errors.Join(err, f.Close())
	}
	return w, nil
}

func validateOpenOptions(opts Options) error {
	if err := validateFieldNamePolicy(opts.FieldNamePolicy); err != nil {
		return err
	}
	if !isZeroUUID(opts.MachineID) && isZeroUUID(opts.BootID) {
		return ErrMissingBootID
	}
	if isZeroUUID(opts.MachineID) && !isZeroUUID(opts.BootID) {
		return ErrMissingMachineID
	}
	return nil
}

func newAppendWriter(path string, f *os.File, opts Options) (*Writer, error) {
	header, err := readAppendHeader(f)
	if err != nil {
		return nil, err
	}
	if err := validateAppendHeader(header); err != nil {
		return nil, err
	}

	stat, err := f.Stat()
	if err != nil {
		return nil, err
	}
	fileSize := uint64(stat.Size())
	arenaEnd, err := header.validateDeclaredArena(fileSize)
	if err != nil {
		return nil, err
	}
	if err := header.validateEmptyEntryMetadata(); err != nil {
		return nil, err
	}
	tail, err := readObjectHeaderAt(f, header.tailObjectOffset)
	if err != nil {
		return nil, err
	}
	// Mapping resizes the file, so prove the complete tail fits before mapping.
	if err := header.validateArenaObject(header.tailObjectOffset, tail.size, arenaEnd); err != nil {
		return nil, err
	}
	if header.isCompact() && fileSize > journalCompactSizeMax {
		return nil, fmt.Errorf("%w: compact journal cannot exceed 4 GiB", errInvalidJournal)
	}

	// Reject unsupported mapping sizes before entering the mutation boundary.
	if err := checkArenaSize(fileSize); err != nil {
		return nil, err
	}

	// Retain physical preallocation in the writable arena so mapping and later
	// publication cannot truncate padding beyond the original declared arena.
	header.arenaSize = fileSize - header.headerSize
	header.state = stateOnline
	w := &Writer{
		file:                    f,
		path:                    path,
		header:                  header,
		appendOffset:            align8(header.tailObjectOffset + tail.size),
		nextSeqnum:              header.tailEntrySeqnum + 1,
		bootID:                  header.tailEntryBootID,
		compression:             appendHeaderCompression(header),
		compressThreshold:       defaultCompressThreshold,
		compact:                 header.isCompact(),
		livePublishEveryEntries: livePublishEveryEntries(opts),
		fieldNamePolicy:         opts.FieldNamePolicy,
	}
	if err := w.applyAppendBootID(opts); err != nil {
		return nil, err
	}
	// Writable mapping can resize the file. From this boundary onward a failed
	// open requires verification, just like a failed append or archive mutation.
	if err := w.mapArena(fileSize); err != nil {
		return nil, w.fail(err)
	}
	if err := w.writeHeader(); err != nil {
		return nil, errors.Join(w.fail(err), w.closeArena())
	}
	return w, nil
}

func (w *Writer) applyAppendBootID(opts Options) error {
	if !isZeroUUID(w.bootID) {
		return nil
	}
	if !isZeroUUID(opts.BootID) {
		w.bootID = opts.BootID
		return nil
	}
	if w.header.nEntries > 0 {
		return fmt.Errorf("%w: cannot open existing file without a tail boot id", ErrMissingBootID)
	}
	return nil
}

func readAppendHeader(f *os.File) (journalHeader, error) {
	buf := make([]byte, headerSize)
	if _, err := f.ReadAt(buf, 0); err != nil {
		return journalHeader{}, err
	}
	return parseHeader(buf)
}

func validateAppendHeader(header journalHeader) error {
	const supportedWriterIncompatible = incompatibleKeyedHash | incompatibleCompressedZSTD | incompatibleCompressedXZ | incompatibleCompressedLZ4 | incompatibleCompact
	if header.incompatibleFlags&^supportedWriterIncompatible != 0 {
		return errUnsupportedJournal
	}
	if header.incompatibleFlags&incompatibleKeyedHash == 0 {
		return errUnsupportedJournal
	}
	if header.headerSize < headerSize {
		return errUnsupportedJournal
	}
	if header.dataHashTableOffset == 0 || header.fieldHashTableOffset == 0 || header.tailObjectOffset == 0 {
		return errInvalidJournal
	}
	return nil
}

func appendHeaderCompression(header journalHeader) int {
	switch {
	case header.incompatibleFlags&incompatibleCompressedZSTD != 0:
		return CompressionZSTD
	case header.incompatibleFlags&incompatibleCompressedXZ != 0:
		return CompressionXZ
	case header.incompatibleFlags&incompatibleCompressedLZ4 != 0:
		return CompressionLZ4
	default:
		return CompressionNone
	}
}

// AppendMap appends a string-valued entry with deterministic field ordering.
// Under the strict writer contract this compatibility wrapper returns
// ErrMissingMonotonicUsec; use AppendMapWithOptions for new code.
func (w *Writer) AppendMap(fields map[string]string) error {
	return w.AppendMapWithOptions(fields, EntryOptions{})
}

// AppendMapWithOptions appends a string-valued entry with deterministic field
// ordering and explicit entry metadata.
func (w *Writer) AppendMapWithOptions(fields map[string]string, opts EntryOptions) error {
	keys := make([]string, 0, len(fields))
	for k := range fields {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	entry := make([]Field, 0, len(keys))
	for _, k := range keys {
		entry = append(entry, StringField(k, fields[k]))
	}
	return w.Append(entry, opts)
}

// Append appends one journal entry.
func (w *Writer) Append(fields []Field, opts EntryOptions) error {
	if err := w.writable(); err != nil {
		return err
	}
	preparedFields, err := prepareFieldsForPolicy(fields, w.fieldNamePolicy)
	if err != nil {
		return err
	}
	fields = preparedFields
	return w.appendPayloads(len(fields), func(i int) []byte {
		field := fields[i]
		w.payloadScratch = append(w.payloadScratch[:0], field.Name...)
		w.payloadScratch = append(w.payloadScratch, '=')
		w.payloadScratch = append(w.payloadScratch, field.Value...)
		return w.payloadScratch
	}, opts)
}

// AppendRaw appends one journal entry from complete KEY=value byte payloads.
// The first '=' byte separates the field name from the value; later '=' bytes
// and arbitrary value bytes are preserved.
func (w *Writer) AppendRaw(payloads [][]byte, opts EntryOptions) error {
	if err := w.writable(); err != nil {
		return err
	}
	preparedPayloads, err := prepareRawPayloadsForPolicy(payloads, w.fieldNamePolicy)
	if err != nil {
		return err
	}
	payloads = preparedPayloads
	return w.appendPayloads(len(payloads), func(i int) []byte {
		return payloads[i]
	}, opts)
}

func (w *Writer) appendPayloads(count int, payloadAt func(int) []byte, opts EntryOptions) (err error) {
	if count == 0 {
		return errEntryEmpty
	}

	opts, entrySeqnum, err := w.prepareEntryOptions(opts)
	if err != nil {
		return err
	}

	// Storage and sealing primitives mark the first possible mutation. Read-only
	// lookup and capacity failures leave the writer reusable.
	w.appendMutated = false
	defer func() {
		if err != nil && w.appendMutated {
			err = w.fail(err)
		}
	}()

	if err := w.maybeAppendTag(opts.RealtimeUsec); err != nil {
		return err
	}

	items := w.prepareEntryItemsScratch(count)
	defer func() {
		w.entryItemsScratch = items[:0]
		if cap(w.payloadScratch) > payloadScratchMaxRetain {
			w.payloadScratch = nil
		}
	}()

	items, xorHash, err := w.collectEntryItems(items, count, payloadAt)
	if err != nil {
		return err
	}

	entryOffset := w.appendOffset
	if err := w.writeEntryObject(entryOffset, items, entrySeqnum, opts, xorHash); err != nil {
		return err
	}
	if err := w.publishEntryObject(entryOffset, items, entrySeqnum, opts); err != nil {
		return err
	}
	return w.publishAfterEntry()
}

// ErrMissingMonotonicUsec is returned by Append/AppendRaw when the caller
// does not provide an explicit monotonic timestamp and the writer cannot
// fall back to a process-relative or wall-clock value. Callers must supply
// EntryOptions.MonotonicUsec or MonotonicUsecSet=true.
var ErrMissingMonotonicUsec = fmt.Errorf("journal: monotonic usec is required")

// ErrMonotonicUsecOverflow is returned when the writer cannot clamp a
// same-boot monotonic timestamp because the existing tail timestamp is already
// at the maximum uint64 value.
var ErrMonotonicUsecOverflow = fmt.Errorf("journal: monotonic usec overflow")

func validateEntryMonotonicOptions(opts EntryOptions) error {
	if opts.MonotonicUsec == 0 && !opts.MonotonicUsecSet {
		return ErrMissingMonotonicUsec
	}
	return nil
}

func (w *Writer) prepareEntryOptions(opts EntryOptions) (EntryOptions, uint64, error) {
	now := time.Now()
	if opts.RealtimeUsec == 0 && !opts.RealtimeUsecSet {
		opts.RealtimeUsec = uint64(now.UnixMicro())
	}
	if err := validateEntryMonotonicOptions(opts); err != nil {
		return opts, 0, err
	}
	if isZeroUUID(opts.BootID) {
		opts.BootID = w.bootID
	}
	if isZeroUUID(opts.BootID) {
		return opts, 0, ErrMissingBootID
	}
	if opts.BootID == w.header.tailEntryBootID && opts.MonotonicUsec <= w.header.tailEntryMonotonic {
		if w.header.tailEntryMonotonic == ^uint64(0) {
			return opts, 0, ErrMonotonicUsecOverflow
		}
		opts.MonotonicUsec = w.header.tailEntryMonotonic + 1
	}
	entrySeqnum := w.nextSeqnum
	if opts.Seqnum != 0 {
		if opts.Seqnum < w.nextSeqnum || opts.Seqnum == ^uint64(0) {
			return opts, 0, errInvalidJournal
		}
		entrySeqnum = opts.Seqnum
	}
	if w.seal != nil {
		if _, err := w.seal.needEvolve(opts.RealtimeUsec); err != nil {
			return opts, 0, err
		}
	}
	return opts, entrySeqnum, nil
}

func (w *Writer) prepareEntryItemsScratch(count int) []entryItem {
	items := w.entryItemsScratch[:0]
	if cap(items) < count {
		return make([]entryItem, 0, count)
	}
	return items
}

func (w *Writer) collectEntryItems(items []entryItem, count int, payloadAt func(int) []byte) ([]entryItem, uint64, error) {
	xorHash := uint64(0)
	for i := 0; i < count; i++ {
		payload := payloadAt(i)
		offset, hash, linkState, err := w.addData(payload)
		if err != nil {
			return nil, 0, err
		}
		items = append(items, entryItem{offset: offset, hash: hash, linkState: linkState})
		xorHash ^= jenkinsHash64(payload)
	}
	sort.Slice(items, func(i, j int) bool { return items[i].offset < items[j].offset })
	return dedupeEntryItems(items), xorHash, nil
}

func (w *Writer) writeEntryObject(entryOffset uint64, items []entryItem, entrySeqnum uint64, opts EntryOptions, xorHash uint64) error {
	itemSize := w.entryItemSize()
	entrySize := uint64(entryObjectHeaderSize + len(items)*int(itemSize))
	if err := w.ensureCompactObjectFits(entryOffset, entrySize); err != nil {
		return err
	}
	buf, direct, err := w.newObjectBuffer(entryOffset, entrySize)
	if err != nil {
		return err
	}
	putEntryHeader(buf[:entryObjectHeaderSize], entryHeader{
		object: objectHeader{typ: objectTypeEntry, size: entrySize},
		seqnum: entrySeqnum, realtime: opts.RealtimeUsec,
		monotonic: opts.MonotonicUsec, bootID: opts.BootID, xorHash: xorHash,
	})
	for i, item := range items {
		off := entryObjectHeaderSize + i*int(itemSize)
		if w.compact {
			if item.offset > journalCompactSizeMax {
				return fmt.Errorf("%w: compact object offset exceeds 32-bit range", errInvalidJournal)
			}
			binary.LittleEndian.PutUint32(buf[off:off+compactEntryItemSize], uint32(item.offset))
		} else {
			binary.LittleEndian.PutUint64(buf[off:off+8], item.offset)
			binary.LittleEndian.PutUint64(buf[off+8:off+16], item.hash)
		}
	}
	return w.commitObjectBuffer(entryOffset, buf, direct)
}

func (w *Writer) publishEntryObject(entryOffset uint64, items []entryItem, entrySeqnum uint64, opts EntryOptions) error {
	entrySize := uint64(entryObjectHeaderSize + len(items)*int(w.entryItemSize()))
	if err := w.objectAdded(entryOffset, entrySize); err != nil {
		return err
	}
	// Publish object reachability only after the complete entry object exists.
	// Entry count is committed last below so live stock readers see full rows.
	if err := w.publishObjectMetadata(); err != nil {
		return err
	}

	if err := w.hmacPutObject(entryOffset, objectTypeEntry); err != nil {
		return err
	}

	if err := w.appendToEntryArray(entryOffset); err != nil {
		return err
	}
	for _, item := range items {
		if err := w.linkDataToEntry(item.offset, entryOffset, item.linkState); err != nil {
			return err
		}
	}
	w.entryAdded(entryOffset, entrySeqnum, opts.RealtimeUsec, opts.MonotonicUsec, opts.BootID)
	if err := w.publishEntryMetadata(); err != nil {
		return err
	}
	return nil
}

// Sync flushes file data and metadata to disk.
func (w *Writer) Sync() error {
	if err := w.writable(); err != nil {
		return err
	}
	if err := w.writeHeader(); err != nil {
		return w.fail(err)
	}
	return w.fail(w.syncArena())
}

// Close syncs and closes the journal, leaving its online state unchanged. After
// a mutating failure it only releases resources and returns the stored failure.
// Subsequent Close calls return nil once resources have been released.
func (w *Writer) Close() error {
	return w.closeWithState(stateOnline)
}

// CloseOffline marks the journal offline, syncs it, and closes it. After a
// mutating failure it only releases resources and returns the stored failure.
func (w *Writer) CloseOffline() error {
	return w.closeWithState(stateOffline)
}

func (w *Writer) closeWithState(state uint8) error {
	if w.closed {
		return nil
	}
	if w.failure == nil {
		w.header.state = state
		if err := w.writeHeader(); err != nil {
			w.fail(err)
		} else {
			w.fail(w.syncArena())
		}
	}
	closeErr := w.release()
	if w.failure != nil {
		return errors.Join(w.failure, closeErr)
	}
	return w.fail(closeErr)
}

// release never writes metadata or syncs an uncertain journal.
func (w *Writer) release() error {
	arenaErr := w.closeArena()
	closeErr := w.file.Close()
	w.closed = true
	return errors.Join(arenaErr, closeErr)
}

// CurrentSize returns the current committed journal file size in bytes.
func (w *Writer) CurrentSize() uint64 {
	return w.appendOffset
}

// ArchiveTo marks the journal archived, renames it, syncs the parent
// directory, and closes it.
func (w *Writer) ArchiveTo(path string) error {
	return w.archiveTo(path, true)
}

func (w *Writer) archiveTo(path string, syncOnArchive bool) (err error) {
	if err := w.writable(); err != nil {
		return err
	}
	defer func() {
		if err != nil {
			err = w.fail(err)
		}
	}()
	w.header.state = stateArchived
	if err := w.writeHeader(); err != nil {
		return err
	}
	if syncOnArchive {
		if err := syncArchiveJournalFile(w); err != nil {
			return err
		}
	}
	if w.path != path {
		if err := os.Rename(w.path, path); err != nil {
			return err
		}
	}
	w.path = path
	dirErr := syncJournalDirectory(path)
	if err := errors.Join(dirErr, w.release()); err != nil {
		return err
	}
	return nil
}

type entryItem struct {
	offset    uint64
	hash      uint64
	linkState resolvedDataLinkState
}

type resolvedDataLinkState struct {
	nEntries           uint64
	entryArrayOffset   uint64
	compactTailOffset  uint64
	compactTailEntries uint64
}

const (
	fieldCacheSlots         = 1024
	fieldCacheMaxPayloadLen = 128
	payloadScratchMaxRetain = 1 << 20
)

type fieldCacheEntry struct {
	payload        []byte
	offset         uint64
	headDataOffset uint64
}

type fieldCache struct {
	entries []fieldCacheEntry
}

func (c *fieldCache) get(hash uint64, payload []byte) (uint64, uint64, bool) {
	if len(payload) > fieldCacheMaxPayloadLen || len(c.entries) == 0 {
		return 0, 0, false
	}
	entry := c.entries[int(hash)&(fieldCacheSlots-1)]
	if entry.offset == 0 || !bytes.Equal(entry.payload, payload) {
		return 0, 0, false
	}
	return entry.offset, entry.headDataOffset, true
}

func (c *fieldCache) insert(hash uint64, payload []byte, offset, headDataOffset uint64) {
	if len(payload) > fieldCacheMaxPayloadLen {
		return
	}
	if len(c.entries) == 0 {
		c.entries = make([]fieldCacheEntry, fieldCacheSlots)
	}
	entry := &c.entries[int(hash)&(fieldCacheSlots-1)]
	entry.payload = append(entry.payload[:0], payload...)
	entry.offset = offset
	entry.headDataOffset = headDataOffset
}
