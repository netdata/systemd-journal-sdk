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
	"syscall"
	"time"

	"github.com/klauspost/compress/zstd"
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
	HeadSeqnum            uint64
	DataHashTableBuckets  int
	FieldHashTableBuckets int
	// Compression specifies the compression algorithm for DATA objects.
	// Defaults to CompressionNone.
	Compression int
	// CompressThresholdBytes is the minimum uncompressed payload size in bytes
	// required before compression is attempted. Defaults to 64.
	CompressThresholdBytes int
}

// EntryOptions controls timestamps and boot ID for one appended entry.
type EntryOptions struct {
	RealtimeUsec  uint64
	MonotonicUsec uint64
	BootID        UUID
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
	header            journalHeader
	appendOffset      uint64
	nextSeqnum        uint64
	bootID            UUID
	started           time.Time
	closed            bool
	compression       int
	compressThreshold int
}

// Create creates or truncates a journal file after acquiring the writer lock.
func Create(path string, opts Options) (*Writer, error) {
	opts = normalizeOptions(opts)

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
		compression: opts.Compression, compressThreshold: opts.CompressThresholdBytes,
	}
	if err := w.initialize(opts); err != nil {
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
	const supportedWriterIncompatible = incompatibleKeyedHash | incompatibleCompressedZSTD
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
	}
	if isZeroUUID(w.bootID) {
		w.bootID = header.fileID
	}
	if err := w.writeHeader(); err != nil {
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
	if opts.RealtimeUsec == 0 {
		opts.RealtimeUsec = uint64(now.UnixMicro())
	}
	if opts.MonotonicUsec == 0 {
		opts.MonotonicUsec = uint64(now.Sub(w.started) / time.Microsecond)
	}
	if isZeroUUID(opts.BootID) {
		opts.BootID = w.bootID
	}

	payloads := make([][]byte, 0, len(fields))
	for _, field := range fields {
		if err := validateFieldName(field.Name); err != nil {
			return err
		}
		payload := make([]byte, 0, len(field.Name)+1+len(field.Value))
		payload = append(payload, field.Name...)
		payload = append(payload, '=')
		payload = append(payload, field.Value...)
		payloads = append(payloads, payload)
	}

	items := make([]entryItem, 0, len(payloads))
	xorHash := uint64(0)
	for _, payload := range payloads {
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
	entrySize := uint64(entryObjectHeaderSize + len(items)*regularEntryItemSize)
	buf := make([]byte, align8(entrySize))
	putEntryHeader(buf[:entryObjectHeaderSize], entryHeader{
		object: objectHeader{typ: objectTypeEntry, size: entrySize},
		seqnum: w.nextSeqnum, realtime: opts.RealtimeUsec,
		monotonic: opts.MonotonicUsec, bootID: opts.BootID, xorHash: xorHash,
	})
	for i, item := range items {
		off := entryObjectHeaderSize + i*regularEntryItemSize
		binary.LittleEndian.PutUint64(buf[off:off+8], item.offset)
		binary.LittleEndian.PutUint64(buf[off+8:off+16], item.hash)
	}
	if err := w.writeObject(entryOffset, buf); err != nil {
		return err
	}
	w.objectAdded(entryOffset, entrySize)
	// Publish object reachability only after the complete entry object exists.
	// Entry count is committed last below so live stock readers see full rows.
	if err := w.publishObjectMetadata(); err != nil {
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
	w.entryAdded(opts.RealtimeUsec, opts.MonotonicUsec, opts.BootID)
	return w.publishEntryMetadata()
}

// Sync flushes file data and metadata to disk.
func (w *Writer) Sync() error {
	if w.closed {
		return errWriterClosed
	}
	if err := w.writeHeader(); err != nil {
		return err
	}
	return w.file.Sync()
}

// Close marks the file offline, syncs it, releases the writer lock, and closes it.
func (w *Writer) Close() error {
	if w.closed {
		return nil
	}
	w.header.state = stateOffline
	err1 := w.writeHeader()
	err2 := w.file.Sync()
	err3 := syscall.Flock(int(w.file.Fd()), syscall.LOCK_UN)
	err4 := w.file.Close()
	err5 := w.lock.release()
	w.lock = nil
	w.closed = true
	return errors.Join(err1, err2, err3, err4, err5)
}

// CurrentSize returns the current committed journal file size in bytes.
func (w *Writer) CurrentSize() uint64 {
	return w.appendOffset
}

func (w *Writer) archiveTo(path string) error {
	if w.closed {
		return errWriterClosed
	}
	w.header.state = stateArchived
	if err := w.writeHeader(); err != nil {
		return err
	}
	if err := w.file.Sync(); err != nil {
		return err
	}
	if err := os.Rename(w.path, path); err != nil {
		w.header.state = stateOnline
		restoreErr := w.writeHeader()
		syncErr := w.file.Sync()
		return errors.Join(err, restoreErr, syncErr)
	}
	w.path = path
	dirErr := syncJournalDirectory(path)
	unlockErr := syscall.Flock(int(w.file.Fd()), syscall.LOCK_UN)
	closeErr := w.file.Close()
	lockErr := w.lock.release()
	w.lock = nil
	w.closed = true
	if err := errors.Join(dirErr, unlockErr, closeErr, lockErr); err != nil {
		return err
	}
	return nil
}

type entryItem struct {
	offset uint64
	hash   uint64
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
	if opts.DataHashTableBuckets == 0 {
		opts.DataHashTableBuckets = defaultDataHashBuckets
	}
	if opts.FieldHashTableBuckets == 0 {
		opts.FieldHashTableBuckets = defaultFieldHashBuckets
	}
	if opts.CompressThresholdBytes == 0 {
		opts.CompressThresholdBytes = defaultCompressThreshold
	}
	return opts
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
	dataSize := uint64(opts.DataHashTableBuckets * hashItemSize)
	fieldSize := uint64(opts.FieldHashTableBuckets * hashItemSize)
	dataOffset := uint64(headerSize + objectHeaderSize)
	fieldObjectOffset := dataOffset + dataSize
	fieldOffset := fieldObjectOffset + objectHeaderSize
	appendOffset := fieldOffset + fieldSize

	incFlags := uint32(incompatibleKeyedHash)
	if opts.Compression == CompressionZSTD {
		incFlags |= incompatibleCompressedZSTD
	}

	w.header = journalHeader{
		signature:            [8]byte{'L', 'P', 'K', 'S', 'H', 'H', 'R', 'H'},
		incompatibleFlags:    incFlags,
		state:                stateOnline,
		fileID:               opts.FileID,
		machineID:            opts.MachineID,
		tailEntryBootID:      opts.BootID,
		seqnumID:             opts.SeqnumID,
		headerSize:           headerSize,
		arenaSize:            appendOffset - headerSize,
		dataHashTableOffset:  dataOffset,
		dataHashTableSize:    dataSize,
		fieldHashTableOffset: fieldOffset,
		fieldHashTableSize:   fieldSize,
		tailObjectOffset:     fieldObjectOffset,
		nObjects:             2,
	}
	w.appendOffset = appendOffset
	w.nextSeqnum = opts.HeadSeqnum

	if err := w.file.Truncate(int64(appendOffset)); err != nil {
		return err
	}
	if err := w.writeHeader(); err != nil {
		return err
	}
	if err := w.writeObjectHeader(dataOffset-objectHeaderSize, objectHeader{typ: objectTypeDataHashTable, size: objectHeaderSize + dataSize}); err != nil {
		return err
	}
	return w.writeObjectHeader(fieldObjectOffset, objectHeader{typ: objectTypeFieldHashTable, size: objectHeaderSize + fieldSize})
}

func (w *Writer) writeHeader() error {
	buf := make([]byte, headerSize)
	putHeader(buf, w.header)
	_, err := w.file.WriteAt(buf, 0)
	return err
}

func (w *Writer) publishObjectMetadata() error {
	if err := w.writeUint64At(96, w.header.arenaSize); err != nil {
		return err
	}
	if err := w.writeUint64At(136, w.header.tailObjectOffset); err != nil {
		return err
	}
	return w.writeUint64At(144, w.header.nObjects)
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
	return w.writeUint64At(152, w.header.nEntries)
}

func (w *Writer) writeObjectHeader(offset uint64, header objectHeader) error {
	buf := make([]byte, objectHeaderSize)
	putObjectHeader(buf, header)
	_, err := w.file.WriteAt(buf, int64(offset))
	return err
}

func (w *Writer) writeObject(offset uint64, buf []byte) error {
	_, err := w.file.WriteAt(buf, int64(offset))
	return err
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

func (w *Writer) objectAdded(offset, size uint64) {
	w.header.tailObjectOffset = offset
	w.appendOffset = align8(offset + size)
	w.header.nObjects++
	w.header.arenaSize = w.appendOffset - w.header.headerSize
}

func (w *Writer) entryAdded(realtime, monotonic uint64, bootID UUID) {
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
	w.nextSeqnum++
}

func (w *Writer) addData(payload []byte) (uint64, uint64, error) {
	hash := w.hash(payload)
	if offset, ok, err := w.findData(hash, payload); err != nil || ok {
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
	if objectPayload == nil {
		objectPayload = payload
	}

	size := uint64(dataObjectHeaderSize + len(objectPayload))
	buf := make([]byte, align8(size))
	putDataHeader(buf[:dataObjectHeaderSize], dataHeader{
		object: objectHeader{typ: objectTypeData, flag: compressionFlag, size: size},
		hash:   hash,
	})
	copy(buf[dataObjectHeaderSize:], objectPayload)
	if err := w.writeObject(offset, buf); err != nil {
		return 0, 0, err
	}
	w.objectAdded(offset, size)

	if err := w.appendHashItem(w.header.dataHashTableOffset, w.header.dataHashTableSize, objectTypeData, hash, offset); err != nil {
		return 0, 0, err
	}

	if eq := bytes.IndexByte(payload, '='); eq > 0 {
		fieldOffset, err := w.addField(payload[:eq])
		if err != nil {
			return 0, 0, err
		}
		field, err := w.readFieldHeader(fieldOffset)
		if err != nil {
			return 0, 0, err
		}
		if err := w.writeUint64At(offset+32, field.headDataOffset); err != nil {
			return 0, 0, err
		}
		if err := w.writeUint64At(fieldOffset+32, offset); err != nil {
			return 0, 0, err
		}
	}

	return offset, hash, nil
}

func (w *Writer) addField(payload []byte) (uint64, error) {
	hash := w.hash(payload)
	if offset, ok, err := w.findField(hash, payload); err != nil || ok {
		return offset, err
	}

	offset := w.appendOffset
	size := uint64(fieldObjectHeaderSize + len(payload))
	buf := make([]byte, align8(size))
	putFieldHeader(buf[:fieldObjectHeaderSize], fieldHeader{
		object: objectHeader{typ: objectTypeField, size: size},
		hash:   hash,
	})
	copy(buf[fieldObjectHeaderSize:], payload)
	if err := w.writeObject(offset, buf); err != nil {
		return 0, err
	}
	w.objectAdded(offset, size)

	if err := w.appendHashItem(w.header.fieldHashTableOffset, w.header.fieldHashTableSize, objectTypeField, hash, offset); err != nil {
		return 0, err
	}
	return offset, nil
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
		oh, err := readObjectHeaderAt(w.file, item.head)
		if err != nil {
			return err
		}
		if oh.typ != typ {
			return errInvalidJournal
		}
	}
	return w.writeHashItem(bucketOffset, item)
}

func (w *Writer) findData(hash uint64, payload []byte) (uint64, bool, error) {
	bucketOffset := w.header.dataHashTableOffset + (hash%(w.header.dataHashTableSize/hashItemSize))*hashItemSize
	item, err := w.readHashItem(bucketOffset)
	if err != nil {
		return 0, false, err
	}

	for offset := item.head; offset != 0; {
		header, stored, err := w.readDataObject(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash && bytes.Equal(stored, payload) {
			return offset, true, nil
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

	for offset := item.head; offset != 0; {
		header, stored, err := w.readFieldObject(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash && bytes.Equal(stored, payload) {
			return offset, true, nil
		}
		offset = header.nextHashOffset
	}
	return 0, false, nil
}

func (w *Writer) readHashItem(offset uint64) (hashItem, error) {
	buf := make([]byte, hashItemSize)
	if _, err := w.file.ReadAt(buf, int64(offset)); err != nil {
		return hashItem{}, err
	}
	return parseHashItem(buf), nil
}

func (w *Writer) writeHashItem(offset uint64, item hashItem) error {
	buf := make([]byte, hashItemSize)
	putHashItem(buf, item)
	_, err := w.file.WriteAt(buf, int64(offset))
	return err
}

func (w *Writer) readDataObject(offset uint64) (dataHeader, []byte, error) {
	header, err := w.readDataHeader(offset)
	if err != nil {
		return dataHeader{}, nil, err
	}
	if header.object.typ != objectTypeData || header.object.size < dataObjectHeaderSize {
		return dataHeader{}, nil, errInvalidJournal
	}
	payload := make([]byte, header.object.size-dataObjectHeaderSize)
	if _, err := w.file.ReadAt(payload, int64(offset+dataObjectHeaderSize)); err != nil {
		return dataHeader{}, nil, err
	}
	if header.object.flag&objectCompressedZSTD != 0 {
		decoded, err := zstdDecompress(payload)
		if err != nil {
			return dataHeader{}, nil, err
		}
		payload = decoded
	} else if header.object.flag&(objectCompressedXZ|objectCompressedLZ4) != 0 {
		return dataHeader{}, nil, errUnsupportedJournal
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
	if _, err := w.file.ReadAt(payload, int64(offset+fieldObjectHeaderSize)); err != nil {
		return fieldHeader{}, nil, err
	}
	return header, payload, nil
}

func (w *Writer) readDataHeader(offset uint64) (dataHeader, error) {
	buf := make([]byte, dataObjectHeaderSize)
	if _, err := w.file.ReadAt(buf, int64(offset)); err != nil {
		return dataHeader{}, err
	}
	return parseDataHeader(buf)
}

func (w *Writer) readFieldHeader(offset uint64) (fieldHeader, error) {
	buf := make([]byte, fieldObjectHeaderSize)
	if _, err := w.file.ReadAt(buf, int64(offset)); err != nil {
		return fieldHeader{}, err
	}
	return parseFieldHeader(buf)
}

func (w *Writer) writeUint64At(offset, value uint64) error {
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, value)
	_, err := w.file.WriteAt(buf, int64(offset))
	return err
}

func (w *Writer) writeUUIDAt(offset uint64, value UUID) error {
	_, err := w.file.WriteAt(value[:], int64(offset))
	return err
}

func (w *Writer) appendToEntryArray(entryOffset uint64) error {
	if w.header.entryArrayOffset == 0 {
		arrayOffset, err := w.allocateOffsetArray(initialEntryArrayCap)
		if err != nil {
			return err
		}
		w.header.entryArrayOffset = arrayOffset
		return w.writeArrayItem(arrayOffset, 0, entryOffset)
	}

	remaining := w.header.nEntries
	offset := w.header.entryArrayOffset
	for {
		header, cap, err := w.readOffsetArrayHeader(offset)
		if err != nil {
			return err
		}
		if remaining < cap {
			return w.writeArrayItem(offset, remaining, entryOffset)
		}
		remaining -= cap
		if header.nextArrayOffset == 0 {
			newOffset, err := w.allocateOffsetArray(cap * 2)
			if err != nil {
				return err
			}
			if err := w.writeUint64At(offset+16, newOffset); err != nil {
				return err
			}
			return w.writeArrayItem(newOffset, 0, entryOffset)
		}
		offset = header.nextArrayOffset
	}
}

func (w *Writer) allocateOffsetArray(capacity uint64) (uint64, error) {
	offset := w.appendOffset
	size := uint64(offsetArrayObjectHeaderSize) + capacity*8
	buf := make([]byte, align8(size))
	putOffsetArrayHeader(buf[:offsetArrayObjectHeaderSize], offsetArrayHeader{
		object: objectHeader{typ: objectTypeEntryArray, size: size},
	})
	if err := w.writeObject(offset, buf); err != nil {
		return 0, err
	}
	w.objectAdded(offset, size)
	if err := w.publishObjectMetadata(); err != nil {
		return 0, err
	}
	return offset, nil
}

func (w *Writer) readOffsetArrayHeader(offset uint64) (offsetArrayHeader, uint64, error) {
	buf := make([]byte, offsetArrayObjectHeaderSize)
	if _, err := w.file.ReadAt(buf, int64(offset)); err != nil {
		return offsetArrayHeader{}, 0, err
	}
	header, err := parseOffsetArrayHeader(buf)
	if err != nil {
		return offsetArrayHeader{}, 0, err
	}
	if header.object.typ != objectTypeEntryArray || header.object.size < offsetArrayObjectHeaderSize {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	return header, (header.object.size - offsetArrayObjectHeaderSize) / 8, nil
}

func (w *Writer) writeArrayItem(arrayOffset, index, entryOffset uint64) error {
	return w.writeUint64At(arrayOffset+offsetArrayObjectHeaderSize+index*8, entryOffset)
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
		arrayOffset, err := w.allocateOffsetArray(initialDataEntryArrayCap)
		if err != nil {
			return err
		}
		if err := w.writeArrayItem(arrayOffset, 0, entryOffset); err != nil {
			return err
		}
		if err := w.writeUint64At(dataOffset+48, arrayOffset); err != nil {
			return err
		}
		return w.writeUint64At(dataOffset+56, 2)
	default:
		if header.entryArrayOffset == 0 {
			return errInvalidJournal
		}
		if err := w.appendToDataEntryArray(header.entryArrayOffset, header.nEntries-1, entryOffset); err != nil {
			return err
		}
		return w.writeUint64At(dataOffset+56, header.nEntries+1)
	}
}

func (w *Writer) appendToDataEntryArray(arrayOffset, currentCount, entryOffset uint64) error {
	remaining := currentCount
	offset := arrayOffset
	for {
		header, cap, err := w.readOffsetArrayHeader(offset)
		if err != nil {
			return err
		}
		if remaining < cap {
			return w.writeArrayItem(offset, remaining, entryOffset)
		}
		remaining -= cap
		if header.nextArrayOffset == 0 {
			newOffset, err := w.allocateOffsetArray(cap * 2)
			if err != nil {
				return err
			}
			if err := w.writeUint64At(offset+16, newOffset); err != nil {
				return err
			}
			return w.writeArrayItem(newOffset, 0, entryOffset)
		}
		offset = header.nextArrayOffset
	}
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
	decoder, err := zstd.NewReader(nil)
	if err != nil {
		return nil, err
	}
	defer decoder.Close()
	return decoder.DecodeAll(payload, nil)
}
