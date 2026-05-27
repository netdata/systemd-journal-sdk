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
	"syscall"
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
	lock              *writerLock
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
	dataCache         recentDataCache
	payloadScratch    []byte
	entryItemsScratch []entryItem
	// Full memory-ordering point before same-size ftruncate wakes stock follow readers.
	postChangeFence atomic.Uint64
}

// Create creates or truncates a journal file after acquiring the writer lock.
func Create(path string, opts Options) (*Writer, error) {
	opts = normalizeOptions(opts)
	if !validCompression(opts.Compression) {
		return nil, fmt.Errorf("unsupported journal compression: %d", opts.Compression)
	}

	lock, err := acquireWriterLock(path)
	if err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o640)
	if err != nil {
		_ = lock.release()
		return nil, err
	}
	if err := lockFile(f); err != nil {
		_ = f.Close()
		_ = lock.release()
		return nil, err
	}
	if err := f.Truncate(0); err != nil {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, err
	}

	w := &Writer{
		file: f, path: path, lock: lock, bootID: opts.BootID, started: time.Now(),
		compression: opts.Compression, compressThreshold: opts.CompressThresholdBytes, compact: opts.Compact,
	}
	if err := w.initialize(opts); err != nil {
		_ = w.closeArena()
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, err
	}
	return w, nil
}

// Open opens a journal file created by this package for appending.
func Open(path string) (*Writer, error) {
	lock, err := acquireWriterLock(path)
	if err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		_ = lock.release()
		return nil, err
	}
	if err := lockFile(f); err != nil {
		_ = f.Close()
		_ = lock.release()
		return nil, err
	}

	buf := make([]byte, headerSize)
	if _, err := f.ReadAt(buf, 0); err != nil {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, err
	}
	header, err := parseHeader(buf)
	if err != nil {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, err
	}
	const supportedWriterIncompatible = incompatibleKeyedHash | incompatibleCompressedZSTD | incompatibleCompressedXZ | incompatibleCompressedLZ4 | incompatibleCompact
	if header.incompatibleFlags&^supportedWriterIncompatible != 0 {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, errUnsupportedJournal
	}
	if header.dataHashTableOffset == 0 || header.fieldHashTableOffset == 0 || header.tailObjectOffset == 0 {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, errInvalidJournal
	}

	tail, err := readObjectHeaderAt(f, header.tailObjectOffset)
	if err != nil {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, err
	}

	compression := CompressionNone
	if header.incompatibleFlags&incompatibleCompressedZSTD != 0 {
		compression = CompressionZSTD
	} else if header.incompatibleFlags&incompatibleCompressedXZ != 0 {
		compression = CompressionXZ
	} else if header.incompatibleFlags&incompatibleCompressedLZ4 != 0 {
		compression = CompressionLZ4
	}
	header.state = stateOnline
	now := time.Now()
	w := &Writer{
		file:              f,
		path:              path,
		lock:              lock,
		header:            header,
		appendOffset:      align8(header.tailObjectOffset + tail.size),
		nextSeqnum:        header.tailEntrySeqnum + 1,
		bootID:            header.tailEntryBootID,
		started:           startTimeForTailMonotonic(now, header.tailEntryMonotonic),
		compression:       compression,
		compressThreshold: defaultCompressThreshold,
		compact:           header.isCompact(),
	}
	fileSize, ok := checkedAdd(header.headerSize, header.arenaSize)
	if !ok {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, errInvalidJournal
	}
	if err := w.mapArena(fileSize); err != nil {
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, err
	}
	if isZeroUUID(w.bootID) {
		if bootID, err := readUUIDFile("/proc/sys/kernel/random/boot_id"); err == nil {
			w.bootID = bootID
		} else {
			w.bootID = header.fileID
		}
	}
	if err := w.writeHeader(); err != nil {
		_ = w.closeArena()
		_ = unlockAndClose(f)
		_ = lock.release()
		return nil, err
	}
	return w, nil
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
	if len(fields) == 0 {
		return errEntryEmpty
	}

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

	if err := w.maybeAppendTag(opts.RealtimeUsec); err != nil {
		return err
	}

	for _, field := range fields {
		if err := validateFieldName(field.Name); err != nil {
			return err
		}
	}

	items := w.entryItemsScratch[:0]
	if cap(items) < len(fields) {
		items = make([]entryItem, 0, len(fields))
	}
	defer func() {
		w.entryItemsScratch = items[:0]
		if cap(w.payloadScratch) > payloadScratchMaxRetain {
			w.payloadScratch = nil
		}
	}()
	xorHash := uint64(0)
	for _, field := range fields {
		w.payloadScratch = append(w.payloadScratch[:0], field.Name...)
		w.payloadScratch = append(w.payloadScratch, '=')
		w.payloadScratch = append(w.payloadScratch, field.Value...)
		payload := w.payloadScratch
		offset, hash, err := w.addData(payload)
		if err != nil {
			return err
		}
		items = append(items, entryItem{offset: offset, hash: hash})
		xorHash ^= jenkinsHash64(payload)
	}

	sort.Slice(items, func(i, j int) bool { return items[i].offset < items[j].offset })
	items = dedupeEntryItems(items)

	entryOffset := w.appendOffset
	itemSize := w.entryItemSize()
	entrySize := uint64(entryObjectHeaderSize + len(items)*int(itemSize))
	if err := w.ensureCompactObjectFits(entryOffset, entrySize); err != nil {
		return err
	}
	buf := make([]byte, align8(entrySize))
	putEntryHeader(buf[:entryObjectHeaderSize], entryHeader{
		object: objectHeader{typ: objectTypeEntry, size: entrySize},
		seqnum: w.nextSeqnum, realtime: opts.RealtimeUsec,
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
	if err := w.writeObject(entryOffset, buf); err != nil {
		return err
	}
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
	w.entryAdded(entryOffset, opts.RealtimeUsec, opts.MonotonicUsec, opts.BootID)
	if err := w.publishEntryMetadata(); err != nil {
		return err
	}
	return w.postChange()
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

// CloseOffline marks the journal offline, syncs it, releases the writer lock,
// and closes it.
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
	err4 := syscall.Flock(int(w.file.Fd()), syscall.LOCK_UN)
	err5 := w.file.Close()
	err6 := w.lock.release()
	w.lock = nil
	w.closed = true
	return errors.Join(err1, err2, err3, err4, err5, err6)
}

// CurrentSize returns the current committed journal file size in bytes.
func (w *Writer) CurrentSize() uint64 {
	return w.appendOffset
}

// ArchiveTo marks the journal archived, renames it, syncs the parent
// directory, releases the writer lock, and closes it.
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
	unlockErr := syscall.Flock(int(w.file.Fd()), syscall.LOCK_UN)
	closeErr := w.file.Close()
	lockErr := w.lock.release()
	w.lock = nil
	w.closed = true
	if err := errors.Join(dirErr, arenaErr, unlockErr, closeErr, lockErr); err != nil {
		return err
	}
	return nil
}

type entryItem struct {
	offset uint64
	hash   uint64
}

const (
	fieldCacheSlots              = 1024
	fieldCacheMaxPayloadLen      = 128
	recentDataCacheSlots         = 65536
	recentDataCacheMaxPayloadLen = 256
	payloadScratchMaxRetain      = 1 << 20
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

type recentDataCacheEntry struct {
	payload []byte
	item    entryItem
}

type recentDataCache struct {
	entries []recentDataCacheEntry
}

func (c *recentDataCache) get(hash uint64, payload []byte) (entryItem, bool) {
	if len(payload) > recentDataCacheMaxPayloadLen || len(c.entries) == 0 {
		return entryItem{}, false
	}
	entry := c.entries[int(hash)&(recentDataCacheSlots-1)]
	if entry.item.offset == 0 || !bytes.Equal(entry.payload, payload) {
		return entryItem{}, false
	}
	return entry.item, true
}

func (c *recentDataCache) insert(hash uint64, payload []byte, item entryItem) {
	if len(payload) > recentDataCacheMaxPayloadLen {
		return
	}
	if len(c.entries) == 0 {
		c.entries = make([]recentDataCacheEntry, recentDataCacheSlots)
	}
	entry := &c.entries[int(hash)&(recentDataCacheSlots-1)]
	entry.payload = append(entry.payload[:0], payload...)
	entry.item = item
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

func validateFieldName(name string) error {
	if name == "" {
		return errFieldName
	}
	if len(name) > 64 {
		return fmt.Errorf("%w: %q", errFieldName, name)
	}
	if name[0] >= '0' && name[0] <= '9' {
		return fmt.Errorf("%w: %q", errFieldName, name)
	}
	for i := 0; i < len(name); i++ {
		c := name[i]
		if c == '_' || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			continue
		}
		return fmt.Errorf("%w: %q", errFieldName, name)
	}
	return nil
}

func startTimeForTailMonotonic(now time.Time, tailUsec uint64) time.Time {
	const maxDurationUsec = uint64(1<<63-1) / uint64(time.Microsecond)
	if tailUsec > maxDurationUsec {
		tailUsec = maxDurationUsec
	}
	return now.Add(-time.Duration(tailUsec) * time.Microsecond)
}

func lockFile(f *os.File) error {
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		return err
	}
	return nil
}

func unlockAndClose(f *os.File) error {
	err1 := syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
	err2 := f.Close()
	return errors.Join(err1, err2)
}

func (w *Writer) initialize(opts Options) error {
	// systemd v260.1 layout for deterministic uncompressed writer:
	// - File preallocated to 8 MiB (FILE_SIZE_INCREASE rounding)
	// - FIELD_HASH_TABLE object starts at headerSize (272)
	// - DATA_HASH_TABLE object starts after FIELD_HASH_TABLE (aligned)
	// - Hash table offsets in header point to items array (object start + 16)

	dataSize := uint64(opts.DataHashTableBuckets * hashItemSize)
	fieldSize := uint64(opts.FieldHashTableBuckets * hashItemSize)

	// Object starts (systemd creates FIELD_HASH_TABLE first, then DATA_HASH_TABLE)
	fieldObjectOffset := uint64(headerSize)
	dataObjectOffset := align8(fieldObjectOffset + objectHeaderSize + fieldSize)

	// Items array offsets (stored in header, point past the object header)
	fieldOffset := fieldObjectOffset + objectHeaderSize
	dataOffset := dataObjectOffset + objectHeaderSize

	// Append area starts after the data hash table object
	appendOffset := align8(dataObjectOffset + objectHeaderSize + dataSize)

	fileSize, ok := roundUpToFileSizeIncrease(appendOffset)
	if !ok {
		return fmt.Errorf("journal initial arena too large")
	}
	if opts.Compact && fileSize > journalCompactSizeMax {
		return fmt.Errorf("compact journal cannot exceed 4 GiB")
	}

	incFlags := uint32(incompatibleKeyedHash)
	if opts.Compression == CompressionZSTD {
		incFlags |= incompatibleCompressedZSTD
	} else if opts.Compression == CompressionXZ {
		incFlags |= incompatibleCompressedXZ
	} else if opts.Compression == CompressionLZ4 {
		incFlags |= incompatibleCompressedLZ4
	}
	if opts.Compact {
		incFlags |= incompatibleCompact
	}

	compatibleFlags := uint32(compatibleTailEntryBootID)
	if opts.Seal != nil {
		var err error
		w.seal, err = newSealState(*opts.Seal)
		if err != nil {
			return err
		}
		compatibleFlags |= compatibleSealed | compatibleSealedContinuous
	}

	w.header = journalHeader{
		signature:            [8]byte{'L', 'P', 'K', 'S', 'H', 'H', 'R', 'H'},
		compatibleFlags:      compatibleFlags,
		incompatibleFlags:    incFlags,
		state:                stateOnline,
		fileID:               opts.FileID,
		machineID:            opts.MachineID,
		seqnumID:             opts.SeqnumID,
		headerSize:           headerSize,
		arenaSize:            fileSize - headerSize,
		dataHashTableOffset:  dataOffset,
		dataHashTableSize:    dataSize,
		fieldHashTableOffset: fieldOffset,
		fieldHashTableSize:   fieldSize,
		tailObjectOffset:     dataObjectOffset, // last object start
		nObjects:             2,
	}
	w.appendOffset = appendOffset
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

	// Write FIELD_HASH_TABLE object header at object start
	if err := w.writeObjectHeader(fieldObjectOffset, objectHeader{
		typ:  objectTypeFieldHashTable,
		size: objectHeaderSize + fieldSize,
	}); err != nil {
		return err
	}

	// Write DATA_HASH_TABLE object header at object start
	if err := w.writeObjectHeader(dataObjectOffset, objectHeader{
		typ:  objectTypeDataHashTable,
		size: objectHeaderSize + dataSize,
	}); err != nil {
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

func (w *Writer) entryAdded(entryOffset, realtime, monotonic uint64, bootID UUID) {
	w.header.nEntries++
	if w.header.headEntrySeqnum == 0 {
		w.header.headEntrySeqnum = w.nextSeqnum
	}
	if w.header.headEntryRealtime == 0 {
		w.header.headEntryRealtime = realtime
	}
	w.header.tailEntrySeqnum = w.nextSeqnum
	w.header.tailEntryRealtime = realtime
	w.header.tailEntryMonotonic = monotonic
	w.header.tailEntryBootID = bootID
	w.header.tailEntryOffset = entryOffset
	w.nextSeqnum++
}

func (w *Writer) addData(payload []byte) (uint64, uint64, error) {
	hash := w.hash(payload)
	if item, ok := w.dataCache.get(hash, payload); ok {
		return item.offset, item.hash, nil
	}
	if offset, ok, err := w.findData(hash, payload); err != nil || ok {
		if ok {
			w.dataCache.insert(hash, payload, entryItem{offset: offset, hash: hash})
		}
		return offset, hash, err
	}

	offset := w.appendOffset

	var objectPayload []byte
	var compressionFlag uint8
	if w.compression == CompressionZSTD && len(payload) >= w.compressThreshold {
		if compressed, err := zstdCompress(payload); err == nil && len(compressed) < len(payload) {
			objectPayload = compressed
			compressionFlag = objectCompressedZSTD
		}
	}
	if w.compression == CompressionXZ && len(payload) >= w.compressThreshold && len(payload) >= 80 {
		if compressed, err := xzCompress(payload); err == nil && len(compressed) < len(payload) {
			objectPayload = compressed
			compressionFlag = objectCompressedXZ
		}
	}
	if w.compression == CompressionLZ4 && len(payload) >= w.compressThreshold && len(payload) >= 9 {
		if compressed := lz4Compress(payload); len(compressed) < len(payload) {
			objectPayload = compressed
			compressionFlag = objectCompressedLZ4
		}
	}
	if objectPayload == nil {
		objectPayload = payload
	}

	payloadOffset := w.dataPayloadOffset()
	size := payloadOffset + uint64(len(objectPayload))
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, 0, err
	}
	buf := make([]byte, align8(size))
	putDataHeader(buf[:dataObjectHeaderSize], dataHeader{
		object: objectHeader{typ: objectTypeData, flag: compressionFlag, size: size},
		hash:   hash,
	})
	copy(buf[payloadOffset:], objectPayload)
	if err := w.writeObject(offset, buf); err != nil {
		return 0, 0, err
	}
	if err := w.objectAdded(offset, size); err != nil {
		return 0, 0, err
	}

	if err := w.appendHashItem(w.header.dataHashTableOffset, w.header.dataHashTableSize, objectTypeData, hash, offset); err != nil {
		return 0, 0, err
	}
	w.header.nData++

	if err := w.hmacPutObject(offset, objectTypeData); err != nil {
		return 0, 0, err
	}

	if eq := bytes.IndexByte(payload, '='); eq > 0 {
		fieldPayload := payload[:eq]
		fieldHash := w.hash(fieldPayload)
		fieldOffset, fieldHeadDataOffset, err := w.addField(fieldHash, fieldPayload)
		if err != nil {
			return 0, 0, err
		}
		if err := w.writeUint64At(offset+32, fieldHeadDataOffset); err != nil {
			return 0, 0, err
		}
		if err := w.writeUint64At(fieldOffset+32, offset); err != nil {
			return 0, 0, err
		}
		w.fieldCache.insert(fieldHash, fieldPayload, fieldOffset, offset)
	}

	w.dataCache.insert(hash, payload, entryItem{offset: offset, hash: hash})
	return offset, hash, nil
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
	buf := make([]byte, align8(size))
	putFieldHeader(buf[:fieldObjectHeaderSize], fieldHeader{
		object: objectHeader{typ: objectTypeField, size: size},
		hash:   hash,
	})
	copy(buf[fieldObjectHeaderSize:], payload)
	if err := w.writeObject(offset, buf); err != nil {
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
		header, stored, err := w.readDataObject(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash && bytes.Equal(stored, payload) {
			return offset, true, nil
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
	buf := make([]byte, hashItemSize)
	if err := w.readAt(buf, offset); err != nil {
		return hashItem{}, err
	}
	return parseHashItem(buf), nil
}

func (w *Writer) writeHashItem(offset uint64, item hashItem) error {
	buf := make([]byte, hashItemSize)
	putHashItem(buf, item)
	return w.writeAt(offset, buf)
}

func (w *Writer) readDataObject(offset uint64) (dataHeader, []byte, error) {
	header, err := w.readDataHeader(offset)
	if err != nil {
		return dataHeader{}, nil, err
	}
	payloadOffset := w.dataPayloadOffset()
	if header.object.typ != objectTypeData || header.object.size < payloadOffset {
		return dataHeader{}, nil, errInvalidJournal
	}
	payload := make([]byte, header.object.size-payloadOffset)
	if err := w.readAt(payload, offset+payloadOffset); err != nil {
		return dataHeader{}, nil, err
	}
	if header.object.flag&objectCompressedZSTD != 0 {
		decoded, err := zstdDecompress(payload)
		if err != nil {
			return dataHeader{}, nil, err
		}
		payload = decoded
	} else if header.object.flag&objectCompressedXZ != 0 {
		decoded, err := xzDecompress(payload)
		if err != nil {
			return dataHeader{}, nil, err
		}
		payload = decoded
	} else if header.object.flag&objectCompressedLZ4 != 0 {
		decoded, err := lz4Decompress(payload)
		if err != nil {
			return dataHeader{}, nil, err
		}
		payload = decoded
	}
	return header, payload, nil
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
	buf := make([]byte, objectHeaderSize)
	if err := w.readAt(buf, offset); err != nil {
		return objectHeader{}, err
	}
	return parseObjectHeader(buf)
}

func (w *Writer) readDataHeader(offset uint64) (dataHeader, error) {
	buf := make([]byte, dataObjectHeaderSize)
	if err := w.readAt(buf, offset); err != nil {
		return dataHeader{}, err
	}
	return parseDataHeader(buf)
}

func (w *Writer) readFieldHeader(offset uint64) (fieldHeader, error) {
	buf := make([]byte, fieldObjectHeaderSize)
	if err := w.readAt(buf, offset); err != nil {
		return fieldHeader{}, err
	}
	return parseFieldHeader(buf)
}

func (w *Writer) writeUint64At(offset, value uint64) error {
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, value)
	return w.writeAt(offset, buf)
}

func (w *Writer) writeUint32At(offset uint64, value uint32) error {
	buf := make([]byte, 4)
	binary.LittleEndian.PutUint32(buf, value)
	return w.writeAt(offset, buf)
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
		arrayOffset, err := w.allocateOffsetArray(4)
		if err != nil {
			return err
		}
		w.header.entryArrayOffset = arrayOffset
		w.header.tailEntryArrayOffset = uint32(arrayOffset)
		w.header.tailEntryArrayNEntries = 1
		return w.writeArrayItem(arrayOffset, 0, entryOffset)
	}

	tailOffset := uint64(w.header.tailEntryArrayOffset)
	if tailOffset == 0 {
		tailOffset = w.header.entryArrayOffset
		for remaining := w.header.nEntries; ; {
			header, cap, err := w.readOffsetArrayHeader(tailOffset)
			if err != nil {
				return err
			}
			if remaining < cap || header.nextArrayOffset == 0 {
				break
			}
			remaining -= cap
			tailOffset = header.nextArrayOffset
		}
	}

	_, cap, err := w.readOffsetArrayHeader(tailOffset)
	if err != nil {
		return err
	}
	tailEntries := uint64(w.header.tailEntryArrayNEntries)
	if tailEntries == 0 {
		tailEntries = w.header.nEntries
		for offset := w.header.entryArrayOffset; offset != 0 && offset != tailOffset; {
			h, c, err := w.readOffsetArrayHeader(offset)
			if err != nil {
				return err
			}
			tailEntries -= c
			offset = h.nextArrayOffset
		}
	}
	if tailEntries < cap {
		if err := w.writeArrayItem(tailOffset, tailEntries, entryOffset); err != nil {
			return err
		}
		w.header.tailEntryArrayOffset = uint32(tailOffset)
		w.header.tailEntryArrayNEntries = uint32(tailEntries + 1)
		return nil
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

func (w *Writer) allocateOffsetArray(capacity uint64) (uint64, error) {
	offset := w.appendOffset
	size := uint64(offsetArrayObjectHeaderSize) + capacity*w.offsetArrayItemSize()
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, err
	}
	buf := make([]byte, align8(size))
	putOffsetArrayHeader(buf[:offsetArrayObjectHeaderSize], offsetArrayHeader{
		object: objectHeader{typ: objectTypeEntryArray, size: size},
	})
	if err := w.writeObject(offset, buf); err != nil {
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
	buf := make([]byte, offsetArrayObjectHeaderSize)
	if err := w.readAt(buf, offset); err != nil {
		return offsetArrayHeader{}, 0, err
	}
	header, err := parseOffsetArrayHeader(buf)
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
		if err := w.writeUint64At(dataOffset+40, entryOffset); err != nil {
			return err
		}
		return w.writeUint64At(dataOffset+56, 1)
	case 1:
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
			if err := w.writeUint32At(dataOffset+compactDataTailOffsetOffset, uint32(arrayOffset)); err != nil {
				return err
			}
			if err := w.writeUint32At(dataOffset+compactDataTailEntriesOffset, 1); err != nil {
				return err
			}
		}
		return w.writeUint64At(dataOffset+56, 2)
	default:
		if header.entryArrayOffset == 0 {
			return errInvalidJournal
		}
		tailOffset, tailEntries, err := w.appendToDataEntryArray(header.entryArrayOffset, header.nEntries-1, entryOffset)
		if err != nil {
			return err
		}
		if w.compact {
			if err := w.writeUint32At(dataOffset+compactDataTailOffsetOffset, uint32(tailOffset)); err != nil {
				return err
			}
			if err := w.writeUint32At(dataOffset+compactDataTailEntriesOffset, uint32(tailEntries)); err != nil {
				return err
			}
		}
		return w.writeUint64At(dataOffset+56, header.nEntries+1)
	}
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
	return buf.Bytes(), nil
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
