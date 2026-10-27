package journal

import (
	"bytes"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/klauspost/compress/zstd"
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
	Fields         map[string][]byte
	FieldValues    map[string][][]byte
	Payloads       [][]byte
	RawFields      []RawField
	RawFieldValues map[string][][]byte
	Seqnum         uint64
	Realtime       uint64
	Monotonic      uint64
	BootID         UUID
	Cursor         string
}

type RawField struct {
	Name  []byte
	Value []byte
}

func (e *Entry) RawValues(name []byte) [][]byte {
	if e == nil || e.RawFieldValues == nil {
		return nil
	}
	return e.RawFieldValues[rawFieldKey(name)]
}

func (e *Entry) Raw(name []byte) ([]byte, bool) {
	values := e.RawValues(name)
	if len(values) == 0 {
		return nil, false
	}
	return values[0], true
}

type ReaderAccessMode int

const (
	// ReaderAccessReadAt selects the rolling positioned-read backend.
	//
	// This mode exists for tests, diagnostics, constrained-platform
	// investigation, and controlled fallback evidence. It is not a production
	// reader mode; production callers should use ReaderAccessAuto or
	// ReaderAccessMmap so supported targets use rolling mmap.
	ReaderAccessReadAt ReaderAccessMode = iota
	// ReaderAccessMmap selects rolling mmap-backed reader windows.
	ReaderAccessMmap
	// ReaderAccessAuto selects rolling mmap where supported and falls back to
	// ReaderAccessReadAt only when mmap setup fails. Treat a read-at fallback
	// in production as a deployment signal to investigate and benchmark.
	ReaderAccessAuto
)

type ReaderBounds int

const (
	ReaderBoundsLive ReaderBounds = iota
	ReaderBoundsSnapshot
)

type ReaderOptions struct {
	AccessMode       ReaderAccessMode
	Bounds           ReaderBounds
	WindowSize       uint64
	MaxWindows       int
	MaxRowArenaBytes uint64
}

func DefaultReaderOptions() ReaderOptions {
	return ReaderOptions{
		AccessMode:       ReaderAccessAuto,
		Bounds:           ReaderBoundsLive,
		WindowSize:       defaultReaderWindowSize,
		MaxWindows:       defaultReaderMaxWindows,
		MaxRowArenaBytes: defaultReaderMaxRowArenaBytes,
	}
}

// WithAccessMode selects the reader byte-access backend.
//
// Production callers should prefer ReaderAccessAuto or ReaderAccessMmap.
// ReaderAccessReadAt is retained for tests, diagnostics, and fallback
// investigation only; it is not the production performance path.
func (o ReaderOptions) WithAccessMode(mode ReaderAccessMode) ReaderOptions {
	o.AccessMode = mode
	return o
}

// WithMmap toggles the reader access backend.
//
// Passing false selects ReaderAccessReadAt, which is not a production reader
// mode. Prefer DefaultReaderOptions or WithAccessMode(ReaderAccessMmap) for
// production readers on supported targets.
func (o ReaderOptions) WithMmap(enabled bool) ReaderOptions {
	if enabled {
		o.AccessMode = ReaderAccessMmap
	} else {
		o.AccessMode = ReaderAccessReadAt
	}
	return o
}

func (o ReaderOptions) WithBounds(bounds ReaderBounds) ReaderOptions {
	o.Bounds = bounds
	return o
}

func (o ReaderOptions) WithWindowSize(size uint64) ReaderOptions {
	o.WindowSize = size
	return o
}

func (o ReaderOptions) WithMaxWindows(maxWindows int) ReaderOptions {
	o.MaxWindows = maxWindows
	return o
}

func (o ReaderOptions) WithMaxRowArenaBytes(size uint64) ReaderOptions {
	o.MaxRowArenaBytes = size
	return o
}

func (o ReaderOptions) WithSnapshot(enabled bool) ReaderOptions {
	if enabled {
		o.Bounds = ReaderBoundsSnapshot
	} else {
		o.Bounds = ReaderBoundsLive
	}
	return o
}

func (o ReaderOptions) normalized() ReaderOptions {
	return normalizeReaderOptions(o)
}

// Reader reads one journal file. A Reader is not safe for concurrent use by
// multiple goroutines; callers should serialize access or open separate readers.
type Reader struct {
	file        *os.File
	header      journalHeader
	options     ReaderOptions
	accessor    readerAccessor
	fileSize    uint64
	arenaEnd    uint64
	entryExtent uint64

	entrySlotHeaderBuffer [objectHeaderSize]byte

	// Cached from immutable per-file layout flags; live refresh updates mutable
	// header counters/tails, but compact vs regular layout cannot change.
	entryItemSizeBytes       uint8
	offsetArrayItemSizeBytes uint8
	dataPayloadOffsetBytes   uint8

	entryOffsets []uint64
	entryIndex   int
	realtimeSeek *uint64

	currentHeader       entryHeader
	currentHeaderOffset uint64
	currentHeaderValid  bool
	entryDataActive     bool
	allowRecovery       bool
	staged              bool

	filter *filterBuilder

	entryDataOffsets      []uint64
	entryDataOffsetsEntry uint64
	entryDataIndex        int

	uniqueField         string
	uniqueHeadOffset    uint64
	uniqueCurrentOffset uint64
	uniqueChain         chainGuard

	recovery            *readerRecoveryState
	directoryOrderDirty *bool
}

type readerRefreshSnapshot struct {
	header   journalHeader
	offsets  []uint64
	index    int
	fileSize uint64
	visible  readerAccessorVisibleSnapshot
	recovery *readerRecoveryState
}

func OpenFile(path string) (*Reader, error) {
	return OpenFileWithOptions(path, DefaultReaderOptions())
}

func OpenFileWithOptions(path string, opts ReaderOptions) (*Reader, error) {
	return openFileWithPolicy(path, opts, true, true)
}

// openFileWithOptions opens a journal file. loadEntries=false is only for
// header and FIELD/DATA-index operations that never traverse ENTRY arrays.
func openFileWithOptions(path string, opts ReaderOptions, loadEntries bool) (*Reader, error) {
	return openFileWithPolicy(path, opts, loadEntries, true)
}

func openFileWithPolicy(path string, opts ReaderOptions, loadEntries, allowRecovery bool) (*Reader, error) {
	opts = opts.normalized()
	f, cleanupPath, err := openJournalFile(path)
	if err != nil {
		return nil, err
	}

	accessor, _, err := newReaderAccessor(f, opts)
	if err != nil {
		_ = closeJournalFile(f, cleanupPath)
		return nil, err
	}

	header, err := readHeaderFromAccessor(accessor)
	if err != nil {
		_ = accessor.close()
		_ = closeJournalFile(f, cleanupPath)
		return nil, err
	}

	const supportedReaderIncompatible = incompatibleKeyedHash | incompatibleCompressedZSTD | incompatibleCompressedXZ | incompatibleCompressedLZ4 | incompatibleCompact
	if header.incompatibleFlags&^supportedReaderIncompatible != 0 {
		_ = accessor.close()
		_ = closeJournalFile(f, cleanupPath)
		return nil, errUnsupportedJournal
	}

	fileSize := accessor.size()
	r := &Reader{
		file:          f,
		header:        header,
		staged:        cleanupPath != "",
		options:       opts,
		accessor:      accessor,
		fileSize:      fileSize,
		allowRecovery: allowRecovery,
	}
	r.configureLayout()

	if loadEntries {
		if err := r.loadEntryArray(); err != nil {
			_ = r.Close()
			return nil, err
		}
	}

	return r, nil
}

func (r *Reader) configureLayout() {
	r.arenaEnd = r.header.headerSize + r.header.arenaSize
	if r.arenaEnd < r.header.headerSize {
		r.arenaEnd = 0
	}
	r.entryExtent = minUint64(r.arenaEnd, r.fileSize)
	if r.header.isCompact() {
		r.entryItemSizeBytes = compactEntryItemSize
		r.offsetArrayItemSizeBytes = compactOffsetArrayItemSize
		r.dataPayloadOffsetBytes = compactDataObjectHeaderSize
		return
	}
	r.entryItemSizeBytes = regularEntryItemSize
	r.offsetArrayItemSizeBytes = regularOffsetArrayItemSize
	r.dataPayloadOffsetBytes = dataObjectHeaderSize
}

func (r *Reader) Close() error {
	var accessErr error
	if r.accessor != nil {
		accessErr = r.accessor.close()
		r.accessor = nil
	}
	var cleanupPath string
	if r.staged {
		cleanupPath = r.file.Name()
	}
	closeErr := closeJournalFile(r.file, cleanupPath)
	return errors.Join(accessErr, closeErr)
}

func openJournalFile(path string) (*os.File, string, error) {
	if !strings.HasSuffix(path, ".zst") {
		f, err := openNativeReaderFile(path)
		return f, "", err
	}

	src, err := openReaderFile(path)
	if err != nil {
		return nil, "", err
	}
	decoder, err := zstd.NewReader(src)
	if err != nil {
		_ = src.Close()
		return nil, "", err
	}
	defer func() {
		decoder.Close()
		_ = src.Close()
	}()

	tmp, err := os.CreateTemp("", "systemd-journal-sdk-*.journal")
	if err != nil {
		return nil, "", err
	}
	cleanupPath := tmp.Name()
	if _, err := io.Copy(tmp, decoder); err != nil {
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

func (h *journalHeader) FileID() UUID {
	return h.fileID
}

func (h *journalHeader) MachineID() UUID {
	return h.machineID
}

func (h *journalHeader) TailEntryBootID() UUID {
	return h.tailEntryBootID
}

func (h *journalHeader) SeqnumID() UUID {
	return h.seqnumID
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

func (h *journalHeader) ArenaSize() uint64 {
	return h.arenaSize
}

func (h *journalHeader) DataHashTableSize() uint64 {
	return h.dataHashTableSize
}

func (h *journalHeader) FieldHashTableSize() uint64 {
	return h.fieldHashTableSize
}

func (h *journalHeader) NObjects() uint64 {
	return h.nObjects
}

func (h *journalHeader) NEntries() uint64 {
	return h.nEntries
}

func (h *journalHeader) HeadEntryRealtime() uint64 {
	return h.headEntryRealtime
}

func (h *journalHeader) TailEntryRealtime() uint64 {
	return h.tailEntryRealtime
}

func (h *journalHeader) TailEntryMonotonic() uint64 {
	return h.tailEntryMonotonic
}

func (h *journalHeader) HeadEntrySeqnum() uint64 {
	return h.headEntrySeqnum
}

func (h *journalHeader) TailEntrySeqnum() uint64 {
	return h.tailEntrySeqnum
}

func (h *journalHeader) NData() uint64 {
	return h.nData
}

func (h *journalHeader) NFields() uint64 {
	return h.nFields
}

func (h *journalHeader) NTags() uint64 {
	return h.nTags
}

func (h *journalHeader) NEntryArrays() uint64 {
	return h.nEntryArrays
}

func (h *journalHeader) DataHashChainDepth() uint64 {
	return h.dataHashChainDepth
}

func (h *journalHeader) FieldHashChainDepth() uint64 {
	return h.fieldHashChainDepth
}

func (r *Reader) readAt(dst []byte, offset uint64) error {
	if r.accessor == nil {
		return errInvalidJournal
	}
	return r.accessor.readAt(dst, offset)
}

func (r *Reader) readSlice(offset, size uint64) ([]byte, error) {
	if r.accessor == nil {
		return nil, errInvalidJournal
	}
	return r.accessor.tempSlice(offset, size)
}

func (r *Reader) readRowSlice(offset, size uint64) ([]byte, error) {
	if r.accessor == nil {
		return nil, errInvalidJournal
	}
	return r.accessor.rowSlice(offset, size)
}

func (r *Reader) rowCopy(src []byte) ([]byte, error) {
	if r.accessor == nil {
		return nil, errInvalidJournal
	}
	return r.accessor.rowCopy(src)
}

func (r *Reader) SelectedAccessMode() ReaderAccessMode {
	if r.accessor == nil {
		return r.options.AccessMode
	}
	return r.accessor.selectedMode()
}

func (r *Reader) AccessStats() ReaderAccessStats {
	if r.accessor == nil {
		return ReaderAccessStats{RequestedAccessMode: r.options.AccessMode}
	}
	return r.accessor.stats()
}

func rawFieldKey(name []byte) string {
	return hex.EncodeToString(name)
}

func splitRawPayload(payload []byte) ([]byte, []byte, bool) {
	eq := bytes.IndexByte(payload, '=')
	if eq < 0 {
		return nil, nil, false
	}
	return payload[:eq], payload[eq+1:], true
}

func cloneBytes(src []byte) []byte {
	return append([]byte(nil), src...)
}

func (r *Reader) clearEntryDataState() {
	if r.entryDataOffsets != nil {
		r.entryDataOffsets = r.entryDataOffsets[:0]
	}
	r.entryDataOffsetsEntry = 0
	r.entryDataIndex = 0
	r.entryDataActive = false
}

func (r *Reader) ClearEntryDataState() {
	r.clearEntryDataState()
}

func (r *Reader) clearCurrentEntryState() {
	r.clearEntryDataState()
	r.currentHeaderValid = false
	if r.accessor != nil {
		_ = r.accessor.clearRow()
	}
}

func (r *Reader) currentEntryOffset() (uint64, error) {
	if uint(r.entryIndex) < uint(len(r.entryOffsets)) {
		return r.entryOffsets[r.entryIndex], nil
	}
	if r.entryIndex == -2 && r.filter != nil && r.filter.overrideOffset != 0 {
		return r.filter.overrideOffset, nil
	}
	return 0, errEndOfEntries
}

func (r *Reader) readCurrentHeader() (journalHeader, uint64, error) {
	if r.accessor == nil {
		return journalHeader{}, 0, errInvalidJournal
	}
	header, _, size, err := r.accessor.refreshVisibleBounds()
	return header, size, err
}

func (r *Reader) Refresh() (bool, error) {
	r.clearRecoveryEOF()
	return r.refreshEntryOffsets()
}

func (r *Reader) refreshEntryOffsets() (bool, error) {
	if r.staged || r.options.Bounds == ReaderBoundsSnapshot {
		return false, nil
	}
	snapshot := r.refreshSnapshot()
	header, size, err := r.readCurrentHeader()
	if err != nil {
		r.restoreRefreshSnapshot(snapshot)
		return false, err
	}
	if r.sameEntryArrayState(header, size) {
		r.header = header
		r.configureLayout()
		r.clampEntryIndex()
		return false, nil
	}

	r.applyRefreshState(header, size)
	if err := r.loadEntryArray(); err != nil {
		r.restoreRefreshSnapshot(snapshot)
		return false, err
	}
	if r.directoryOrderDirty != nil && (snapshot.recovery.Incomplete() || r.recovery.Incomplete()) &&
		(!sameRecoveryState(snapshot.recovery, r.recovery)) {
		*r.directoryOrderDirty = true
	}
	r.entryIndex = snapshot.index
	if snapshot.recovery.Incomplete() && len(r.entryOffsets) > len(snapshot.offsets) && snapshot.index >= len(snapshot.offsets) {
		r.entryIndex = len(snapshot.offsets) - 1
	}
	r.clampEntryIndex()
	if r.filter != nil &&
		(!sameRecoveryState(snapshot.recovery, r.recovery)) {
		r.filter.stale = true
	}
	return header != snapshot.header || size != snapshot.fileSize || !sameRecoveryState(r.recovery, snapshot.recovery) || len(r.entryOffsets) != len(snapshot.offsets), nil
}

func (r *Reader) sameEntryArrayState(header journalHeader, size uint64) bool {
	return !r.recovery.Incomplete() && size == r.fileSize &&
		(r.entryOffsets == nil || uint64(len(r.entryOffsets)) == header.nEntries) &&
		header.nEntries == r.header.nEntries &&
		header.tailEntryArrayOffset == r.header.tailEntryArrayOffset &&
		header.tailEntryArrayNEntries == r.header.tailEntryArrayNEntries
}

func (r *Reader) clampEntryIndex() {
	if r.entryIndex > len(r.entryOffsets) {
		r.entryIndex = len(r.entryOffsets)
	}
}

func (r *Reader) refreshSnapshot() readerRefreshSnapshot {
	return readerRefreshSnapshot{
		header:   r.header,
		offsets:  r.entryOffsets,
		index:    r.entryIndex,
		fileSize: r.fileSize,
		visible:  r.accessor.snapshotVisibleBounds(),
		recovery: r.recovery,
	}
}

func (r *Reader) applyRefreshState(header journalHeader, size uint64) {
	r.header = header
	r.fileSize = size
	r.configureLayout()
}

func (r *Reader) restoreRefreshSnapshot(snapshot readerRefreshSnapshot) {
	r.header = snapshot.header
	r.entryOffsets = snapshot.offsets
	r.entryIndex = snapshot.index
	r.recovery = snapshot.recovery
	if r.accessor != nil {
		physical := r.accessor.snapshotVisibleBounds().fileSize
		if physical < snapshot.fileSize {
			r.currentHeaderValid = false
		}
		snapshot.fileSize = minUint64(snapshot.fileSize, physical)
		snapshot.visible.fileSize = minUint64(snapshot.visible.fileSize, physical)
	}
	r.fileSize = snapshot.fileSize
	r.configureLayout()
	if r.accessor != nil {
		r.accessor.restoreVisibleBounds(snapshot.visible)
	}
}

func (r *Reader) loadEntryArray() error {
	previousRecovery := r.recovery
	r.recovery = nil
	if r.header.entryArrayOffset == 0 {
		r.entryOffsets = []uint64{}
		if r.header.nEntries != 0 {
			if r.allowRecovery {
				return r.retainRecoveredPrefix(nil, RecoveryMissingEntryArray, 0, previousRecovery)
			}
			return errCorruptObject
		}
		return nil
	}

	offsets := []uint64{}
	offset := r.header.entryArrayOffset
	highest := offset
	remaining := r.header.nEntries
	for offset != 0 && remaining > 0 {
		header, chunkOffsets, toRead, err := r.readEntryArrayChunk(offset, remaining)
		if err != nil {
			return r.recoverEntryArrayFailure(offset, remaining, offsets, chunkOffsets, err, previousRecovery)
		}
		offsets = append(offsets, chunkOffsets...)
		remaining -= toRead
		if remaining > 0 && header.nextArrayOffset != 0 {
			if header.nextArrayOffset <= highest {
				if err := rejectArrayRevisit(r, r.header.entryArrayOffset, offset, header.nextArrayOffset, errCorruptObject); err != nil {
					return err
				}
			} else {
				highest = header.nextArrayOffset
			}
		}
		offset = header.nextArrayOffset
	}
	if remaining != 0 {
		if r.allowRecovery {
			return r.retainRecoveredPrefix(offsets, RecoveryMissingEntryArray, offset, previousRecovery)
		}
		return errCorruptObject
	}

	r.entryOffsets = offsets
	r.entryIndex = -1
	return nil
}

func (r *Reader) readEntryArrayChunk(offset uint64, remaining uint64) (offsetArrayHeader, []uint64, uint64, error) {
	header, capacity, err := r.readOffsetArrayHeader(offset)
	if err != nil {
		return offsetArrayHeader{}, nil, 0, err
	}
	toRead := minUint64(remaining, capacity)
	itemSize := r.offsetArrayItemSize()
	start := offset + offsetArrayObjectHeaderSize
	bodySize := header.object.size - offsetArrayObjectHeaderSize
	if start > r.fileSize || bodySize > r.fileSize-start {
		return header, nil, 0, invalidEntryBodyBounds(start, bodySize, r.fileSize)
	}
	// Header capacity bounds this product by the validated object size.
	byteCount := toRead * itemSize
	if byteCount > uint64(int(^uint(0)>>1)) {
		return header, nil, 0, errCorruptObject
	}
	buf := make([]byte, byteCount)
	if err := r.readAt(buf, start); err != nil {
		return header, nil, 0, err
	}
	offsets, err := r.validEntryOffsetsFromArray(buf, itemSize)
	if err != nil {
		return header, offsets, toRead, err
	}
	return header, offsets, toRead, nil
}

func minUint64(a, b uint64) uint64 {
	if a < b {
		return a
	}
	return b
}

func (r *Reader) validEntryOffsetsFromArray(buf []byte, itemSize uint64) ([]uint64, error) {
	offsets := make([]uint64, 0, len(buf)/int(itemSize))
	for pos := 0; pos < len(buf); pos += int(itemSize) {
		off := entryOffsetArrayItem(buf[pos:], itemSize)
		if off == 0 {
			return offsets, readerTailError{0}
		}
		valid, err := r.validEntryObjectOffset(off)
		if err != nil {
			return offsets, err
		}
		if valid {
			offsets = append(offsets, off)
		}
	}
	return offsets, nil
}

func entryOffsetArrayItem(src []byte, itemSize uint64) uint64 {
	if itemSize == compactOffsetArrayItemSize {
		return uint64(binary.LittleEndian.Uint32(src[:compactOffsetArrayItemSize]))
	}
	return binary.LittleEndian.Uint64(src[:regularOffsetArrayItemSize])
}

func (r *Reader) readEntrySlotHeader(offset uint64) ([]byte, error) {
	windowSize := r.options.WindowSize
	if windowSize == 0 {
		windowSize = defaultReaderWindowSize
	}
	if windowSize >= objectHeaderSize && windowSize&(windowSize-1) == 0 &&
		offset&(windowSize-1) <= windowSize-objectHeaderSize {
		return r.readSlice(offset, objectHeaderSize)
	}
	// Crossing temp slices issue a file read even when both chunks are cached.
	header := r.entrySlotHeaderBuffer[:]
	if err := r.readAt(header, offset); err != nil {
		return nil, err
	}
	return header, nil
}

func (r *Reader) validEntryObjectOffset(offset uint64) (bool, error) {
	if offset < r.header.headerSize || offset&7 != 0 || offset > r.arenaEnd || r.arenaEnd-offset < objectHeaderSize {
		return false, errCorruptObject
	}
	headerBuf, err := r.readEntrySlotHeader(offset)
	if err != nil {
		return false, err
	}
	if len(headerBuf) < objectHeaderSize {
		return false, errInvalidJournal
	}
	typ, size := headerBuf[0], binary.LittleEndian.Uint64(headerBuf[8:16])
	if typ != objectTypeEntry {
		return false, invalidEntrySlotHeader(headerBuf, offset)
	}
	if size < entryObjectHeaderSize || size&(r.entryItemSize()-1) != 0 {
		return false, errCorruptObject
	}
	if offset > r.entryExtent || size > r.entryExtent-offset {
		return false, r.invalidEntryExtent(offset, size)
	}
	return true, nil
}

func (r *Reader) readOffsetArrayHeader(offset uint64) (offsetArrayHeader, uint64, error) {
	if offset < r.header.headerSize || offset&7 != 0 || offset > r.arenaEnd || offsetArrayObjectHeaderSize > r.arenaEnd-offset {
		return offsetArrayHeader{}, 0, errCorruptObject
	}
	buf, err := r.readArraySlotHeader(offset)
	if err != nil {
		return offsetArrayHeader{}, 0, err
	}
	header, err := parseOffsetArrayHeader(buf)
	if err != nil {
		return offsetArrayHeader{}, 0, err
	}
	if header.object.typ != objectTypeEntryArray || header.object.size <= offsetArrayObjectHeaderSize {
		return offsetArrayHeader{}, 0, invalidArraySlotHeader(buf, offset)
	}
	itemSize := r.offsetArrayItemSize()
	bodySize := header.object.size - offsetArrayObjectHeaderSize
	if bodySize&(itemSize-1) != 0 {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	if header.object.size > r.arenaEnd-offset {
		return offsetArrayHeader{}, 0, errCorruptObject
	}
	capacity := bodySize >> ((itemSize >> 2) + 1)
	return header, capacity, nil
}

func (r *Reader) Next() error {
	if r.entryIndex == -2 {
		if err := r.normalizeFilteredPosition(DirectionForward); err != nil {
			return err
		}
	}
	r.clearCurrentEntryState()
	if r.realtimeSeek != nil {
		return r.nextFromRealtimeSeek()
	}
	if r.entryIndex < -1 {
		r.entryIndex = -1
	}
	r.entryIndex++

	if r.entryIndex >= len(r.entryOffsets) {
		return r.nextAfterTailRefresh()
	}
	r.clearRecoveryEOF()
	return nil
}

func (r *Reader) nextFromRealtimeSeek() error {
	usec := *r.realtimeSeek
	idx, err := r.firstRealtimeIndexAtOrAfter(usec)
	r.realtimeSeek = nil
	if err != nil {
		return err
	}
	idx, err = r.refreshRealtimeSeekIndex(usec, idx)
	if err != nil {
		return err
	}
	if idx >= len(r.entryOffsets) {
		r.entryIndex = len(r.entryOffsets)
		return errEndOfEntries
	}
	r.entryIndex = idx
	return nil
}

func (r *Reader) refreshRealtimeSeekIndex(usec uint64, idx int) (int, error) {
	if idx < len(r.entryOffsets) {
		return idx, nil
	}
	changed, err := r.refreshEntryOffsets()
	if err != nil || !changed {
		return idx, err
	}
	return r.firstRealtimeIndexAtOrAfter(usec)
}

func (r *Reader) nextAfterTailRefresh() error {
	unchanged, err := r.unchangedRecoveryEOF(DirectionForward)
	if err != nil {
		return err
	}
	if unchanged {
		r.entryIndex = len(r.entryOffsets)
		return errEndOfEntries
	}
	oldLen := len(r.entryOffsets)
	changed, err := r.refreshEntryOffsets()
	if err != nil {
		return err
	}
	if changed && oldLen < len(r.entryOffsets) {
		r.entryIndex = oldLen
		if r.entryIndex < len(r.entryOffsets) {
			r.clearRecoveryEOF()
			return nil
		}
	}
	r.entryIndex = len(r.entryOffsets)
	r.rememberRecoveryEOF(DirectionForward)
	return errEndOfEntries
}

func (r *Reader) Previous() error {
	r.clearRecoveryEOF()
	if r.entryIndex == -2 {
		if err := r.normalizeFilteredPosition(DirectionBackward); err != nil {
			return err
		}
	}
	r.clearCurrentEntryState()
	if r.realtimeSeek != nil {
		idx, err := r.lastRealtimeIndexAtOrBefore(*r.realtimeSeek)
		r.realtimeSeek = nil
		if err != nil {
			return err
		}
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
	r.entryIndex--

	if r.entryIndex < 0 {
		r.entryIndex = -1
		return errStartOfEntries
	}
	return nil
}

func (r *Reader) SeekHead() error {
	r.clearRecoveryEOF()
	r.clearCurrentEntryState()
	r.entryIndex = -1
	r.realtimeSeek = nil
	return nil
}

func (r *Reader) SeekTail() error {
	r.clearRecoveryEOF()
	r.clearCurrentEntryState()
	r.entryIndex = len(r.entryOffsets)
	r.realtimeSeek = nil
	return nil
}

func (r *Reader) SeekRealtimeUsec(usec uint64) error {
	r.clearRecoveryEOF()
	r.clearCurrentEntryState()
	r.entryIndex = -1
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

func (r *Reader) readEntryHeaderAt(offset uint64) (entryHeader, error) {
	if r.currentHeaderValid && r.currentHeaderOffset == offset {
		return r.currentHeader, nil
	}
	entryBuf, err := r.readSlice(offset, entryObjectHeaderSize)
	if err != nil {
		return entryHeader{}, err
	}

	entryHdr, err := parseEntryHeader(entryBuf)
	if err != nil {
		return entryHeader{}, err
	}
	if entryHdr.object.typ != objectTypeEntry {
		return entryHeader{}, errCorruptObject
	}
	if entryHdr.object.size < entryObjectHeaderSize {
		return entryHeader{}, errCorruptObject
	}
	if currentOffset, err := r.currentEntryOffset(); err == nil && currentOffset == offset {
		r.currentHeader = entryHdr
		r.currentHeaderOffset = offset
		r.currentHeaderValid = true
		return r.currentHeader, nil
	}
	return entryHdr, nil
}

func (r *Reader) readEntryDataOffsetsAt(offset uint64, dst []uint64) (entryHeader, []uint64, error) {
	entryHdr, err := r.readEntryHeaderAt(offset)
	if err != nil {
		return entryHeader{}, nil, err
	}
	itemSize := r.entryItemSize()
	if (entryHdr.object.size-entryObjectHeaderSize)%itemSize != 0 {
		return entryHeader{}, nil, errCorruptObject
	}
	nItems := (entryHdr.object.size - entryObjectHeaderSize) / itemSize
	itemsOffset := offset + entryObjectHeaderSize
	itemsSize := nItems * itemSize
	itemsBuf, err := r.readSlice(itemsOffset, itemsSize)
	if err != nil {
		return entryHeader{}, nil, err
	}

	offsets := dst[:0]
	if itemSize == compactEntryItemSize {
		for pos := 0; pos < len(itemsBuf); pos += compactEntryItemSize {
			dataOff := uint64(binary.LittleEndian.Uint32(itemsBuf[pos : pos+compactEntryItemSize]))
			if dataOff != 0 {
				offsets = append(offsets, dataOff)
			}
		}
	} else {
		for pos := 0; pos < len(itemsBuf); pos += regularEntryItemSize {
			// Regular ENTRY items are 16 bytes; only the first 8 bytes are the DATA offset.
			dataOff := binary.LittleEndian.Uint64(itemsBuf[pos : pos+8])
			if dataOff != 0 {
				offsets = append(offsets, dataOff)
			}
		}
	}
	return entryHdr, offsets, nil
}

func (r *Reader) currentEntryDataOffsets() ([]uint64, error) {
	offset, err := r.currentEntryOffset()
	if err != nil {
		return nil, err
	}
	if r.entryDataOffsetsEntry == offset && r.entryDataOffsets != nil {
		return r.entryDataOffsets, nil
	}
	_, offsets, err := r.readEntryDataOffsetsAt(offset, r.entryDataOffsets)
	if err != nil {
		return nil, err
	}
	r.entryDataOffsets = offsets
	r.entryDataOffsetsEntry = offset
	r.entryDataIndex = 0
	return r.entryDataOffsets, nil
}

func (r *Reader) entryRealtimeAtIndex(index int) (uint64, error) {
	if index < 0 || index >= len(r.entryOffsets) {
		return 0, errNotFound
	}
	offset := r.entryOffsets[index]
	hdr, err := r.readEntryHeaderAt(offset)
	if err != nil {
		return 0, err
	}
	return hdr.realtime, nil
}

func (r *Reader) hash(payload []byte) uint64 {
	if r.header.incompatibleFlags&incompatibleKeyedHash != 0 {
		return sipHash24(r.header.fileID, payload)
	}
	return jenkinsHash64(payload)
}

func (r *Reader) entryItemSize() uint64 {
	return uint64(r.entryItemSizeBytes)
}

func (r *Reader) offsetArrayItemSize() uint64 {
	return uint64(r.offsetArrayItemSizeBytes)
}

func (r *Reader) dataPayloadOffset() uint64 {
	return uint64(r.dataPayloadOffsetBytes)
}

func parseEntryHeader(src []byte) (entryHeader, error) {
	if len(src) < entryObjectHeaderSize {
		return entryHeader{}, errInvalidJournal
	}

	objHeader, err := parseObjectHeader(src[0:16])
	if err != nil {
		return entryHeader{}, err
	}

	var hdr entryHeader
	hdr.object = objHeader
	hdr.seqnum = binary.LittleEndian.Uint64(src[16:24])
	hdr.realtime = binary.LittleEndian.Uint64(src[24:32])
	hdr.monotonic = binary.LittleEndian.Uint64(src[32:40])
	copy(hdr.bootID[:], src[40:56])
	hdr.xorHash = binary.LittleEndian.Uint64(src[56:64])

	return hdr, nil
}

func (r *Reader) AddMatch(data []byte) {
	r.clearRecoveryEOF()
	if r.filter == nil {
		r.filter = &filterBuilder{}
	}
	r.filter.addMatch(data)
}

func (r *Reader) AddDisjunction() error {
	r.clearRecoveryEOF()
	if r.filter == nil {
		r.filter = &filterBuilder{}
	}
	r.clearCurrentEntryState()
	return r.filter.addDisjunction(r)
}

func (r *Reader) AddConjunction() error {
	r.clearRecoveryEOF()
	if r.filter == nil {
		r.filter = &filterBuilder{}
	}
	r.clearCurrentEntryState()
	return r.filter.addConjunction(r)
}

func (r *Reader) FlushMatches() {
	r.filter = nil
	_ = r.SeekHead()
}

func (r *Reader) GetRealtimeUsec() (uint64, error) {
	offset, err := r.currentEntryOffset()
	if err != nil {
		return 0, err
	}
	hdr, err := r.readEntryHeaderAt(offset)
	if err != nil {
		return 0, err
	}

	return hdr.realtime, nil
}

func (r *Reader) GetCursor() (string, error) {
	offset, err := r.currentEntryOffset()
	if err != nil {
		return "", err
	}
	hdr, err := r.readEntryHeaderAt(offset)
	if err != nil {
		return "", err
	}

	return r.makeCursor(offset, hdr), nil
}

// SeekCursor positions the reader at the first entry at or after cursor. A
// syntactically valid cursor that is not present leaves the reader at the next
// later entry, matching libsystemd seek-cursor behavior.
func (r *Reader) SeekCursor(cursor string) error {
	want, err := parseCursorLocation(cursor, true)
	if err != nil {
		return ErrInvalidCursor
	}

	if want.realtimeSet {
		if err := r.SeekRealtimeUsec(want.realtime); err != nil {
			return err
		}
	} else if err := r.SeekHead(); err != nil {
		return err
	}

	for {
		ok, err := r.Step()
		if err != nil {
			return err
		}
		if !ok {
			return r.SeekTail()
		}
		current, err := r.GetCursor()
		if err != nil {
			return err
		}
		got, err := parseCursorLocation(current, false)
		if err != nil {
			return err
		}
		if cursorLocationAtOrAfter(got, want) {
			return nil
		}
	}
}

func (r *Reader) TestCursor(cursor string) (bool, error) {
	current, err := r.GetCursor()
	if err != nil {
		if errors.Is(err, errEndOfEntries) {
			return false, ErrEndOfEntries
		}
		return false, err
	}
	got, err := parseCursorLocation(current, false)
	if err != nil {
		return false, err
	}
	want, err := parseCursorLocation(cursor, false)
	if err != nil {
		return false, nil
	}
	return cursorLocationMatches(got, want), nil
}

func (r *Reader) Step() (bool, error) {
	if r.filter != nil {
		return r.stepFiltered(DirectionForward)
	}
	err := r.Next()
	if err == nil {
		return true, nil
	}
	if errors.Is(err, errEndOfEntries) {
		return false, nil
	}
	return err == nil, err
}

func (r *Reader) StepBack() (bool, error) {
	if r.filter != nil {
		return r.stepFiltered(DirectionBackward)
	}
	err := r.Previous()
	if err == nil {
		return true, nil
	}
	if errors.Is(err, errStartOfEntries) {
		return false, nil
	}
	return err == nil, err
}

//go:noinline
func (r *Reader) recoverEntryArrayFailure(offset, remaining uint64, offsets, chunkOffsets []uint64, err error, previousRecovery *readerRecoveryState) error {
	lost, ok := readerTailFailure(err)
	if !ok || !r.allowRecovery {
		return err
	}
	reason := RecoveryMissingEntry
	if lost == offset || lost == offset+offsetArrayObjectHeaderSize {
		reason = RecoveryMissingEntryArray
		if lost != offset {
			chunkOffsets, err = r.recoverPartialArray(offset, remaining)
			if err != nil {
				if _, ok := readerTailFailure(err); !ok {
					return err
				}
			}
		}
	}
	return r.retainRecoveredPrefix(append(offsets, chunkOffsets...), reason, lost, previousRecovery)
}

//go:noinline
func (r *Reader) invalidEntryExtent(offset, size uint64) error {
	if size > r.arenaEnd-offset {
		return errCorruptObject
	}
	return readerBoundsError{offset, size, r.fileSize}
}

//go:noinline
func invalidEntrySlotHeader(header []byte, offset uint64) error {
	if binary.LittleEndian.Uint64(header[:8]) == 0 && binary.LittleEndian.Uint64(header[8:16]) == 0 {
		return readerTailError{offset}
	}
	return errCorruptObject
}

//go:noinline
func invalidEntryBodyBounds(offset, size, fileSize uint64) error {
	return readerBoundsError{offset, size, fileSize}
}

func (r *Reader) readArraySlotHeader(offset uint64) ([]byte, error) {
	windowSize := r.options.WindowSize
	if windowSize == 0 {
		windowSize = defaultReaderWindowSize
	}
	if windowSize >= offsetArrayObjectHeaderSize && windowSize&(windowSize-1) == 0 &&
		offset&(windowSize-1) <= windowSize-offsetArrayObjectHeaderSize {
		return r.readSlice(offset, offsetArrayObjectHeaderSize)
	}
	buf := make([]byte, offsetArrayObjectHeaderSize)
	if err := r.readAt(buf, offset); err != nil {
		return nil, err
	}
	return buf, nil
}

//go:noinline
func invalidArraySlotHeader(header []byte, offset uint64) error {
	if binary.LittleEndian.Uint64(header[:8])|binary.LittleEndian.Uint64(header[8:16])|binary.LittleEndian.Uint64(header[16:24]) == 0 {
		return readerTailError{offset}
	}
	return errInvalidJournal
}
