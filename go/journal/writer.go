package journal

import (
	"bytes"
	"crypto/rand"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"sort"
	"sync/atomic"
	"time"

	"github.com/klauspost/compress/zstd"
	"github.com/pierrec/lz4/v4"
	"github.com/ulikunitz/xz"
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
	started           time.Time
	closed            bool
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

// PublishEveryEntries returns a pointer suitable for Options.LivePublishEveryEntries.
func PublishEveryEntries(entries uint64) *uint64 {
	return &entries
}

// Create creates or truncates a journal file.
func Create(path string, opts Options) (*Writer, error) {
	opts = normalizeOptions(opts)
	if !validCompression(opts.Compression) {
		return nil, fmt.Errorf("unsupported journal compression: %d", opts.Compression)
	}
	if err := validateFieldNamePolicy(opts.FieldNamePolicy); err != nil {
		return nil, err
	}

	f, err := openWriterFile(path, true, 0o640)
	if err != nil {
		return nil, err
	}
	if err := f.Truncate(0); err != nil {
		_ = f.Close()
		return nil, err
	}

	w := &Writer{
		file: f, path: path, bootID: opts.BootID, started: time.Now(),
		compression: opts.Compression, compressThreshold: opts.CompressThresholdBytes, compact: opts.Compact,
		livePublishEveryEntries: livePublishEveryEntries(opts),
		fieldNamePolicy:         opts.FieldNamePolicy,
	}
	if err := w.initialize(opts); err != nil {
		_ = w.closeArena()
		_ = f.Close()
		return nil, err
	}
	return w, nil
}

// Open opens a journal file created by this package for appending.
func Open(path string) (*Writer, error) {
	return OpenWithOptions(path, Options{})
}

// OpenWithOptions opens a journal file created by this package for appending,
// using options that affect future appends.
func OpenWithOptions(path string, opts Options) (*Writer, error) {
	opts = normalizeOpenOptions(opts)
	if err := validateFieldNamePolicy(opts.FieldNamePolicy); err != nil {
		return nil, err
	}
	f, err := openWriterFile(path, false, 0)
	if err != nil {
		return nil, err
	}

	header, err := readAppendHeader(f)
	if err != nil {
		_ = f.Close()
		return nil, err
	}
	if err := validateAppendHeader(header); err != nil {
		_ = f.Close()
		return nil, err
	}

	tail, err := readObjectHeaderAt(f, header.tailObjectOffset)
	if err != nil {
		_ = f.Close()
		return nil, err
	}

	header.state = stateOnline
	now := time.Now()
	w := &Writer{
		file:                    f,
		path:                    path,
		header:                  header,
		appendOffset:            align8(header.tailObjectOffset + tail.size),
		nextSeqnum:              header.tailEntrySeqnum + 1,
		bootID:                  header.tailEntryBootID,
		started:                 startTimeForTailMonotonic(now, header.tailEntryMonotonic),
		compression:             appendHeaderCompression(header),
		compressThreshold:       defaultCompressThreshold,
		compact:                 header.isCompact(),
		livePublishEveryEntries: livePublishEveryEntries(opts),
		fieldNamePolicy:         opts.FieldNamePolicy,
	}
	fileSize, ok := checkedAdd(header.headerSize, header.arenaSize)
	if !ok {
		_ = f.Close()
		return nil, errInvalidJournal
	}
	if err := w.mapArena(fileSize); err != nil {
		_ = f.Close()
		return nil, err
	}
	if isZeroUUID(w.bootID) {
		if !isZeroUUID(opts.BootID) {
			w.bootID = opts.BootID
		} else {
			w.bootID = header.fileID
		}
	}
	if err := w.writeHeader(); err != nil {
		_ = w.closeArena()
		_ = f.Close()
		return nil, err
	}
	return w, nil
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
func (w *Writer) AppendMap(fields map[string]string) error {
	keys := make([]string, 0, len(fields))
	for k := range fields {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	entry := make([]Field, 0, len(keys))
	for _, k := range keys {
		entry = append(entry, StringField(k, fields[k]))
	}
	return w.Append(entry, EntryOptions{})
}

// Append appends one journal entry.
func (w *Writer) Append(fields []Field, opts EntryOptions) error {
	if w.closed {
		return errWriterClosed
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
	if w.closed {
		return errWriterClosed
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

func (w *Writer) appendPayloads(count int, payloadAt func(int) []byte, opts EntryOptions) error {
	if count == 0 {
		return errEntryEmpty
	}

	opts, entrySeqnum, err := w.prepareEntryOptions(opts)
	if err != nil {
		return err
	}

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

func (w *Writer) prepareEntryOptions(opts EntryOptions) (EntryOptions, uint64, error) {
	now := time.Now()
	if opts.RealtimeUsec == 0 && !opts.RealtimeUsecSet {
		opts.RealtimeUsec = uint64(now.UnixMicro())
	}
	if opts.MonotonicUsec == 0 && !opts.MonotonicUsecSet {
		opts.MonotonicUsec = uint64(now.Sub(w.started) / time.Microsecond)
	}
	if isZeroUUID(opts.BootID) {
		opts.BootID = w.bootID
	}
	entrySeqnum := w.nextSeqnum
	if opts.Seqnum != 0 {
		if opts.Seqnum < w.nextSeqnum || opts.Seqnum == ^uint64(0) {
			return opts, 0, errInvalidJournal
		}
		entrySeqnum = opts.Seqnum
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
		offset, hash, err := w.addData(payload)
		if err != nil {
			return nil, 0, err
		}
		items = append(items, entryItem{offset: offset, hash: hash})
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
		if err := w.linkDataToEntry(item.offset, entryOffset); err != nil {
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
	if w.closed {
		return errWriterClosed
	}
	if err := w.writeHeader(); err != nil {
		return err
	}
	return w.syncArena()
}

func (w *Writer) Close() error {
	return w.closeWithState(stateOnline)
}

// CloseOffline marks the journal offline, syncs it, and closes it.
func (w *Writer) CloseOffline() error {
	return w.closeWithState(stateOffline)
}

func (w *Writer) closeWithState(state uint8) error {
	if w.closed {
		return nil
	}
	w.header.state = state
	err1 := w.writeHeader()
	err2 := w.syncArena()
	err3 := w.closeArena()
	err4 := w.file.Close()
	w.closed = true
	return errors.Join(err1, err2, err3, err4)
}

// CurrentSize returns the current committed journal file size in bytes.
func (w *Writer) CurrentSize() uint64 {
	return w.appendOffset
}

// ArchiveTo marks the journal archived, renames it, syncs the parent
// directory, and closes it.
func (w *Writer) ArchiveTo(path string) error {
	return w.archiveTo(path)
}

func (w *Writer) archiveTo(path string) error {
	if w.closed {
		return errWriterClosed
	}
	w.header.state = stateArchived
	if err := w.writeHeader(); err != nil {
		return err
	}
	if err := w.syncArena(); err != nil {
		return err
	}
	if w.path != path {
		if err := os.Rename(w.path, path); err != nil {
			w.header.state = stateOnline
			restoreErr := w.writeHeader()
			syncErr := w.syncArena()
			return errors.Join(err, restoreErr, syncErr)
		}
	}
	w.path = path
	dirErr := syncJournalDirectory(path)
	arenaErr := w.closeArena()
	closeErr := w.file.Close()
	w.closed = true
	if err := errors.Join(dirErr, arenaErr, closeErr); err != nil {
		return err
	}
	return nil
}

type entryItem struct {
	offset uint64
	hash   uint64
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

func normalizeOptions(opts Options) Options {
	if isZeroUUID(opts.MachineID) {
		opts.MachineID = mustRandomUUID()
	}
	if isZeroUUID(opts.BootID) {
		opts.BootID = mustRandomUUID()
	}
	if isZeroUUID(opts.SeqnumID) {
		opts.SeqnumID = mustRandomUUID()
	}
	if isZeroUUID(opts.FileID) {
		opts.FileID = mustRandomUUID()
	}
	if opts.HeadSeqnum == 0 {
		opts.HeadSeqnum = 1
	}
	maxFileSize := normalizeJournalMaxFileSize(opts.MaxFileSize, opts.Compact)
	if opts.MaxFileSize == 0 {
		opts.MaxFileSize = maxFileSize
	}
	if opts.DataHashTableBuckets == 0 {
		opts.DataHashTableBuckets = dataHashBucketsForMaxFileSize(maxFileSize)
	}
	if opts.FieldHashTableBuckets == 0 {
		opts.FieldHashTableBuckets = defaultFieldHashBuckets
	}
	if opts.CompressThresholdBytes == 0 {
		opts.CompressThresholdBytes = defaultCompressThreshold
	} else if opts.CompressThresholdBytes < minCompressThreshold {
		opts.CompressThresholdBytes = minCompressThreshold
	}
	return opts
}

func normalizeOpenOptions(opts Options) Options {
	if opts.LivePublishEveryEntries == nil {
		opts.LivePublishEveryEntries = PublishEveryEntries(1)
	}
	return opts
}

func livePublishEveryEntries(opts Options) uint64 {
	if opts.LivePublishEveryEntries == nil {
		return 1
	}
	return *opts.LivePublishEveryEntries
}

func validCompression(compression int) bool {
	switch compression {
	case CompressionNone, CompressionZSTD, CompressionXZ, CompressionLZ4:
		return true
	default:
		return false
	}
}

// String returns the canonical 32-character lowercase hexadecimal UUID form
// used by journal paths and headers.
func (id UUID) String() string {
	return hex.EncodeToString(id[:])
}

func mustRandomUUID() UUID {
	id, err := NewUUID()
	if err != nil {
		panic(err)
	}
	return id
}

func isZeroUUID(id UUID) bool {
	return id == UUID{}
}

func startTimeForTailMonotonic(now time.Time, tailUsec uint64) time.Time {
	const maxDurationUsec = uint64(1<<63-1) / uint64(time.Microsecond)
	if tailUsec > maxDurationUsec {
		tailUsec = maxDurationUsec
	}
	return now.Add(-time.Duration(tailUsec) * time.Microsecond)
}

func (w *Writer) initialize(opts Options) error {
	layout := initialWriterLayout(opts)

	fileSize, ok := roundUpToFileSizeIncrease(layout.appendOffset)
	if !ok {
		return fmt.Errorf("journal initial arena too large")
	}
	if opts.Compact && fileSize > journalCompactSizeMax {
		return fmt.Errorf("compact journal cannot exceed 4 GiB")
	}

	incFlags := initialIncompatibleFlags(opts)
	compatibleFlags, err := w.initialCompatibleFlags(opts)
	if err != nil {
		return err
	}

	w.header = newInitialHeader(opts, layout, fileSize, compatibleFlags, incFlags)
	w.appendOffset = layout.appendOffset
	w.nextSeqnum = opts.HeadSeqnum

	if err := w.mapArena(fileSize); err != nil {
		return err
	}
	arenaMapped := true
	defer func() {
		if arenaMapped {
			_ = w.closeArena()
		}
	}()
	if err := w.writeHeader(); err != nil {
		return err
	}
	if err := w.writeInitialHashTableObjects(layout); err != nil {
		return err
	}

	if w.seal != nil {
		if err := w.appendFirstTag(); err != nil {
			return err
		}
	}

	arenaMapped = false
	return nil
}

func initialIncompatibleFlags(opts Options) uint32 {
	flags := uint32(incompatibleKeyedHash)
	switch opts.Compression {
	case CompressionZSTD:
		flags |= incompatibleCompressedZSTD
	case CompressionXZ:
		flags |= incompatibleCompressedXZ
	case CompressionLZ4:
		flags |= incompatibleCompressedLZ4
	}
	if opts.Compact {
		flags |= incompatibleCompact
	}
	return flags
}

func (w *Writer) initialCompatibleFlags(opts Options) (uint32, error) {
	flags := uint32(compatibleTailEntryBootID)
	if opts.Seal == nil {
		return flags, nil
	}
	seal, err := newSealState(*opts.Seal)
	if err != nil {
		return 0, err
	}
	w.seal = seal
	return flags | compatibleSealed | compatibleSealedContinuous, nil
}

func (w *Writer) writeInitialHashTableObjects(layout initialLayout) error {
	if err := w.writeObjectHeader(layout.fieldObjectOffset, objectHeader{
		typ:  objectTypeFieldHashTable,
		size: objectHeaderSize + layout.fieldSize,
	}); err != nil {
		return err
	}
	return w.writeObjectHeader(layout.dataObjectOffset, objectHeader{
		typ:  objectTypeDataHashTable,
		size: objectHeaderSize + layout.dataSize,
	})
}

type initialLayout struct {
	fieldObjectOffset uint64
	dataSize          uint64
	fieldSize         uint64
	dataObjectOffset  uint64
	fieldOffset       uint64
	dataOffset        uint64
	appendOffset      uint64
}

func initialWriterLayout(opts Options) initialLayout {
	dataSize := uint64(opts.DataHashTableBuckets * hashItemSize)
	fieldSize := uint64(opts.FieldHashTableBuckets * hashItemSize)
	fieldObjectOffset := uint64(headerSize)
	dataObjectOffset := align8(fieldObjectOffset + objectHeaderSize + fieldSize)
	return initialLayout{
		fieldObjectOffset: fieldObjectOffset,
		dataSize:          dataSize,
		fieldSize:         fieldSize,
		dataObjectOffset:  dataObjectOffset,
		fieldOffset:       fieldObjectOffset + objectHeaderSize,
		dataOffset:        dataObjectOffset + objectHeaderSize,
		appendOffset:      align8(dataObjectOffset + objectHeaderSize + dataSize),
	}
}

func newInitialHeader(opts Options, layout initialLayout, fileSize uint64, compatibleFlags, incFlags uint32) journalHeader {
	return journalHeader{
		signature:            [8]byte{'L', 'P', 'K', 'S', 'H', 'H', 'R', 'H'},
		compatibleFlags:      compatibleFlags,
		incompatibleFlags:    incFlags,
		state:                stateOnline,
		fileID:               opts.FileID,
		machineID:            opts.MachineID,
		seqnumID:             opts.SeqnumID,
		headerSize:           headerSize,
		arenaSize:            fileSize - headerSize,
		dataHashTableOffset:  layout.dataOffset,
		dataHashTableSize:    layout.dataSize,
		fieldHashTableOffset: layout.fieldOffset,
		fieldHashTableSize:   layout.fieldSize,
		tailObjectOffset:     layout.dataObjectOffset,
		nObjects:             2,
	}
}

func (w *Writer) mapArena(size uint64) error {
	arena, err := newMappedArena(w.file, size)
	if err != nil {
		return err
	}
	w.arena = arena
	return nil
}

func (w *Writer) closeArena() error {
	if w.arena == nil {
		return nil
	}
	err := w.arena.close()
	w.arena = nil
	return err
}

func (w *Writer) syncArena() error {
	if w.arena != nil {
		return w.arena.sync()
	}
	return w.file.Sync()
}

func (w *Writer) postChange() error {
	w.postChangeFence.Add(1)
	size, ok := checkedAdd(w.header.headerSize, w.header.arenaSize)
	if !ok || size > uint64(int64(^uint64(0)>>1)) {
		return fmt.Errorf("%w: journal file too large", errInvalidJournal)
	}
	return w.file.Truncate(int64(size))
}

func (w *Writer) publishAfterEntry() error {
	switch w.livePublishEveryEntries {
	case 0:
		return nil
	case 1:
		return w.postChange()
	default:
		w.entriesSinceLivePublication++
		if w.entriesSinceLivePublication >= w.livePublishEveryEntries {
			w.entriesSinceLivePublication = 0
			return w.postChange()
		}
		return nil
	}
}

func (w *Writer) readAt(dst []byte, offset uint64) error {
	if w.arena != nil {
		return w.arena.readAt(dst, offset)
	}
	_, err := w.file.ReadAt(dst, int64(offset))
	return err
}

func (w *Writer) writeAt(offset uint64, src []byte) error {
	if w.arena != nil {
		return w.arena.writeAt(offset, src)
	}
	_, err := w.file.WriteAt(src, int64(offset))
	return err
}

func (w *Writer) writeHeader() error {
	buf := make([]byte, headerSize)
	putHeader(buf, w.header)
	return w.writeAt(0, buf)
}

func (w *Writer) publishObjectMetadata() error {
	if err := w.writeUint64At(96, w.header.arenaSize); err != nil {
		return err
	}
	if err := w.writeUint64At(136, w.header.tailObjectOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(144, w.header.nObjects); err != nil {
		return err
	}
	if err := w.writeUint64At(208, w.header.nData); err != nil {
		return err
	}
	if err := w.writeUint64At(216, w.header.nFields); err != nil {
		return err
	}
	if err := w.writeUint64At(232, w.header.nEntryArrays); err != nil {
		return err
	}
	if err := w.writeUint64At(240, w.header.dataHashChainDepth); err != nil {
		return err
	}
	return w.writeUint64At(248, w.header.fieldHashChainDepth)
}

func (w *Writer) publishEntryMetadata() error {
	if err := w.writeUUIDAt(56, w.header.tailEntryBootID); err != nil {
		return err
	}
	if err := w.writeUint64At(160, w.header.tailEntrySeqnum); err != nil {
		return err
	}
	if err := w.writeUint64At(168, w.header.headEntrySeqnum); err != nil {
		return err
	}
	if err := w.writeUint64At(176, w.header.entryArrayOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(184, w.header.headEntryRealtime); err != nil {
		return err
	}
	if err := w.writeUint64At(192, w.header.tailEntryRealtime); err != nil {
		return err
	}
	if err := w.writeUint64At(200, w.header.tailEntryMonotonic); err != nil {
		return err
	}
	if err := w.writeUint32At(256, w.header.tailEntryArrayOffset); err != nil {
		return err
	}
	if err := w.writeUint32At(260, w.header.tailEntryArrayNEntries); err != nil {
		return err
	}
	if err := w.writeUint64At(264, w.header.tailEntryOffset); err != nil {
		return err
	}
	return w.writeUint64At(152, w.header.nEntries)
}

func (w *Writer) writeObjectHeader(offset uint64, header objectHeader) error {
	buf := make([]byte, objectHeaderSize)
	putObjectHeader(buf, header)
	return w.writeAt(offset, buf)
}

func (w *Writer) writeObject(offset uint64, buf []byte) error {
	end, ok := checkedAdd(offset, uint64(len(buf)))
	if !ok {
		return fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	if err := w.ensureArenaSize(end); err != nil {
		return err
	}
	return w.writeAt(offset, buf)
}

func (w *Writer) newObjectBuffer(offset, size uint64) ([]byte, bool, error) {
	alignedSize := align8(size)
	end, ok := checkedAdd(offset, alignedSize)
	if !ok {
		return nil, false, fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	if err := w.ensureArenaSize(end); err != nil {
		return nil, false, err
	}
	if w.arena != nil {
		if data, ok, err := w.arena.directBytesAt(offset, alignedSize); err != nil || ok {
			return data, ok, err
		}
	}
	if alignedSize > uint64(int(^uint(0)>>1)) {
		return nil, false, fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	return make([]byte, int(alignedSize)), false, nil
}

func (w *Writer) commitObjectBuffer(offset uint64, buf []byte, direct bool) error {
	if direct {
		return nil
	}
	return w.writeAt(offset, buf)
}

func readObjectHeaderAt(f *os.File, offset uint64) (objectHeader, error) {
	buf := make([]byte, objectHeaderSize)
	if _, err := f.ReadAt(buf, int64(offset)); err != nil {
		return objectHeader{}, err
	}
	return parseObjectHeader(buf)
}

func (w *Writer) hash(payload []byte) uint64 {
	return sipHash24(w.header.fileID, payload)
}

func (w *Writer) objectAdded(offset, size uint64) error {
	if offset > ^uint64(0)-size {
		return fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	w.header.tailObjectOffset = offset
	w.appendOffset = align8(offset + size)
	w.header.nObjects++
	return w.ensureArenaSize(w.appendOffset)
}

func (w *Writer) ensureArenaSize(requiredSize uint64) error {
	oldSize := headerSize + w.header.arenaSize
	if requiredSize <= oldSize {
		return nil
	}
	newSize, ok := roundUpToFileSizeIncrease(requiredSize)
	if !ok {
		return fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	if w.compact && newSize > journalCompactSizeMax {
		return fmt.Errorf("%w: compact journal cannot exceed 4 GiB", errInvalidJournal)
	}
	if w.arena != nil {
		if err := w.arena.remap(newSize); err != nil {
			return err
		}
	} else if err := w.file.Truncate(int64(newSize)); err != nil {
		return err
	}
	w.header.arenaSize = newSize - headerSize
	return nil
}

func (w *Writer) entryAdded(entryOffset, entrySeqnum, realtime, monotonic uint64, bootID UUID) {
	w.header.nEntries++
	if w.header.headEntrySeqnum == 0 {
		w.header.headEntrySeqnum = entrySeqnum
	}
	if w.header.headEntryRealtime == 0 {
		w.header.headEntryRealtime = realtime
	}
	w.header.tailEntrySeqnum = entrySeqnum
	w.header.tailEntryRealtime = realtime
	w.header.tailEntryMonotonic = monotonic
	w.header.tailEntryBootID = bootID
	w.header.tailEntryOffset = entryOffset
	w.nextSeqnum = entrySeqnum + 1
}

func (w *Writer) addData(payload []byte) (uint64, uint64, error) {
	hash := w.hash(payload)
	if offset, ok, err := w.findData(hash, payload); err != nil || ok {
		return offset, hash, err
	}

	objectPayload, compressionFlag := w.compressedDataPayload(payload)
	offset, err := w.writeDataObject(hash, objectPayload, compressionFlag)
	if err != nil {
		return 0, 0, err
	}

	if err := w.appendHashItem(w.header.dataHashTableOffset, w.header.dataHashTableSize, objectTypeData, hash, offset); err != nil {
		return 0, 0, err
	}
	w.header.nData++

	if err := w.hmacPutObject(offset, objectTypeData); err != nil {
		return 0, 0, err
	}

	if err := w.linkDataToField(offset, payload); err != nil {
		return 0, 0, err
	}

	return offset, hash, nil
}

func (w *Writer) compressedDataPayload(payload []byte) ([]byte, uint8) {
	if len(payload) < w.compressThreshold {
		return payload, 0
	}
	if compressed, flag, ok := w.tryCompressDataPayload(payload); ok {
		return compressed, flag
	}
	return payload, 0
}

func (w *Writer) tryCompressDataPayload(payload []byte) ([]byte, uint8, bool) {
	switch w.compression {
	case CompressionZSTD:
		return tryZstdDataPayload(payload)
	case CompressionXZ:
		return tryXZDataPayload(payload)
	case CompressionLZ4:
		return tryLZ4DataPayload(payload)
	default:
		return nil, 0, false
	}
}

func tryZstdDataPayload(payload []byte) ([]byte, uint8, bool) {
	compressed, err := zstdCompress(payload)
	if err == nil && len(compressed) < len(payload) {
		return compressed, objectCompressedZSTD, true
	}
	return nil, 0, false
}

func tryXZDataPayload(payload []byte) ([]byte, uint8, bool) {
	if len(payload) < 80 {
		return nil, 0, false
	}
	compressed, err := xzCompress(payload)
	if err == nil && len(compressed) < len(payload) {
		return compressed, objectCompressedXZ, true
	}
	return nil, 0, false
}

func tryLZ4DataPayload(payload []byte) ([]byte, uint8, bool) {
	if len(payload) < 9 {
		return nil, 0, false
	}
	compressed := lz4Compress(payload)
	if len(compressed) < len(payload) {
		return compressed, objectCompressedLZ4, true
	}
	return nil, 0, false
}

func (w *Writer) writeDataObject(hash uint64, objectPayload []byte, compressionFlag uint8) (uint64, error) {
	offset := w.appendOffset
	payloadOffset := w.dataPayloadOffset()
	size := payloadOffset + uint64(len(objectPayload))
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, err
	}
	buf, direct, err := w.newObjectBuffer(offset, size)
	if err != nil {
		return 0, err
	}
	putDataHeader(buf[:dataObjectHeaderSize], dataHeader{
		object: objectHeader{typ: objectTypeData, flag: compressionFlag, size: size},
		hash:   hash,
	})
	copy(buf[payloadOffset:], objectPayload)
	if err := w.commitObjectBuffer(offset, buf, direct); err != nil {
		return 0, err
	}
	if err := w.objectAdded(offset, size); err != nil {
		return 0, err
	}
	return offset, nil
}

func (w *Writer) linkDataToField(offset uint64, payload []byte) error {
	eq := bytes.IndexByte(payload, '=')
	if eq <= 0 {
		return nil
	}
	fieldPayload := payload[:eq]
	fieldHash := w.hash(fieldPayload)
	fieldOffset, fieldHeadDataOffset, err := w.addField(fieldHash, fieldPayload)
	if err != nil {
		return err
	}
	if err := w.writeUint64At(offset+32, fieldHeadDataOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(fieldOffset+32, offset); err != nil {
		return err
	}
	w.fieldCache.insert(fieldHash, fieldPayload, fieldOffset, offset)
	return nil
}

func (w *Writer) addField(hash uint64, payload []byte) (uint64, uint64, error) {
	if offset, headDataOffset, ok := w.fieldCache.get(hash, payload); ok {
		return offset, headDataOffset, nil
	}
	offset, ok, err := w.findField(hash, payload)
	if err != nil {
		return 0, 0, err
	}
	if ok {
		field, err := w.readFieldHeader(offset)
		if err != nil {
			return 0, 0, err
		}
		w.fieldCache.insert(hash, payload, offset, field.headDataOffset)
		return offset, field.headDataOffset, nil
	}

	offset = w.appendOffset
	size := uint64(fieldObjectHeaderSize + len(payload))
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, 0, err
	}
	buf, direct, err := w.newObjectBuffer(offset, size)
	if err != nil {
		return 0, 0, err
	}
	putFieldHeader(buf[:fieldObjectHeaderSize], fieldHeader{
		object: objectHeader{typ: objectTypeField, size: size},
		hash:   hash,
	})
	copy(buf[fieldObjectHeaderSize:], payload)
	if err := w.commitObjectBuffer(offset, buf, direct); err != nil {
		return 0, 0, err
	}
	if err := w.objectAdded(offset, size); err != nil {
		return 0, 0, err
	}

	if err := w.appendHashItem(w.header.fieldHashTableOffset, w.header.fieldHashTableSize, objectTypeField, hash, offset); err != nil {
		return 0, 0, err
	}
	w.header.nFields++

	if err := w.hmacPutObject(offset, objectTypeField); err != nil {
		return 0, 0, err
	}

	w.fieldCache.insert(hash, payload, offset, 0)
	return offset, 0, nil
}

func (w *Writer) appendHashItem(tableOffset, tableSize uint64, typ uint8, hash, objectOffset uint64) error {
	bucketOffset := tableOffset + (hash%(tableSize/hashItemSize))*hashItemSize
	item, err := w.readHashItem(bucketOffset)
	if err != nil {
		return err
	}
	if item.tail != 0 {
		if err := w.writeUint64At(item.tail+24, objectOffset); err != nil {
			return err
		}
	} else {
		item.head = objectOffset
	}
	item.tail = objectOffset

	// Sanity check the previous tail type if this bucket was non-empty.
	if item.head != objectOffset {
		oh, err := w.readObjectHeader(item.head)
		if err != nil {
			return err
		}
		if oh.typ != typ {
			return errInvalidJournal
		}
	}
	if err := w.writeHashItem(bucketOffset, item); err != nil {
		return err
	}
	if item.head != objectOffset {
		return w.updateHashChainDepth(typ, item.head)
	}
	return nil
}

func (w *Writer) updateHashChainDepth(typ uint8, head uint64) error {
	var depth uint64
	for offset := head; offset != 0; {
		var next uint64
		switch typ {
		case objectTypeData:
			header, err := w.readDataHeader(offset)
			if err != nil {
				return err
			}
			next = header.nextHashOffset
		case objectTypeField:
			header, err := w.readFieldHeader(offset)
			if err != nil {
				return err
			}
			next = header.nextHashOffset
		default:
			return errInvalidJournal
		}
		if next != 0 {
			depth++
		}
		offset = next
	}
	switch typ {
	case objectTypeData:
		if depth > w.header.dataHashChainDepth {
			w.header.dataHashChainDepth = depth
		}
	case objectTypeField:
		if depth > w.header.fieldHashChainDepth {
			w.header.fieldHashChainDepth = depth
		}
	}
	return nil
}

func (w *Writer) findData(hash uint64, payload []byte) (uint64, bool, error) {
	bucketOffset := w.header.dataHashTableOffset + (hash%(w.header.dataHashTableSize/hashItemSize))*hashItemSize
	item, err := w.readHashItem(bucketOffset)
	if err != nil {
		return 0, false, err
	}

	depth := uint64(0)
	for offset := item.head; offset != 0; {
		header, err := w.readDataHeader(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash {
			stored, err := w.readDataPayload(header, offset)
			if err != nil {
				return 0, false, err
			}
			if bytes.Equal(stored, payload) {
				return offset, true, nil
			}
		}
		if header.nextHashOffset != 0 {
			depth++
			if depth > w.header.dataHashChainDepth {
				w.header.dataHashChainDepth = depth
			}
		}
		offset = header.nextHashOffset
	}
	return 0, false, nil
}

func (w *Writer) findField(hash uint64, payload []byte) (uint64, bool, error) {
	bucketOffset := w.header.fieldHashTableOffset + (hash%(w.header.fieldHashTableSize/hashItemSize))*hashItemSize
	item, err := w.readHashItem(bucketOffset)
	if err != nil {
		return 0, false, err
	}

	depth := uint64(0)
	for offset := item.head; offset != 0; {
		header, stored, err := w.readFieldObject(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash && bytes.Equal(stored, payload) {
			return offset, true, nil
		}
		if header.nextHashOffset != 0 {
			depth++
			if depth > w.header.fieldHashChainDepth {
				w.header.fieldHashChainDepth = depth
			}
		}
		offset = header.nextHashOffset
	}
	return 0, false, nil
}

func (w *Writer) readHashItem(offset uint64) (hashItem, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, hashItemSize); err != nil || ok {
			if err != nil {
				return hashItem{}, err
			}
			return parseHashItem(src), nil
		}
	}
	var buf [hashItemSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return hashItem{}, err
	}
	return parseHashItem(buf[:]), nil
}

func (w *Writer) writeHashItem(offset uint64, item hashItem) error {
	var buf [hashItemSize]byte
	putHashItem(buf[:], item)
	return w.writeAt(offset, buf[:])
}

func (w *Writer) readDataObject(offset uint64) (dataHeader, []byte, error) {
	header, err := w.readDataHeader(offset)
	if err != nil {
		return dataHeader{}, nil, err
	}
	payload, err := w.readDataPayload(header, offset)
	if err != nil {
		return dataHeader{}, nil, err
	}
	return header, payload, nil
}

func (w *Writer) readDataPayload(header dataHeader, offset uint64) ([]byte, error) {
	payloadOffset := w.dataPayloadOffset()
	if header.object.typ != objectTypeData || header.object.size < payloadOffset {
		return nil, errInvalidJournal
	}
	payloadSize := header.object.size - payloadOffset
	var payload []byte
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset+payloadOffset, payloadSize); err != nil || ok {
			if err != nil {
				return nil, err
			}
			payload = src
		}
	}
	if payload == nil {
		if payloadSize > uint64(int(^uint(0)>>1)) {
			return nil, errInvalidJournal
		}
		payload = make([]byte, int(payloadSize))
		if err := w.readAt(payload, offset+payloadOffset); err != nil {
			return nil, err
		}
	}
	return decompressDataPayload(header.object.flag, payload)
}

func (w *Writer) readFieldObject(offset uint64) (fieldHeader, []byte, error) {
	header, err := w.readFieldHeader(offset)
	if err != nil {
		return fieldHeader{}, nil, err
	}
	if header.object.typ != objectTypeField || header.object.size < fieldObjectHeaderSize {
		return fieldHeader{}, nil, errInvalidJournal
	}
	payload := make([]byte, header.object.size-fieldObjectHeaderSize)
	if err := w.readAt(payload, offset+fieldObjectHeaderSize); err != nil {
		return fieldHeader{}, nil, err
	}
	return header, payload, nil
}

func (w *Writer) readObjectHeader(offset uint64) (objectHeader, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, objectHeaderSize); err != nil || ok {
			if err != nil {
				return objectHeader{}, err
			}
			return parseObjectHeader(src)
		}
	}
	var buf [objectHeaderSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return objectHeader{}, err
	}
	return parseObjectHeader(buf[:])
}

func (w *Writer) readDataHeader(offset uint64) (dataHeader, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, dataObjectHeaderSize); err != nil || ok {
			if err != nil {
				return dataHeader{}, err
			}
			return parseDataHeader(src)
		}
	}
	var buf [dataObjectHeaderSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return dataHeader{}, err
	}
	return parseDataHeader(buf[:])
}

func (w *Writer) readFieldHeader(offset uint64) (fieldHeader, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, fieldObjectHeaderSize); err != nil || ok {
			if err != nil {
				return fieldHeader{}, err
			}
			return parseFieldHeader(src)
		}
	}
	var buf [fieldObjectHeaderSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return fieldHeader{}, err
	}
	return parseFieldHeader(buf[:])
}

func (w *Writer) writeUint64At(offset, value uint64) error {
	if w.arena != nil {
		if dst, ok, err := w.arena.directBytesAt(offset, 8); err != nil || ok {
			if err != nil {
				return err
			}
			binary.LittleEndian.PutUint64(dst, value)
			return nil
		}
	}
	var buf [8]byte
	binary.LittleEndian.PutUint64(buf[:], value)
	return w.writeAt(offset, buf[:])
}

func (w *Writer) writeUint32At(offset uint64, value uint32) error {
	if w.arena != nil {
		if dst, ok, err := w.arena.directBytesAt(offset, 4); err != nil || ok {
			if err != nil {
				return err
			}
			binary.LittleEndian.PutUint32(dst, value)
			return nil
		}
	}
	var buf [4]byte
	binary.LittleEndian.PutUint32(buf[:], value)
	return w.writeAt(offset, buf[:])
}

func (w *Writer) writeUUIDAt(offset uint64, value UUID) error {
	return w.writeAt(offset, value[:])
}

func nextEntryArrayCapacity(index, previousCapacity uint64) uint64 {
	capacity := previousCapacity
	if index > capacity {
		capacity = (index + 1) * 2
	} else {
		capacity *= 2
	}
	if capacity < 4 {
		capacity = 4
	}
	return capacity
}

func (w *Writer) appendToEntryArray(entryOffset uint64) error {
	if w.header.entryArrayOffset == 0 {
		return w.initEntryArray(entryOffset)
	}

	tailOffset, err := w.entryArrayTailOffset()
	if err != nil {
		return err
	}

	_, cap, err := w.readOffsetArrayHeader(tailOffset)
	if err != nil {
		return err
	}
	tailEntries, err := w.entryArrayTailEntries(tailOffset)
	if err != nil {
		return err
	}
	if tailEntries < cap {
		return w.appendToExistingEntryArrayTail(tailOffset, tailEntries, entryOffset)
	}

	newOffset, err := w.allocateOffsetArray(nextEntryArrayCapacity(w.header.nEntries, cap))
	if err != nil {
		return err
	}
	if err := w.writeUint64At(tailOffset+16, newOffset); err != nil {
		return err
	}
	if err := w.writeArrayItem(newOffset, 0, entryOffset); err != nil {
		return err
	}
	w.header.tailEntryArrayOffset = uint32(newOffset)
	w.header.tailEntryArrayNEntries = 1
	return nil
}

func (w *Writer) initEntryArray(entryOffset uint64) error {
	arrayOffset, err := w.allocateOffsetArray(4)
	if err != nil {
		return err
	}
	w.header.entryArrayOffset = arrayOffset
	w.header.tailEntryArrayOffset = uint32(arrayOffset)
	w.header.tailEntryArrayNEntries = 1
	return w.writeArrayItem(arrayOffset, 0, entryOffset)
}

func (w *Writer) entryArrayTailOffset() (uint64, error) {
	tailOffset := uint64(w.header.tailEntryArrayOffset)
	if tailOffset != 0 {
		return tailOffset, nil
	}
	tailOffset = w.header.entryArrayOffset
	for remaining := w.header.nEntries; ; {
		header, cap, err := w.readOffsetArrayHeader(tailOffset)
		if err != nil {
			return 0, err
		}
		if remaining < cap || header.nextArrayOffset == 0 {
			return tailOffset, nil
		}
		remaining -= cap
		tailOffset = header.nextArrayOffset
	}
}

func (w *Writer) entryArrayTailEntries(tailOffset uint64) (uint64, error) {
	tailEntries := uint64(w.header.tailEntryArrayNEntries)
	if tailEntries != 0 {
		return tailEntries, nil
	}
	tailEntries = w.header.nEntries
	for offset := w.header.entryArrayOffset; offset != 0 && offset != tailOffset; {
		h, c, err := w.readOffsetArrayHeader(offset)
		if err != nil {
			return 0, err
		}
		tailEntries -= c
		offset = h.nextArrayOffset
	}
	return tailEntries, nil
}

func (w *Writer) appendToExistingEntryArrayTail(tailOffset, tailEntries, entryOffset uint64) error {
	if err := w.writeArrayItem(tailOffset, tailEntries, entryOffset); err != nil {
		return err
	}
	w.header.tailEntryArrayOffset = uint32(tailOffset)
	w.header.tailEntryArrayNEntries = uint32(tailEntries + 1)
	return nil
}

func (w *Writer) allocateOffsetArray(capacity uint64) (uint64, error) {
	offset := w.appendOffset
	size := uint64(offsetArrayObjectHeaderSize) + capacity*w.offsetArrayItemSize()
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, err
	}
	buf, direct, err := w.newObjectBuffer(offset, size)
	if err != nil {
		return 0, err
	}
	putOffsetArrayHeader(buf[:offsetArrayObjectHeaderSize], offsetArrayHeader{
		object: objectHeader{typ: objectTypeEntryArray, size: size},
	})
	if err := w.commitObjectBuffer(offset, buf, direct); err != nil {
		return 0, err
	}
	if err := w.objectAdded(offset, size); err != nil {
		return 0, err
	}
	w.header.nEntryArrays++
	if err := w.publishObjectMetadata(); err != nil {
		return 0, err
	}
	if err := w.hmacPutObject(offset, objectTypeEntryArray); err != nil {
		return 0, err
	}
	return offset, nil
}

func (w *Writer) readOffsetArrayHeader(offset uint64) (offsetArrayHeader, uint64, error) {
	var src []byte
	if w.arena != nil {
		if data, ok, err := w.arena.directBytesAt(offset, offsetArrayObjectHeaderSize); err != nil || ok {
			if err != nil {
				return offsetArrayHeader{}, 0, err
			}
			src = data
		}
	}
	var buf [offsetArrayObjectHeaderSize]byte
	if src == nil {
		if err := w.readAt(buf[:], offset); err != nil {
			return offsetArrayHeader{}, 0, err
		}
		src = buf[:]
	}
	header, err := parseOffsetArrayHeader(src)
	if err != nil {
		return offsetArrayHeader{}, 0, err
	}
	if header.object.typ != objectTypeEntryArray || header.object.size < offsetArrayObjectHeaderSize {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	itemSize := w.offsetArrayItemSize()
	if (header.object.size-offsetArrayObjectHeaderSize)%itemSize != 0 {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	return header, (header.object.size - offsetArrayObjectHeaderSize) / itemSize, nil
}

func (w *Writer) writeArrayItem(arrayOffset, index, entryOffset uint64) error {
	itemOffset := arrayOffset + offsetArrayObjectHeaderSize + index*w.offsetArrayItemSize()
	if w.compact {
		if entryOffset > journalCompactSizeMax {
			return fmt.Errorf("%w: compact entry offset exceeds 32-bit range", errInvalidJournal)
		}
		return w.writeUint32At(itemOffset, uint32(entryOffset))
	}
	return w.writeUint64At(itemOffset, entryOffset)
}

func (w *Writer) linkDataToEntry(dataOffset, entryOffset uint64) error {
	header, err := w.readDataHeader(dataOffset)
	if err != nil {
		return err
	}
	switch header.nEntries {
	case 0:
		return w.linkFirstEntryToData(dataOffset, entryOffset)
	case 1:
		return w.linkSecondEntryToData(dataOffset, entryOffset)
	default:
		return w.linkLaterEntryToData(dataOffset, entryOffset, header)
	}
}

func (w *Writer) linkFirstEntryToData(dataOffset, entryOffset uint64) error {
	if err := w.writeUint64At(dataOffset+40, entryOffset); err != nil {
		return err
	}
	return w.writeUint64At(dataOffset+56, 1)
}

func (w *Writer) linkSecondEntryToData(dataOffset, entryOffset uint64) error {
	arrayOffset, err := w.allocateOffsetArray(4)
	if err != nil {
		return err
	}
	if err := w.writeArrayItem(arrayOffset, 0, entryOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(dataOffset+48, arrayOffset); err != nil {
		return err
	}
	if w.compact {
		if err := w.writeCompactDataTail(dataOffset, arrayOffset, 1); err != nil {
			return err
		}
	}
	return w.writeUint64At(dataOffset+56, 2)
}

func (w *Writer) linkLaterEntryToData(dataOffset, entryOffset uint64, header dataHeader) error {
	if header.entryArrayOffset == 0 {
		return errInvalidJournal
	}
	currentCount := header.nEntries - 1
	tailOffset, tailEntries, err := w.appendToDataEntryArrayTail(dataOffset, header.entryArrayOffset, currentCount, entryOffset)
	if err != nil {
		return err
	}
	if w.compact {
		if err := w.writeCompactDataTail(dataOffset, tailOffset, tailEntries); err != nil {
			return err
		}
	}
	return w.writeUint64At(dataOffset+56, header.nEntries+1)
}

func (w *Writer) appendToDataEntryArrayTail(dataOffset, entryArrayOffset, currentCount, entryOffset uint64) (uint64, uint64, error) {
	tailOffset, tailEntries, ok, err := w.appendToCompactDataEntryArrayTail(dataOffset, currentCount, entryOffset)
	if err != nil || ok {
		return tailOffset, tailEntries, err
	}
	return w.appendToDataEntryArray(entryArrayOffset, currentCount, entryOffset)
}

func (w *Writer) appendToCompactDataEntryArrayTail(dataOffset, currentCount, entryOffset uint64) (uint64, uint64, bool, error) {
	if !w.compact {
		return 0, 0, false, nil
	}
	tailOffset, tailEntries, ok, err := w.readCompactDataTail(dataOffset)
	if err != nil {
		return 0, 0, false, err
	}
	if !ok || tailEntries == 0 || tailEntries > currentCount {
		return 0, 0, false, nil
	}
	header, cap, err := w.readOffsetArrayHeader(tailOffset)
	if err != nil || header.nextArrayOffset != 0 || tailEntries > cap {
		return 0, 0, false, nil
	}
	if tailEntries < cap {
		return w.appendToExistingCompactDataTail(tailOffset, tailEntries, entryOffset)
	}
	return w.appendNewCompactDataTail(tailOffset, currentCount, cap, entryOffset)
}

func (w *Writer) appendToExistingCompactDataTail(tailOffset, tailEntries, entryOffset uint64) (uint64, uint64, bool, error) {
	if err := w.writeArrayItem(tailOffset, tailEntries, entryOffset); err != nil {
		return 0, 0, false, err
	}
	return tailOffset, tailEntries + 1, true, nil
}

func (w *Writer) appendNewCompactDataTail(tailOffset, currentCount, cap, entryOffset uint64) (uint64, uint64, bool, error) {
	newOffset, err := w.allocateOffsetArray(nextEntryArrayCapacity(currentCount, cap))
	if err != nil {
		return 0, 0, false, err
	}
	if err := w.writeUint64At(tailOffset+16, newOffset); err != nil {
		return 0, 0, false, err
	}
	if err := w.writeArrayItem(newOffset, 0, entryOffset); err != nil {
		return 0, 0, false, err
	}
	return newOffset, 1, true, nil
}

func (w *Writer) readCompactDataTail(dataOffset uint64) (uint64, uint64, bool, error) {
	if !w.compact {
		return 0, 0, false, nil
	}
	tailFieldOffset := dataOffset + compactDataTailOffsetOffset
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(tailFieldOffset, 8); err != nil || ok {
			if err != nil {
				return 0, 0, false, err
			}
			tailOffset := uint64(binary.LittleEndian.Uint32(src[0:4]))
			tailEntries := uint64(binary.LittleEndian.Uint32(src[4:8]))
			return tailOffset, tailEntries, tailOffset != 0 && tailEntries != 0, nil
		}
	}
	var buf [8]byte
	if err := w.readAt(buf[:], tailFieldOffset); err != nil {
		return 0, 0, false, err
	}
	tailOffset := uint64(binary.LittleEndian.Uint32(buf[0:4]))
	tailEntries := uint64(binary.LittleEndian.Uint32(buf[4:8]))
	return tailOffset, tailEntries, tailOffset != 0 && tailEntries != 0, nil
}

func (w *Writer) writeCompactDataTail(dataOffset, tailOffset, tailEntries uint64) error {
	if tailOffset > journalCompactSizeMax || tailEntries > uint64(^uint32(0)) {
		return fmt.Errorf("%w: compact DATA tail exceeds 32-bit range", errInvalidJournal)
	}
	if err := w.writeUint32At(dataOffset+compactDataTailOffsetOffset, uint32(tailOffset)); err != nil {
		return err
	}
	return w.writeUint32At(dataOffset+compactDataTailEntriesOffset, uint32(tailEntries))
}

func (w *Writer) appendToDataEntryArray(arrayOffset, currentCount, entryOffset uint64) (uint64, uint64, error) {
	remaining := currentCount
	offset := arrayOffset
	for {
		header, cap, err := w.readOffsetArrayHeader(offset)
		if err != nil {
			return 0, 0, err
		}
		if remaining < cap {
			if err := w.writeArrayItem(offset, remaining, entryOffset); err != nil {
				return 0, 0, err
			}
			return offset, remaining + 1, nil
		}
		remaining -= cap
		if header.nextArrayOffset == 0 {
			newOffset, err := w.allocateOffsetArray(nextEntryArrayCapacity(currentCount, cap))
			if err != nil {
				return 0, 0, err
			}
			if err := w.writeUint64At(offset+16, newOffset); err != nil {
				return 0, 0, err
			}
			if err := w.writeArrayItem(newOffset, 0, entryOffset); err != nil {
				return 0, 0, err
			}
			return newOffset, 1, nil
		}
		offset = header.nextArrayOffset
	}
}

func (w *Writer) entryItemSize() uint64 {
	if w.compact {
		return compactEntryItemSize
	}
	return regularEntryItemSize
}

func (w *Writer) offsetArrayItemSize() uint64 {
	if w.compact {
		return compactOffsetArrayItemSize
	}
	return regularOffsetArrayItemSize
}

func (w *Writer) dataPayloadOffset() uint64 {
	if w.compact {
		return compactDataObjectHeaderSize
	}
	return dataObjectHeaderSize
}

func (w *Writer) ensureCompactObjectFits(offset, size uint64) error {
	if !w.compact {
		return nil
	}
	if offset > journalCompactSizeMax || align8(offset+size) > journalCompactSizeMax {
		return fmt.Errorf("%w: compact journal cannot exceed 4 GiB", errInvalidJournal)
	}
	return nil
}

func dedupeEntryItems(items []entryItem) []entryItem {
	if len(items) < 2 {
		return items
	}
	out := items[:1]
	for _, item := range items[1:] {
		if item.offset != out[len(out)-1].offset {
			out = append(out, item)
		}
	}
	return out
}

func zstdCompress(payload []byte) ([]byte, error) {
	var buf bytes.Buffer
	enc, err := zstd.NewWriter(&buf, zstd.WithEncoderLevel(zstd.SpeedFastest))
	if err != nil {
		return nil, err
	}
	if _, err := enc.Write(payload); err != nil {
		return nil, err
	}
	if err := enc.Close(); err != nil {
		return nil, err
	}
	return zstdFrameWithContentSize(buf.Bytes(), len(payload)), nil
}

func zstdFrameWithContentSize(frame []byte, contentSize int) []byte {
	const (
		zstdMagic           = "\x28\xb5\x2f\xfd"
		singleSegmentFlag   = byte(1 << 5)
		contentChecksumFlag = byte(1 << 2)
	)
	if len(frame) < 6 || string(frame[:4]) != zstdMagic {
		return frame
	}
	descriptor := frame[4]
	dictionaryIDFlag := descriptor & 0x03
	frameContentSizeFlag := descriptor >> 6
	if dictionaryIDFlag != 0 || frameContentSizeFlag != 0 || descriptor&singleSegmentFlag != 0 {
		return frame
	}

	var sizeFlag byte
	var sizeBytes []byte
	switch {
	case contentSize <= 255:
		sizeFlag = 0
		sizeBytes = []byte{byte(contentSize)}
	case contentSize <= 65791:
		sizeFlag = 1
		encoded := uint16(contentSize - 256)
		sizeBytes = []byte{byte(encoded), byte(encoded >> 8)}
	case uint64(contentSize) <= uint64(^uint32(0)):
		sizeFlag = 2
		encoded := uint32(contentSize)
		sizeBytes = []byte{byte(encoded), byte(encoded >> 8), byte(encoded >> 16), byte(encoded >> 24)}
	default:
		sizeFlag = 3
		encoded := uint64(contentSize)
		sizeBytes = []byte{
			byte(encoded),
			byte(encoded >> 8),
			byte(encoded >> 16),
			byte(encoded >> 24),
			byte(encoded >> 32),
			byte(encoded >> 40),
			byte(encoded >> 48),
			byte(encoded >> 56),
		}
	}

	patched := make([]byte, 0, len(frame)+len(sizeBytes)-1)
	patched = append(patched, frame[:4]...)
	patched = append(patched, sizeFlag<<6|singleSegmentFlag|descriptor&contentChecksumFlag)
	patched = append(patched, sizeBytes...)
	patched = append(patched, frame[6:]...)
	return patched
}

func zstdDecompress(payload []byte) ([]byte, error) {
	decoder, err := zstd.NewReader(nil, zstd.WithDecoderMaxMemory(uint64(maxUncompressedDataObjectSize)))
	if err != nil {
		return nil, err
	}
	defer decoder.Close()
	return decoder.DecodeAll(payload, nil)
}

func xzCompress(payload []byte) ([]byte, error) {
	cfg := xz.WriterConfig{NoCheckSum: true}
	var buf bytes.Buffer
	w, err := cfg.NewWriter(&buf)
	if err != nil {
		return nil, err
	}
	if _, err := w.Write(payload); err != nil {
		return nil, err
	}
	if err := w.Close(); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func xzDecompress(payload []byte) ([]byte, error) {
	r, err := xz.NewReader(bytes.NewReader(payload))
	if err != nil {
		return nil, err
	}
	return readAllLimited(r, maxUncompressedDataObjectSize)
}

func lz4Compress(payload []byte) []byte {
	maxCompressedSize := lz4.CompressBlockBound(len(payload))
	compressed := make([]byte, maxCompressedSize)
	n, err := lz4.CompressBlock(payload, compressed, nil)
	if err != nil || n == 0 {
		return payload
	}
	compressed = compressed[:n]
	out := make([]byte, 8+len(compressed))
	binary.LittleEndian.PutUint64(out[:8], uint64(len(payload)))
	copy(out[8:], compressed)
	return out
}

func lz4Decompress(payload []byte) ([]byte, error) {
	if len(payload) < 8 {
		return nil, errors.New("lz4 compressed payload too short")
	}
	uncompressedSize := binary.LittleEndian.Uint64(payload[:8])
	if uncompressedSize > maxUncompressedDataObjectSize {
		return nil, errors.New("lz4 decompressed payload too large")
	}
	compressedData := payload[8:]
	decoded := make([]byte, uncompressedSize)
	n, err := lz4.UncompressBlock(compressedData, decoded)
	if err != nil {
		return nil, err
	}
	if uint64(n) != uncompressedSize {
		return nil, errors.New("lz4 decompressed size mismatch")
	}
	return decoded, nil
}

func readAllLimited(r io.Reader, maxBytes int) ([]byte, error) {
	limited := io.LimitReader(r, int64(maxBytes)+1)
	decoded, err := io.ReadAll(limited)
	if err != nil {
		return nil, err
	}
	if len(decoded) > maxBytes {
		return nil, errors.New("decompressed payload too large")
	}
	return decoded, nil
}
