package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/klauspost/compress/zstd"
	"github.com/pierrec/lz4/v4"
	"github.com/ulikunitz/xz"
)

var (
	errEndOfEntries   = errors.New("end of entries")
	errStartOfEntries = errors.New("start of entries")
	errNotFound       = errors.New("entry not found")
	errCorruptObject  = errors.New("corrupt object")
)

type Direction int

const (
	DirectionForward  Direction = 0
	DirectionBackward Direction = 1
)

type Entry struct {
	Fields      map[string][]byte
	FieldValues map[string][][]byte
	Payloads    [][]byte
	Seqnum      uint64
	Realtime    uint64
	Monotonic   uint64
	BootID      UUID
	Cursor      string
}

type Reader struct {
	file        *os.File
	header      journalHeader
	path        string
	cleanupPath string

	cursor    uint64
	position  int
	direction Direction

	entryOffsets []uint64
	entryIndex   int
	realtimeSeek *uint64

	filter *filterBuilder
}

type filterBuilder struct {
	level0  []filterExpr
	level1  []filterExpr
	current [][]byte
}

type filterExpr interface {
	matches(*Entry) bool
}

type matchExpr struct {
	field string
	value []byte
}

type andExpr []filterExpr
type orExpr []filterExpr
type falseExpr struct{}

func (f *filterBuilder) addMatch(data []byte) {
	f.current = append(f.current, append([]byte(nil), data...))
}

func (f *filterBuilder) addDisjunction() {
	f.commitCurrent()
}

func (f *filterBuilder) addConjunction() {
	f.commitCurrent()
	f.commitLevel1()
}

func (f *filterBuilder) matches(entry *Entry) bool {
	expr := f.finalExpr()
	if expr == nil {
		return true
	}
	return expr.matches(entry)
}

func (f *filterBuilder) commitCurrent() {
	if expr := buildCurrentFilterExpr(f.current); expr != nil {
		f.level1 = append(f.level1, expr)
	}
	f.current = nil
}

func (f *filterBuilder) commitLevel1() {
	if expr := buildLevel1FilterExpr(f.level1); expr != nil {
		f.level0 = append(f.level0, expr)
	}
	f.level1 = nil
}

func (f *filterBuilder) finalExpr() filterExpr {
	level0 := append([]filterExpr(nil), f.level0...)
	level1 := append([]filterExpr(nil), f.level1...)
	if expr := buildCurrentFilterExpr(f.current); expr != nil {
		level1 = append(level1, expr)
	}
	if expr := buildLevel1FilterExpr(level1); expr != nil {
		level0 = append(level0, expr)
	}
	if len(level0) == 0 {
		return nil
	}
	if len(level0) == 1 {
		return level0[0]
	}
	return andExpr(level0)
}

func buildLevel1FilterExpr(level1 []filterExpr) filterExpr {
	if len(level1) == 0 {
		return nil
	}
	if len(level1) == 1 {
		return level1[0]
	}
	return orExpr(level1)
}

func buildCurrentFilterExpr(matches [][]byte) filterExpr {
	if len(matches) == 0 {
		return nil
	}
	byField := make(map[string][]filterExpr)
	var fields []string
	for _, item := range matches {
		eq := bytes.IndexByte(item, '=')
		if eq < 0 {
			return falseExpr{}
		}
		field := string(item[:eq])
		if _, ok := byField[field]; !ok {
			fields = append(fields, field)
		}
		byField[field] = append(byField[field], matchExpr{
			field: field,
			value: append([]byte(nil), item[eq+1:]...),
		})
	}
	sort.Strings(fields)

	parts := make([]filterExpr, 0, len(fields))
	for _, field := range fields {
		values := byField[field]
		if len(values) == 1 {
			parts = append(parts, values[0])
		} else {
			parts = append(parts, orExpr(values))
		}
	}
	if len(parts) == 1 {
		return parts[0]
	}
	return andExpr(parts)
}

func (m matchExpr) matches(entry *Entry) bool {
	if entry.FieldValues != nil {
		for _, value := range entry.FieldValues[m.field] {
			if bytes.Equal(value, m.value) {
				return true
			}
		}
		return false
	}
	value, ok := entry.Fields[m.field]
	return ok && bytes.Equal(value, m.value)
}

func (a andExpr) matches(entry *Entry) bool {
	for _, expr := range a {
		if !expr.matches(entry) {
			return false
		}
	}
	return true
}

func (o orExpr) matches(entry *Entry) bool {
	for _, expr := range o {
		if expr.matches(entry) {
			return true
		}
	}
	return false
}

func (falseExpr) matches(*Entry) bool {
	return false
}

func OpenFile(path string) (*Reader, error) {
	f, cleanupPath, err := openJournalFile(path)
	if err != nil {
		return nil, err
	}

	buf := make([]byte, headerSize)
	if _, err := f.ReadAt(buf, 0); err != nil {
		_ = closeJournalFile(f, cleanupPath)
		return nil, err
	}

	header, err := parseHeader(buf)
	if err != nil {
		_ = closeJournalFile(f, cleanupPath)
		return nil, err
	}

	const supportedReaderIncompatible = incompatibleKeyedHash | incompatibleCompressedZSTD | incompatibleCompressedXZ | incompatibleCompressedLZ4 | incompatibleCompact
	if header.incompatibleFlags&^supportedReaderIncompatible != 0 {
		_ = closeJournalFile(f, cleanupPath)
		return nil, errUnsupportedJournal
	}

	r := &Reader{
		file:        f,
		header:      header,
		path:        path,
		cleanupPath: cleanupPath,
	}

	if err := r.loadEntryArray(); err != nil {
		_ = closeJournalFile(f, cleanupPath)
		return nil, err
	}

	return r, nil
}

func (r *Reader) Close() error {
	return closeJournalFile(r.file, r.cleanupPath)
}

func openJournalFile(path string) (*os.File, string, error) {
	if !strings.HasSuffix(path, ".zst") {
		f, err := os.Open(path)
		return f, "", err
	}

	compressed, err := os.ReadFile(path)
	if err != nil {
		return nil, "", err
	}
	decoder, err := zstd.NewReader(nil)
	if err != nil {
		return nil, "", err
	}
	defer decoder.Close()

	decoded, err := decoder.DecodeAll(compressed, nil)
	if err != nil {
		return nil, "", err
	}

	tmp, err := os.CreateTemp("", "systemd-journal-sdk-*.journal")
	if err != nil {
		return nil, "", err
	}
	cleanupPath := tmp.Name()
	if _, err := tmp.Write(decoded); err != nil {
		_ = closeJournalFile(tmp, cleanupPath)
		return nil, "", err
	}
	if _, err := tmp.Seek(0, io.SeekStart); err != nil {
		_ = closeJournalFile(tmp, cleanupPath)
		return nil, "", err
	}
	return tmp, cleanupPath, nil
}

func closeJournalFile(f *os.File, cleanupPath string) error {
	closeErr := f.Close()
	if cleanupPath == "" {
		return closeErr
	}
	removeErr := os.Remove(cleanupPath)
	return errors.Join(closeErr, removeErr)
}

func (r *Reader) Header() *journalHeader {
	return &r.header
}

func (h *journalHeader) Signature() [8]byte {
	return h.signature
}

func (h *journalHeader) State() uint8 {
	return h.state
}

func (h *journalHeader) CompatibleFlags() uint32 {
	return h.compatibleFlags
}

func (h *journalHeader) IncompatibleFlags() uint32 {
	return h.incompatibleFlags
}

func (h *journalHeader) HeaderSize() uint64 {
	return h.headerSize
}

func (r *Reader) loadEntryArray() error {
	if r.header.entryArrayOffset == 0 {
		r.entryOffsets = nil
		return nil
	}

	var offsets []uint64
	offset := r.header.entryArrayOffset
	remaining := r.header.nEntries
	for offset != 0 && remaining > 0 {
		header, capacity, err := r.readOffsetArrayHeader(offset)
		if err != nil {
			return err
		}
		toRead := capacity
		if remaining < toRead {
			toRead = remaining
		}
		itemSize := r.offsetArrayItemSize()
		dataOffset := offset + offsetArrayObjectHeaderSize
		dataSize := toRead * itemSize
		buf := make([]byte, dataSize)
		if _, err := r.file.ReadAt(buf, int64(dataOffset)); err != nil {
			return err
		}
		for i := uint64(0); i < toRead; i++ {
			var off uint64
			if r.header.isCompact() {
				off = uint64(binary.LittleEndian.Uint32(buf[i*itemSize : i*itemSize+compactOffsetArrayItemSize]))
			} else {
				off = binary.LittleEndian.Uint64(buf[i*itemSize : i*itemSize+regularOffsetArrayItemSize])
			}
			if off != 0 {
				valid, err := r.validEntryObjectOffset(off)
				if err != nil {
					return err
				}
				if valid {
					offsets = append(offsets, off)
				}
			}
		}
		remaining -= toRead
		offset = header.nextArrayOffset
	}

	r.entryOffsets = offsets
	r.entryIndex = -1
	return nil
}

func (r *Reader) validEntryObjectOffset(offset uint64) (bool, error) {
	headerBuf := make([]byte, objectHeaderSize)
	if _, err := r.file.ReadAt(headerBuf, int64(offset)); err != nil {
		return false, err
	}
	objHeader, err := parseObjectHeader(headerBuf)
	if err != nil {
		return false, err
	}
	if objHeader.typ == 0 && objHeader.size == 0 {
		return false, nil
	}
	if objHeader.typ != objectTypeEntry {
		return false, errCorruptObject
	}
	return true, nil
}

func (r *Reader) readOffsetArrayHeader(offset uint64) (offsetArrayHeader, uint64, error) {
	buf := make([]byte, offsetArrayObjectHeaderSize)
	if _, err := r.file.ReadAt(buf, int64(offset)); err != nil {
		return offsetArrayHeader{}, 0, err
	}
	header, err := parseOffsetArrayHeader(buf)
	if err != nil {
		return offsetArrayHeader{}, 0, err
	}
	if header.object.typ != objectTypeEntryArray || header.object.size < offsetArrayObjectHeaderSize {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	itemSize := r.offsetArrayItemSize()
	if (header.object.size-offsetArrayObjectHeaderSize)%itemSize != 0 {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	capacity := (header.object.size - offsetArrayObjectHeaderSize) / itemSize
	return header, capacity, nil
}

func (r *Reader) Next() error {
	if r.realtimeSeek != nil {
		idx, err := r.firstRealtimeIndexAtOrAfter(*r.realtimeSeek)
		r.realtimeSeek = nil
		if err != nil {
			return err
		}
		r.direction = DirectionForward
		if idx >= len(r.entryOffsets) {
			r.entryIndex = len(r.entryOffsets)
			return errEndOfEntries
		}
		r.entryIndex = idx
		return nil
	}
	if r.entryIndex < -1 {
		r.entryIndex = -1
	}
	r.entryIndex++
	r.direction = DirectionForward

	if r.entryIndex >= len(r.entryOffsets) {
		r.entryIndex = len(r.entryOffsets)
		return errEndOfEntries
	}
	return nil
}

func (r *Reader) Previous() error {
	if r.realtimeSeek != nil {
		idx, err := r.lastRealtimeIndexAtOrBefore(*r.realtimeSeek)
		r.realtimeSeek = nil
		if err != nil {
			return err
		}
		r.direction = DirectionBackward
		if idx < 0 {
			r.entryIndex = -1
			return errStartOfEntries
		}
		r.entryIndex = idx
		return nil
	}
	if r.entryIndex > len(r.entryOffsets) {
		r.entryIndex = len(r.entryOffsets)
	}
	r.direction = DirectionBackward
	r.entryIndex--

	if r.entryIndex < 0 {
		r.entryIndex = -1
		return errStartOfEntries
	}
	return nil
}

func (r *Reader) SeekHead() error {
	r.entryIndex = -1
	r.direction = DirectionForward
	r.realtimeSeek = nil
	return nil
}

func (r *Reader) SeekTail() error {
	r.entryIndex = len(r.entryOffsets)
	r.direction = DirectionBackward
	r.realtimeSeek = nil
	return nil
}

func (r *Reader) SeekRealtimeUsec(usec uint64) error {
	value := usec
	r.realtimeSeek = &value
	return nil
}

func (r *Reader) firstRealtimeIndexAtOrAfter(usec uint64) (int, error) {
	return sort.Search(len(r.entryOffsets), func(i int) bool {
		realtime, err := r.entryRealtimeAtIndex(i)
		return err != nil || realtime >= usec
	}), nil
}

func (r *Reader) lastRealtimeIndexAtOrBefore(usec uint64) (int, error) {
	idx := sort.Search(len(r.entryOffsets), func(i int) bool {
		realtime, err := r.entryRealtimeAtIndex(i)
		return err != nil || realtime > usec
	}) - 1
	return idx, nil
}

func (r *Reader) entryRealtimeAtIndex(index int) (uint64, error) {
	if index < 0 || index >= len(r.entryOffsets) {
		return 0, errNotFound
	}
	offset := r.entryOffsets[index]
	headerBuf := make([]byte, entryObjectHeaderSize)
	if _, err := r.file.ReadAt(headerBuf, int64(offset)); err != nil {
		return 0, err
	}
	hdr, err := parseEntryHeader(headerBuf)
	if err != nil {
		return 0, err
	}
	return hdr.realtime, nil
}

func (r *Reader) GetEntry() (*Entry, error) {
	if r.entryIndex < 0 || r.entryIndex >= len(r.entryOffsets) {
		return nil, errEndOfEntries
	}

	offset := r.entryOffsets[r.entryIndex]
	return r.readEntryAt(offset)
}

func (r *Reader) readEntryAt(offset uint64) (*Entry, error) {
	headerBuf := make([]byte, objectHeaderSize)
	if _, err := r.file.ReadAt(headerBuf, int64(offset)); err != nil {
		return nil, err
	}

	objHeader, err := parseObjectHeader(headerBuf)
	if err != nil {
		return nil, err
	}

	if objHeader.typ != objectTypeEntry {
		return nil, errCorruptObject
	}
	if objHeader.size < entryObjectHeaderSize {
		return nil, errCorruptObject
	}

	entryBuf := make([]byte, entryObjectHeaderSize)
	if _, err := r.file.ReadAt(entryBuf, int64(offset)); err != nil {
		return nil, err
	}

	entryHdr, err := parseEntryHeader(entryBuf)
	if err != nil {
		return nil, err
	}

	itemSize := r.entryItemSize()
	if (objHeader.size-entryObjectHeaderSize)%itemSize != 0 {
		return nil, errCorruptObject
	}
	nItems := (objHeader.size - entryObjectHeaderSize) / itemSize
	entries := make([]uint64, 0, nItems)
	itemsOffset := offset + entryObjectHeaderSize
	itemsSize := nItems * itemSize
	itemsBuf := make([]byte, itemsSize)
	if _, err := r.file.ReadAt(itemsBuf, int64(itemsOffset)); err != nil {
		return nil, err
	}
	for i := uint64(0); i < nItems; i++ {
		var dataOff uint64
		if r.header.isCompact() {
			dataOff = uint64(binary.LittleEndian.Uint32(itemsBuf[i*itemSize : i*itemSize+compactEntryItemSize]))
		} else {
			dataOff = binary.LittleEndian.Uint64(itemsBuf[i*itemSize : i*itemSize+8])
		}
		if dataOff != 0 {
			entries = append(entries, dataOff)
		}
	}

	fields := make(map[string][]byte)
	fieldValues := make(map[string][][]byte)
	payloads := make([][]byte, 0, len(entries))
	for _, dataOff := range entries {
		payload, err := r.readDataPayload(dataOff)
		if err != nil {
			return nil, fmt.Errorf("read data object at offset %d for entry at offset %d: %w", dataOff, offset, err)
		}
		eq := bytes.IndexByte(payload, '=')
		if eq < 0 {
			return nil, fmt.Errorf("%w: data object at offset %d has no field separator", errCorruptObject, dataOff)
		}
		payloads = append(payloads, append([]byte(nil), payload...))
		name := string(payload[:eq])
		value := payload[eq+1:]
		copied := append([]byte(nil), value...)
		if _, ok := fields[name]; !ok {
			fields[name] = copied
		}
		fieldValues[name] = append(fieldValues[name], copied)
	}

	cursor := r.makeCursor(offset, entryHdr)

	return &Entry{
		Fields:      fields,
		FieldValues: fieldValues,
		Payloads:    payloads,
		Seqnum:      entryHdr.seqnum,
		Realtime:    entryHdr.realtime,
		Monotonic:   entryHdr.monotonic,
		BootID:      entryHdr.bootID,
		Cursor:      cursor,
	}, nil
}

func (r *Reader) makeCursor(entryOffset uint64, hdr *entryHeader) string {
	return fmt.Sprintf("s=%s;j=%s;c=%016x;n=%d",
		r.header.seqnumID.String(),
		hdr.bootID.String(),
		hdr.realtime,
		hdr.seqnum)
}

func (r *Reader) readDataPayload(offset uint64) ([]byte, error) {
	headerBuf := make([]byte, dataObjectHeaderSize)
	if _, err := r.file.ReadAt(headerBuf, int64(offset)); err != nil {
		return nil, err
	}

	dataHdr, err := parseDataHeader(headerBuf)
	if err != nil {
		return nil, err
	}

	if dataHdr.object.typ != objectTypeData {
		return nil, errCorruptObject
	}
	payloadOffset := r.dataPayloadOffset()
	if dataHdr.object.size < payloadOffset {
		return nil, errCorruptObject
	}

	payloadLen := dataHdr.object.size - payloadOffset
	payload := make([]byte, payloadLen)
	if _, err := r.file.ReadAt(payload, int64(offset+payloadOffset)); err != nil {
		return nil, err
	}
	if dataHdr.object.flag&objectCompressedZSTD != 0 {
		decoder, err := zstd.NewReader(nil)
		if err != nil {
			return nil, err
		}
		defer decoder.Close()
		decoded, err := decoder.DecodeAll(payload, nil)
		if err != nil {
			return nil, err
		}
		payload = decoded
	} else if dataHdr.object.flag&objectCompressedXZ != 0 {
		r, err := xz.NewReader(bytes.NewReader(payload))
		if err != nil {
			return nil, err
		}
		decoded, err := readAllLimited(r, maxUncompressedDataObjectSize)
		if err != nil {
			return nil, err
		}
		payload = decoded
	} else if dataHdr.object.flag&objectCompressedLZ4 != 0 {
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
		payload = decoded
	}

	return payload, nil
}

func (r *Reader) entryItemSize() uint64 {
	if r.header.isCompact() {
		return compactEntryItemSize
	}
	return regularEntryItemSize
}

func (r *Reader) offsetArrayItemSize() uint64 {
	if r.header.isCompact() {
		return compactOffsetArrayItemSize
	}
	return regularOffsetArrayItemSize
}

func (r *Reader) dataPayloadOffset() uint64 {
	if r.header.isCompact() {
		return compactDataObjectHeaderSize
	}
	return dataObjectHeaderSize
}

func parseEntryHeader(src []byte) (*entryHeader, error) {
	if len(src) < entryObjectHeaderSize {
		return nil, errInvalidJournal
	}

	objHeader, err := parseObjectHeader(src[0:16])
	if err != nil {
		return nil, err
	}

	var hdr entryHeader
	hdr.object = objHeader
	hdr.seqnum = binary.LittleEndian.Uint64(src[16:24])
	hdr.realtime = binary.LittleEndian.Uint64(src[24:32])
	hdr.monotonic = binary.LittleEndian.Uint64(src[32:40])
	copy(hdr.bootID[:], src[40:56])
	hdr.xorHash = binary.LittleEndian.Uint64(src[56:64])

	return &hdr, nil
}

func (r *Reader) AddMatch(data []byte) {
	if r.filter == nil {
		r.filter = &filterBuilder{}
	}
	r.filter.addMatch(data)
}

func (r *Reader) AddDisjunction() {
	if r.filter == nil {
		r.filter = &filterBuilder{}
	}
	r.filter.addDisjunction()
}

func (r *Reader) AddConjunction() {
	if r.filter == nil {
		r.filter = &filterBuilder{}
	}
	r.filter.addConjunction()
}

func (r *Reader) FlushMatches() {
	r.filter = nil
}

func (r *Reader) GetRealtimeUsec() (uint64, error) {
	if r.entryIndex < 0 || r.entryIndex >= len(r.entryOffsets) {
		return 0, errEndOfEntries
	}

	offset := r.entryOffsets[r.entryIndex]
	headerBuf := make([]byte, entryObjectHeaderSize)
	if _, err := r.file.ReadAt(headerBuf, int64(offset)); err != nil {
		return 0, err
	}

	hdr, err := parseEntryHeader(headerBuf)
	if err != nil {
		return 0, err
	}

	return hdr.realtime, nil
}

func (r *Reader) GetCursor() (string, error) {
	if r.entryIndex < 0 || r.entryIndex >= len(r.entryOffsets) {
		return "", errEndOfEntries
	}

	offset := r.entryOffsets[r.entryIndex]
	headerBuf := make([]byte, entryObjectHeaderSize)
	if _, err := r.file.ReadAt(headerBuf, int64(offset)); err != nil {
		return "", err
	}

	hdr, err := parseEntryHeader(headerBuf)
	if err != nil {
		return "", err
	}

	return r.makeCursor(offset, hdr), nil
}

func (r *Reader) TestCursor(cursor string) (bool, error) {
	current, err := r.GetCursor()
	if err != nil {
		return false, err
	}
	return current == cursor, nil
}

func (r *Reader) QueryUnique(fieldName string) ([][]byte, error) {
	unique := make(map[string]struct{})
	var results [][]byte

	for _, offset := range r.entryOffsets {
		entry, err := r.readEntryAt(offset)
		if err != nil {
			continue
		}

		if values, ok := entry.FieldValues[fieldName]; ok {
			for _, value := range values {
				key := string(value)
				if _, exists := unique[key]; !exists {
					unique[key] = struct{}{}
					results = append(results, value)
				}
			}
		}
	}

	return results, nil
}

func (r *Reader) EnumerateFields() (map[string]struct{}, error) {
	fields := make(map[string]struct{})

	for _, offset := range r.entryOffsets {
		entry, err := r.readEntryAt(offset)
		if err != nil {
			continue
		}

		for name := range entry.Fields {
			fields[name] = struct{}{}
		}
	}

	return fields, nil
}

func (r *Reader) Step() (bool, error) {
	for {
		if err := r.Next(); err != nil {
			if errors.Is(err, errEndOfEntries) {
				return false, nil
			}
			return false, err
		}

		if r.filter == nil {
			return true, nil
		}

		entry, err := r.GetEntry()
		if err != nil {
			return false, err
		}

		if r.filter.matches(entry) {
			return true, nil
		}
	}
}

func (r *Reader) StepBack() (bool, error) {
	for {
		if err := r.Previous(); err != nil {
			if errors.Is(err, errStartOfEntries) {
				return false, nil
			}
			return false, err
		}

		if r.filter == nil {
			return true, nil
		}

		entry, err := r.GetEntry()
		if err != nil {
			return false, err
		}

		if r.filter.matches(entry) {
			return true, nil
		}
	}
}

type DirectoryReader struct {
	files             []*Reader
	index             int
	filter            *filterBuilder
	realtimeSeek      *uint64
	realtimeSeekBound *directoryRealtimeSeekBound
	candidates        []*directoryCandidate
	currentKey        *directoryEntryKey
	direction         Direction
	hasDirection      bool
	bootNewest        map[UUID]directoryBootNewest
}

func OpenDirectory(path string) (*DirectoryReader, error) {
	paths, err := collectJournalFiles(path)
	if err != nil {
		return nil, err
	}

	var readers []*Reader
	for _, filePath := range paths {
		r, err := OpenFile(filePath)
		if err != nil {
			continue
		}
		readers = append(readers, r)
	}

	return newDirectoryReader(readers, true)
}

func OpenFiles(paths []string) (*DirectoryReader, error) {
	readers := make([]*Reader, 0, len(paths))
	for _, path := range paths {
		if !isJournalFileName(path) {
			return nil, fmt.Errorf("not a journal file: %s", path)
		}
		r, err := OpenFile(path)
		if err != nil {
			return nil, err
		}
		readers = append(readers, r)
	}
	return newDirectoryReader(readers, false)
}

func newDirectoryReader(readers []*Reader, allowEmpty bool) (*DirectoryReader, error) {
	if len(readers) == 0 && !allowEmpty {
		return nil, errors.New("no journal files found")
	}
	sort.Slice(readers, func(i, j int) bool {
		if readers[i].header.headEntryRealtime != readers[j].header.headEntryRealtime {
			return readers[i].header.headEntryRealtime < readers[j].header.headEntryRealtime
		}
		return readers[i].header.headEntrySeqnum < readers[j].header.headEntrySeqnum
	})

	return &DirectoryReader{
		files:      readers,
		index:      -1,
		candidates: make([]*directoryCandidate, len(readers)),
		bootNewest: buildDirectoryBootNewest(readers),
	}, nil
}

type directoryCandidate struct {
	readerIndex int
	key         directoryEntryKey
}

type directoryRealtimeSeekBound struct {
	usec      uint64
	direction Direction
}

type directoryEntryKey struct {
	seqnumID  UUID
	seqnum    uint64
	bootID    UUID
	monotonic uint64
	realtime  uint64
	xorHash   uint64
}

type directoryBootNewest struct {
	machineID UUID
	monotonic uint64
	realtime  uint64
}

func isJournalFileName(name string) bool {
	return strings.HasSuffix(name, ".journal") ||
		strings.HasSuffix(name, ".journal~") ||
		strings.HasSuffix(name, ".journal.zst") ||
		strings.HasSuffix(name, ".journal~.zst")
}

func collectJournalFiles(path string) ([]string, error) {
	entries, err := os.ReadDir(path)
	if err != nil {
		return nil, err
	}

	var files []string
	for _, entry := range entries {
		fullPath := filepath.Join(path, entry.Name())
		if directoryEntryIsRegularFile(fullPath) && isJournalFileName(entry.Name()) {
			files = append(files, fullPath)
		}
	}

	for _, entry := range entries {
		if !isJournalSubdirName(entry.Name()) {
			continue
		}
		childPath := filepath.Join(path, entry.Name())
		if !directoryEntryIsDirectory(childPath) {
			continue
		}
		children, err := os.ReadDir(childPath)
		if err != nil {
			continue
		}
		for _, child := range children {
			fullPath := filepath.Join(childPath, child.Name())
			if directoryEntryIsRegularFile(fullPath) && isJournalFileName(child.Name()) {
				files = append(files, fullPath)
			}
		}
	}

	sort.Strings(files)
	return files, nil
}

func directoryEntryIsRegularFile(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.Mode().IsRegular()
}

func directoryEntryIsDirectory(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}

func isJournalSubdirName(name string) bool {
	if strings.Contains(name, ".") {
		return false
	}
	return id128StringValid(name)
}

func id128StringValid(s string) bool {
	if len(s) == 32 {
		for _, ch := range s {
			if !isASCIIHex(ch) {
				return false
			}
		}
		return true
	}
	if len(s) == 36 {
		for i, ch := range s {
			if i == 8 || i == 13 || i == 18 || i == 23 {
				if ch != '-' {
					return false
				}
				continue
			}
			if !isASCIIHex(ch) {
				return false
			}
		}
		return true
	}
	return false
}

func isASCIIHex(ch rune) bool {
	return (ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f') || (ch >= 'A' && ch <= 'F')
}

func buildDirectoryBootNewest(readers []*Reader) map[UUID]directoryBootNewest {
	boots := make(map[UUID]directoryBootNewest)
	for _, r := range readers {
		bootID := r.header.tailEntryBootID
		if uuidIsZero(bootID) {
			continue
		}
		current, ok := boots[bootID]
		if !ok || r.header.tailEntryMonotonic > current.monotonic {
			boots[bootID] = directoryBootNewest{
				machineID: r.header.machineID,
				monotonic: r.header.tailEntryMonotonic,
				realtime:  r.header.tailEntryRealtime,
			}
		}
	}
	return boots
}

func uuidIsZero(id UUID) bool {
	return id == (UUID{})
}

func (dr *DirectoryReader) Close() error {
	var errs []error
	for _, r := range dr.files {
		if err := r.Close(); err != nil {
			errs = append(errs, err)
		}
	}
	return errors.Join(errs...)
}

func (dr *DirectoryReader) Next() error {
	ok, err := dr.stepMerged(DirectionForward, false)
	if err != nil {
		return err
	}
	if !ok {
		return errEndOfEntries
	}
	return nil
}

func (dr *DirectoryReader) GetEntry() (*Entry, error) {
	if dr.index < 0 || dr.index >= len(dr.files) {
		return nil, errEndOfEntries
	}
	return dr.files[dr.index].GetEntry()
}

func (dr *DirectoryReader) Step() (bool, error) {
	return dr.stepMerged(DirectionForward, true)
}

func (dr *DirectoryReader) StepBack() (bool, error) {
	return dr.stepMerged(DirectionBackward, true)
}

func (dr *DirectoryReader) Previous() error {
	ok, err := dr.stepMerged(DirectionBackward, false)
	if err != nil {
		return err
	}
	if !ok {
		return errStartOfEntries
	}
	return nil
}

func (dr *DirectoryReader) currentReader() (*Reader, error) {
	if dr.index < 0 || dr.index >= len(dr.files) {
		return nil, errEndOfEntries
	}
	return dr.files[dr.index], nil
}

func (dr *DirectoryReader) AddMatch(data []byte) {
	if dr.filter == nil {
		dr.filter = &filterBuilder{}
	}
	dr.filter.addMatch(data)
	dr.resetMergeState()
}

func (dr *DirectoryReader) AddDisjunction() {
	if dr.filter == nil {
		dr.filter = &filterBuilder{}
	}
	dr.filter.addDisjunction()
	dr.resetMergeState()
}

func (dr *DirectoryReader) AddConjunction() {
	if dr.filter == nil {
		dr.filter = &filterBuilder{}
	}
	dr.filter.addConjunction()
	dr.resetMergeState()
}

func (dr *DirectoryReader) FlushMatches() {
	dr.filter = nil
	dr.resetMergeState()
}

func (dr *DirectoryReader) GetRealtimeUsec() (uint64, error) {
	r, err := dr.currentReader()
	if err != nil {
		return 0, err
	}
	return r.GetRealtimeUsec()
}

func (dr *DirectoryReader) GetCursor() (string, error) {
	r, err := dr.currentReader()
	if err != nil {
		return "", err
	}
	return r.GetCursor()
}

func (dr *DirectoryReader) TestCursor(cursor string) (bool, error) {
	current, err := dr.GetCursor()
	if err != nil {
		return false, err
	}
	return current == cursor, nil
}

func (dr *DirectoryReader) QueryUnique(fieldName string) ([][]byte, error) {
	unique := make(map[string]struct{})
	var results [][]byte

	for _, r := range dr.files {
		values, err := r.QueryUnique(fieldName)
		if err != nil {
			continue
		}
		for _, v := range values {
			key := string(v)
			if _, exists := unique[key]; !exists {
				unique[key] = struct{}{}
				results = append(results, v)
			}
		}
	}

	return results, nil
}

func (dr *DirectoryReader) EnumerateFields() (map[string]struct{}, error) {
	fields := make(map[string]struct{})

	for _, r := range dr.files {
		rFields, err := r.EnumerateFields()
		if err != nil {
			continue
		}
		for name := range rFields {
			fields[name] = struct{}{}
		}
	}

	return fields, nil
}

func (dr *DirectoryReader) SeekHead() error {
	dr.index = -1
	dr.realtimeSeek = nil
	dr.realtimeSeekBound = nil
	dr.currentKey = nil
	dr.hasDirection = false
	dr.resetCandidates()
	for _, r := range dr.files {
		if err := r.SeekHead(); err != nil {
			return err
		}
	}
	return nil
}

func (dr *DirectoryReader) SeekTail() error {
	dr.index = -1
	dr.realtimeSeek = nil
	dr.realtimeSeekBound = nil
	dr.currentKey = nil
	dr.hasDirection = false
	dr.resetCandidates()
	for _, r := range dr.files {
		if err := r.SeekTail(); err != nil {
			return err
		}
	}
	return nil
}

func (dr *DirectoryReader) SeekRealtimeUsec(usec uint64) error {
	value := usec
	dr.realtimeSeek = &value
	dr.realtimeSeekBound = nil
	dr.currentKey = nil
	dr.hasDirection = false
	dr.resetCandidates()
	return nil
}

func (dr *DirectoryReader) stepMerged(direction Direction, applyFilter bool) (bool, error) {
	if err := dr.prepareMergeDirection(direction); err != nil {
		return false, err
	}

	var best *directoryCandidate
	for i := range dr.files {
		if err := dr.fillCandidate(i, direction, applyFilter); err != nil {
			return false, err
		}
		candidate := dr.candidates[i]
		if candidate == nil {
			continue
		}
		if best == nil {
			best = candidate
			continue
		}
		cmp := dr.compareEntryKeys(candidate.key, best.key)
		if (direction == DirectionForward && cmp < 0) || (direction == DirectionBackward && cmp > 0) {
			best = candidate
		}
	}

	if best == nil {
		dr.index = -1
		dr.realtimeSeekBound = nil
		return false, nil
	}

	dr.index = best.readerIndex
	key := best.key
	dr.currentKey = &key
	dr.candidates[best.readerIndex] = nil
	dr.realtimeSeekBound = nil
	return true, nil
}

func (dr *DirectoryReader) prepareMergeDirection(direction Direction) error {
	if len(dr.files) == 0 {
		return nil
	}

	if dr.realtimeSeek != nil {
		usec := *dr.realtimeSeek
		dr.realtimeSeek = nil
		for _, r := range dr.files {
			if err := r.SeekRealtimeUsec(usec); err != nil {
				return err
			}
		}
		dr.resetCandidates()
		dr.realtimeSeekBound = &directoryRealtimeSeekBound{usec: usec, direction: direction}
		dr.direction = direction
		dr.hasDirection = true
		return nil
	}

	if dr.hasDirection && dr.direction == direction {
		return nil
	}

	if dr.currentKey != nil {
		for _, r := range dr.files {
			if err := r.SeekRealtimeUsec(dr.currentKey.realtime); err != nil {
				return err
			}
		}
	} else if direction == DirectionForward {
		for _, r := range dr.files {
			if err := r.SeekHead(); err != nil {
				return err
			}
		}
	} else {
		for _, r := range dr.files {
			if err := r.SeekTail(); err != nil {
				return err
			}
		}
	}

	dr.resetCandidates()
	dr.direction = direction
	dr.hasDirection = true
	return nil
}

func (dr *DirectoryReader) fillCandidate(readerIndex int, direction Direction, applyFilter bool) error {
	if dr.candidates[readerIndex] != nil {
		return nil
	}

	r := dr.files[readerIndex]
	for {
		ok, err := stepReaderRaw(r, direction)
		if err != nil {
			return err
		}
		if !ok {
			return nil
		}

		if applyFilter && dr.filter != nil {
			entry, err := r.GetEntry()
			if err != nil {
				return err
			}
			if !dr.filter.matches(entry) {
				continue
			}
		}

		key, err := r.currentDirectoryEntryKey()
		if err != nil {
			return err
		}
		if dr.realtimeSeekBound != nil {
			bound := dr.realtimeSeekBound
			if (bound.direction == DirectionForward && key.realtime < bound.usec) ||
				(bound.direction == DirectionBackward && key.realtime > bound.usec) {
				continue
			}
		}
		if dr.currentKey != nil {
			cmp := dr.compareEntryKeys(key, *dr.currentKey)
			if (direction == DirectionForward && cmp <= 0) || (direction == DirectionBackward && cmp >= 0) {
				continue
			}
		}

		dr.candidates[readerIndex] = &directoryCandidate{
			readerIndex: readerIndex,
			key:         key,
		}
		return nil
	}
}

func stepReaderRaw(r *Reader, direction Direction) (bool, error) {
	if direction == DirectionForward {
		err := r.Next()
		if errors.Is(err, errEndOfEntries) {
			return false, nil
		}
		return err == nil, err
	}

	err := r.Previous()
	if errors.Is(err, errStartOfEntries) {
		return false, nil
	}
	return err == nil, err
}

func (r *Reader) currentDirectoryEntryKey() (directoryEntryKey, error) {
	if r.entryIndex < 0 || r.entryIndex >= len(r.entryOffsets) {
		return directoryEntryKey{}, errEndOfEntries
	}

	offset := r.entryOffsets[r.entryIndex]
	entryBuf := make([]byte, entryObjectHeaderSize)
	if _, err := r.file.ReadAt(entryBuf, int64(offset)); err != nil {
		return directoryEntryKey{}, err
	}
	hdr, err := parseEntryHeader(entryBuf)
	if err != nil {
		return directoryEntryKey{}, err
	}

	return directoryEntryKey{
		seqnumID:  r.header.seqnumID,
		seqnum:    hdr.seqnum,
		bootID:    hdr.bootID,
		monotonic: hdr.monotonic,
		realtime:  hdr.realtime,
		xorHash:   hdr.xorHash,
	}, nil
}

func (dr *DirectoryReader) compareEntryKeys(a, b directoryEntryKey) int {
	if a.bootID == b.bootID &&
		a.monotonic == b.monotonic &&
		a.realtime == b.realtime &&
		a.xorHash == b.xorHash &&
		a.seqnumID == b.seqnumID &&
		a.seqnum == b.seqnum {
		return 0
	}

	if a.seqnumID == b.seqnumID {
		if cmp := cmpUint64(a.seqnum, b.seqnum); cmp != 0 {
			return cmp
		}
	}

	if a.bootID == b.bootID {
		if cmp := cmpUint64(a.monotonic, b.monotonic); cmp != 0 {
			return cmp
		}
	} else if cmp := dr.compareBootIDs(a.bootID, b.bootID); cmp != 0 {
		return cmp
	}

	if cmp := cmpUint64(a.realtime, b.realtime); cmp != 0 {
		return cmp
	}
	return cmpUint64(a.xorHash, b.xorHash)
}

func (dr *DirectoryReader) compareBootIDs(a, b UUID) int {
	aNewest, okA := dr.bootNewest[a]
	bNewest, okB := dr.bootNewest[b]
	if !okA || !okB || aNewest.machineID != bNewest.machineID {
		return 0
	}
	return cmpUint64(aNewest.realtime, bNewest.realtime)
}

func cmpUint64(a, b uint64) int {
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	return 0
}

func (dr *DirectoryReader) resetMergeState() {
	dr.currentKey = nil
	dr.hasDirection = false
	dr.index = -1
	dr.realtimeSeekBound = nil
	dr.resetCandidates()
}

func (dr *DirectoryReader) resetCandidates() {
	if len(dr.candidates) != len(dr.files) {
		dr.candidates = make([]*directoryCandidate, len(dr.files))
		return
	}
	for i := range dr.candidates {
		dr.candidates[i] = nil
	}
}

func (dr *DirectoryReader) ListBoots() ([]BootInfo, error) {
	type bootEntry struct {
		bootID    UUID
		firstSeq  uint64
		lastSeq   uint64
		firstTime uint64
		lastTime  uint64
	}

	bootMap := make(map[string]*bootEntry)

	for _, r := range dr.files {
		bootID := r.header.tailEntryBootID
		key := bootID.String()

		if entry, ok := bootMap[key]; ok {
			if r.header.headEntrySeqnum < entry.firstSeq {
				entry.firstSeq = r.header.headEntrySeqnum
			}
			if r.header.headEntryRealtime < entry.firstTime {
				entry.firstTime = r.header.headEntryRealtime
			}
			if r.header.tailEntrySeqnum > entry.lastSeq {
				entry.lastSeq = r.header.tailEntrySeqnum
			}
			if r.header.tailEntryRealtime > entry.lastTime {
				entry.lastTime = r.header.tailEntryRealtime
			}
		} else {
			bootMap[key] = &bootEntry{
				bootID:    bootID,
				firstSeq:  r.header.headEntrySeqnum,
				lastSeq:   r.header.tailEntrySeqnum,
				firstTime: r.header.headEntryRealtime,
				lastTime:  r.header.tailEntryRealtime,
			}
		}
	}

	var boots []*bootEntry
	for _, e := range bootMap {
		boots = append(boots, e)
	}

	sort.Slice(boots, func(i, j int) bool {
		return boots[i].firstTime < boots[j].firstTime
	})

	var results []BootInfo
	offset := -(len(boots) - 1)
	for _, b := range boots {
		results = append(results, BootInfo{
			Index:      int64(offset),
			BootID:     b.bootID.String(),
			FirstEntry: int64(b.firstTime),
			LastEntry:  int64(b.lastTime),
		})
		offset++
	}

	return results, nil
}

type BootInfo struct {
	Index      int64
	BootID     string
	FirstEntry int64
	LastEntry  int64
}

func ExportEntry(entry *Entry) string {
	var buf bytes.Buffer

	if entry.Cursor != "" {
		writeExportField(&buf, "__CURSOR", []byte(entry.Cursor))
	}

	if entry.Realtime != 0 {
		writeExportField(&buf, "__REALTIME_TIMESTAMP", []byte(strconv.FormatUint(entry.Realtime, 10)))
	}

	if entry.Monotonic != 0 {
		writeExportField(&buf, "__MONOTONIC_TIMESTAMP", []byte(strconv.FormatUint(entry.Monotonic, 10)))
	}

	if entry.Seqnum != 0 {
		writeExportField(&buf, "__SEQNUM", []byte(strconv.FormatUint(entry.Seqnum, 10)))
	}

	if seqnumID, _, _, _, err := ParseCursor(entry.Cursor); err == nil && seqnumID != "" {
		writeExportField(&buf, "__SEQNUM_ID", []byte(seqnumID))
	}

	writeExportField(&buf, "_BOOT_ID", []byte(entry.BootID.String()))

	preferred := []string{"_MACHINE_ID", "_HOSTNAME", "PRIORITY", "_TRANSPORT"}
	written := map[string]struct{}{"_BOOT_ID": {}}
	for _, name := range preferred {
		for _, value := range entryValues(entry, name) {
			writeExportField(&buf, name, value)
		}
		written[name] = struct{}{}
	}

	var keys []string
	for _, k := range entryFieldNames(entry) {
		if _, ok := written[k]; ok {
			continue
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)

	for _, k := range keys {
		for _, value := range entryValues(entry, k) {
			writeExportField(&buf, k, value)
		}
	}

	buf.WriteByte('\n')
	return buf.String()
}

func JSONEntry(entry *Entry) (map[string]interface{}, error) {
	result := make(map[string]interface{})
	written := make(map[string]struct{})

	if entry.Cursor != "" {
		addJSONValue(result, "__CURSOR", []byte(entry.Cursor))
		written["__CURSOR"] = struct{}{}
	}
	if entry.Realtime != 0 {
		addJSONValue(result, "__REALTIME_TIMESTAMP", []byte(strconv.FormatUint(entry.Realtime, 10)))
		written["__REALTIME_TIMESTAMP"] = struct{}{}
	}
	if entry.Monotonic != 0 {
		addJSONValue(result, "__MONOTONIC_TIMESTAMP", []byte(strconv.FormatUint(entry.Monotonic, 10)))
		written["__MONOTONIC_TIMESTAMP"] = struct{}{}
	}
	if entry.Seqnum != 0 {
		addJSONValue(result, "__SEQNUM", []byte(strconv.FormatUint(entry.Seqnum, 10)))
		written["__SEQNUM"] = struct{}{}
	}
	if seqnumID, _, _, _, err := ParseCursor(entry.Cursor); err == nil && seqnumID != "" {
		addJSONValue(result, "__SEQNUM_ID", []byte(seqnumID))
		written["__SEQNUM_ID"] = struct{}{}
	}
	addJSONValue(result, "_BOOT_ID", []byte(entry.BootID.String()))
	written["_BOOT_ID"] = struct{}{}

	names := entryFieldNames(entry)
	sort.Strings(names)
	for _, name := range names {
		if _, ok := written[name]; ok {
			continue
		}
		for _, value := range entryValues(entry, name) {
			addJSONValue(result, name, value)
		}
	}

	return result, nil
}

func entryFieldNames(entry *Entry) []string {
	seen := make(map[string]struct{}, len(entry.Fields)+len(entry.FieldValues))
	for name := range entry.Fields {
		seen[name] = struct{}{}
	}
	for name := range entry.FieldValues {
		seen[name] = struct{}{}
	}
	names := make([]string, 0, len(seen))
	for name := range seen {
		names = append(names, name)
	}
	return names
}

func entryValues(entry *Entry, name string) [][]byte {
	if values := entry.FieldValues[name]; len(values) > 0 {
		return values
	}
	if value, ok := entry.Fields[name]; ok {
		return [][]byte{value}
	}
	return nil
}

func writeExportField(buf *bytes.Buffer, name string, value []byte) {
	text := make([]byte, 0, len(name)+1+len(value))
	text = append(text, name...)
	text = append(text, '=')
	text = append(text, value...)

	if journalBytesPrintable(text, false) {
		buf.Write(text)
		buf.WriteByte('\n')
		return
	}

	buf.WriteString(name)
	buf.WriteByte('\n')
	var size [8]byte
	binary.LittleEndian.PutUint64(size[:], uint64(len(value)))
	buf.Write(size[:])
	buf.Write(value)
	buf.WriteByte('\n')
}

func addJSONValue(result map[string]interface{}, name string, value []byte) {
	encoded := jsonFieldValue(value)
	if existing, ok := result[name]; ok {
		if values, ok := existing.([]interface{}); ok {
			result[name] = append(values, encoded)
			return
		}
		result[name] = []interface{}{existing, encoded}
		return
	}
	result[name] = encoded
}

func jsonFieldValue(value []byte) interface{} {
	if journalBytesPrintable(value, true) {
		return string(value)
	}

	values := make([]int, len(value))
	for i, b := range value {
		values[i] = int(b)
	}
	return values
}

func journalBytesPrintable(value []byte, allowNewline bool) bool {
	for len(value) > 0 {
		r, size := utf8.DecodeRune(value)
		if r == utf8.RuneError && size == 1 {
			return false
		}
		if r < ' ' {
			if r == '\t' || (allowNewline && r == '\n') {
				value = value[size:]
				continue
			}
			return false
		}
		if r >= 0x7f && r <= 0x9f {
			return false
		}
		value = value[size:]
	}
	return true
}

func ParseMatchString(s string) ([]byte, error) {
	if s == "" {
		return nil, errors.New("empty match string")
	}
	if s == "=" {
		return nil, errors.New("invalid match: missing field name")
	}
	if strings.HasPrefix(s, "=") {
		return nil, errors.New("invalid match: field name cannot start with =")
	}

	eq := strings.IndexByte(s, '=')
	if eq < 0 {
		return nil, errors.New("invalid match: missing '=' separator")
	}

	field := s[:eq]

	if field == "" {
		return nil, errors.New("invalid match: empty field name")
	}

	if len(field) > 64 {
		return nil, errors.New("invalid match: field name too long")
	}

	if field[0] >= '0' && field[0] <= '9' {
		return nil, fmt.Errorf("invalid field name %q", field)
	}
	for _, c := range field {
		if c == '_' || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			continue
		}
		return nil, fmt.Errorf("invalid field name %q", field)
	}

	return []byte(s), nil
}

func ParseCursor(cursor string) (seqnumID string, bootID string, realtime uint64, seqnum uint64, err error) {
	parts := strings.Split(cursor, ";")
	values := make(map[string]string, len(parts))
	for _, part := range parts {
		key, value, ok := strings.Cut(part, "=")
		if !ok || key == "" {
			return "", "", 0, 0, errors.New("invalid cursor format")
		}
		values[key] = value
	}

	s, okS := values["s"]
	j, okJ := values["j"]
	c, okC := values["c"]
	nstr, okN := values["n"]
	if !okS || !okJ || !okC || !okN {
		return "", "", 0, 0, errors.New("invalid cursor format")
	}
	seqnumID = s
	bootID = j

	realtime, err = strconv.ParseUint(c, 16, 64)
	if err != nil {
		return "", "", 0, 0, errors.New("invalid cursor format: bad realtime")
	}

	seqnumVal, err := strconv.ParseUint(nstr, 10, 64)
	if err != nil {
		return "", "", 0, 0, errors.New("invalid cursor format: bad seqnum")
	}
	seqnum = seqnumVal

	return seqnumID, bootID, realtime, seqnum, nil
}

func formatBootList(boots []BootInfo) string {
	var buf bytes.Buffer
	for _, b := range boots {
		first := time.UnixMicro(int64(b.FirstEntry))
		last := time.UnixMicro(int64(b.LastEntry))
		fmt.Fprintf(&buf, "[%4d] %s %s - %s\n",
			b.Index,
			b.BootID[:8],
			first.Format(time.DateTime),
			last.Format(time.DateTime))
	}
	return buf.String()
}
