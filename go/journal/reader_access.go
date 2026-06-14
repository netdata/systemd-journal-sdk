package journal

import (
	"errors"
	"fmt"
	"io"
	"os"
)

const (
	defaultReaderWindowSize       = 32 * 1024 * 1024
	defaultReaderMaxWindows       = 4
	defaultReaderMaxRowArenaBytes = 256 * 1024 * 1024
	defaultReaderRowArenaSegment  = 4096
)

var errNoEvictableReaderWindow = errors.New("no evictable reader window")

type ReaderAccessStats struct {
	RequestedAccessMode ReaderAccessMode
	SelectedAccessMode  ReaderAccessMode
	FallbackReason      string

	FileSize uint64

	WindowSize uint64
	MaxWindows int

	ActiveWindows    int
	RowPinnedWindows int

	MappedBytes     uint64
	ReadBufferBytes uint64

	MapCount        uint64
	UnmapCount      uint64
	ReadWindowCount uint64
	EvictionCount   uint64

	TempCopyCount uint64
	TempCopyBytes uint64

	RowArenaBytes      uint64
	RowArenaPeakBytes  uint64
	RowArenaLimitBytes uint64
}

type readerAccessorVisibleSnapshot struct {
	fileSize uint64
}

type readerAccessor interface {
	selectedMode() ReaderAccessMode
	size() uint64
	readAt(dst []byte, offset uint64) error
	tempSlice(offset, size uint64) ([]byte, error)
	rowSlice(offset, size uint64) ([]byte, error)
	rowCopy(src []byte) ([]byte, error)
	clearRow() error
	snapshotVisibleBounds() readerAccessorVisibleSnapshot
	restoreVisibleBounds(readerAccessorVisibleSnapshot)
	refreshVisibleBounds() (journalHeader, bool, uint64, error)
	stats() ReaderAccessStats
	close() error
}

type readerWindowBackend interface {
	mode() ReaderAccessMode
	mapWindow(file *os.File, base, size uint64) (*readerAccessWindow, error)
	closeWindow(*readerAccessWindow) error
	refreshFileSize(file *os.File, size uint64) error
	close() error
}

type readerAccessWindow struct {
	base            uint64
	size            uint64
	data            []byte
	mappedData      []byte
	mappedBytes     uint64
	readBufferBytes uint64
	rowPinned       bool
	stale           bool
	lastUsed        uint64
	viewAddr        uintptr
}

type rowArenaSnapshot struct {
	activeSegments int
	lastSegmentLen int
	bytes          uint64
}

type rollingReaderAccessor struct {
	file *os.File

	backend readerWindowBackend

	requestedMode  ReaderAccessMode
	selectedModeID ReaderAccessMode
	fallbackReason string

	fileSize uint64

	windowSize uint64
	maxWindows int

	windows []*readerAccessWindow
	clock   uint64

	scratch []byte

	rowArenaSegments       [][]byte
	rowArenaActiveSegments int
	rowArenaBytes          uint64
	rowArenaCapacityBytes  uint64
	maxRowArenaBytes       uint64

	statsValue ReaderAccessStats
}

func normalizeReaderOptions(opts ReaderOptions) ReaderOptions {
	if opts.AccessMode != ReaderAccessAuto && opts.AccessMode != ReaderAccessMmap {
		opts.AccessMode = ReaderAccessReadAt
	}
	if opts.Bounds != ReaderBoundsSnapshot {
		opts.Bounds = ReaderBoundsLive
	}
	if opts.WindowSize == 0 {
		opts.WindowSize = defaultReaderWindowSize
	}
	if opts.MaxWindows <= 0 {
		opts.MaxWindows = defaultReaderMaxWindows
	}
	if opts.MaxRowArenaBytes == 0 {
		opts.MaxRowArenaBytes = defaultReaderMaxRowArenaBytes
	}
	return opts
}

func newReaderAccessor(file *os.File, opts ReaderOptions) (readerAccessor, ReaderAccessStats, error) {
	opts = normalizeReaderOptions(opts)
	info, err := file.Stat()
	if err != nil {
		return nil, ReaderAccessStats{}, err
	}
	if info.Size() < 0 {
		return nil, ReaderAccessStats{}, fmt.Errorf("%w: negative reader file size", errInvalidJournal)
	}
	fileSize := uint64(info.Size())

	if opts.AccessMode == ReaderAccessReadAt {
		accessor, err := newRollingReaderAccessor(file, opts, newReadAtReaderBackend(), fileSize, "")
		if err != nil {
			return nil, ReaderAccessStats{}, err
		}
		return accessor, accessor.stats(), nil
	}

	mmapBackend, err := newMmapReaderBackend(file)
	if err == nil {
		accessor, mmapErr := newRollingReaderAccessor(file, opts, mmapBackend, fileSize, "")
		if mmapErr == nil {
			if probeErr := accessor.probeInitialWindow(); probeErr == nil {
				return accessor, accessor.stats(), nil
			} else {
				_ = accessor.close()
				err = probeErr
			}
		} else {
			_ = mmapBackend.close()
			err = mmapErr
		}
	}

	if opts.AccessMode == ReaderAccessMmap {
		return nil, ReaderAccessStats{}, err
	}

	fallbackReason := "mmap unavailable"
	if err != nil {
		fallbackReason = err.Error()
	}
	accessor, readAtErr := newRollingReaderAccessor(file, opts, newReadAtReaderBackend(), fileSize, fallbackReason)
	if readAtErr != nil {
		return nil, ReaderAccessStats{}, readAtErr
	}
	return accessor, accessor.stats(), nil
}

func newRollingReaderAccessor(file *os.File, opts ReaderOptions, backend readerWindowBackend, fileSize uint64, fallbackReason string) (*rollingReaderAccessor, error) {
	if opts.WindowSize == 0 || opts.WindowSize > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("%w: invalid reader window size", errInvalidJournal)
	}
	if opts.MaxWindows <= 0 {
		return nil, fmt.Errorf("%w: invalid reader window count", errInvalidJournal)
	}
	if opts.MaxRowArenaBytes > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("%w: reader row arena too large", errInvalidJournal)
	}
	a := &rollingReaderAccessor{
		file:             file,
		backend:          backend,
		requestedMode:    opts.AccessMode,
		selectedModeID:   backend.mode(),
		fallbackReason:   fallbackReason,
		fileSize:         fileSize,
		windowSize:       opts.WindowSize,
		maxWindows:       opts.MaxWindows,
		maxRowArenaBytes: opts.MaxRowArenaBytes,
		windows:          make([]*readerAccessWindow, 0, opts.MaxWindows),
		statsValue:       ReaderAccessStats{},
	}
	a.refreshStats()
	return a, nil
}

func (a *rollingReaderAccessor) probeInitialWindow() error {
	if a.fileSize == 0 {
		return nil
	}
	size := minUint64(a.windowSize, a.fileSize)
	window, err := a.backend.mapWindow(a.file, 0, size)
	if err != nil {
		return err
	}
	a.clock++
	window.lastUsed = a.clock
	a.windows = append(a.windows, window)
	a.recordWindowOpen()
	a.refreshStats()
	return nil
}

func (a *rollingReaderAccessor) selectedMode() ReaderAccessMode {
	return a.selectedModeID
}

func (a *rollingReaderAccessor) size() uint64 {
	return a.fileSize
}

func (a *rollingReaderAccessor) readAt(dst []byte, offset uint64) error {
	if len(dst) == 0 {
		return nil
	}
	remaining := dst
	current := offset
	for len(remaining) > 0 {
		chunk := uint64(len(remaining))
		if chunk > a.windowSize {
			chunk = a.windowSize
		}
		windowBase := a.windowBase(current)
		windowEnd, ok := checkedAdd(windowBase, a.windowSize)
		if !ok {
			return fmt.Errorf("%w: reader request overflows", errInvalidJournal)
		}
		if current+chunk > windowEnd {
			chunk = windowEnd - current
		}
		src, err := a.tempSlice(current, chunk)
		if err != nil {
			return err
		}
		copy(remaining[:int(chunk)], src)
		remaining = remaining[int(chunk):]
		current += chunk
	}
	return nil
}

func (a *rollingReaderAccessor) tempSlice(offset, size uint64) ([]byte, error) {
	if size == 0 {
		return emptySlice(), nil
	}
	if err := a.checkBounds(offset, size); err != nil {
		return nil, err
	}
	if a.rangeFitsWindow(offset, size) {
		window, err := a.getWindow(offset, size, false)
		if err == nil {
			rel := offset - window.base
			return window.data[int(rel):int(rel+size)], nil
		}
		if !errors.Is(err, errNoEvictableReaderWindow) {
			return nil, err
		}
	}
	return a.tempCopy(offset, size)
}

func (a *rollingReaderAccessor) rowSlice(offset, size uint64) ([]byte, error) {
	if size == 0 {
		return emptySlice(), nil
	}
	if err := a.checkBounds(offset, size); err != nil {
		return nil, err
	}
	if a.rangeFitsWindow(offset, size) {
		window, err := a.getWindow(offset, size, true)
		if err == nil {
			rel := offset - window.base
			return window.data[int(rel):int(rel+size)], nil
		}
		if !errors.Is(err, errNoEvictableReaderWindow) {
			return nil, err
		}
	}
	snapshot := a.rowArenaSnapshot()
	buf, err := a.rowAlloc(size)
	if err != nil {
		return nil, err
	}
	if err := readFileAtFull(a.file, buf, offset); err != nil {
		a.restoreRowArenaSnapshot(snapshot)
		a.refreshStats()
		return nil, err
	}
	return buf, nil
}

func (a *rollingReaderAccessor) rowCopy(src []byte) ([]byte, error) {
	if len(src) == 0 {
		return emptySlice(), nil
	}
	dst, err := a.rowAlloc(uint64(len(src)))
	if err != nil {
		return nil, err
	}
	copy(dst, src)
	return dst, nil
}

func (a *rollingReaderAccessor) clearRow() error {
	for _, window := range a.windows {
		window.rowPinned = false
	}
	for i := 0; i < a.rowArenaActiveSegments; i++ {
		a.rowArenaSegments[i] = a.rowArenaSegments[i][:0]
	}
	a.rowArenaActiveSegments = 0
	a.rowArenaBytes = 0
	a.discardStaleUnpinnedWindows()
	a.refreshStats()
	return nil
}

func (a *rollingReaderAccessor) snapshotVisibleBounds() readerAccessorVisibleSnapshot {
	return readerAccessorVisibleSnapshot{fileSize: a.fileSize}
}

func (a *rollingReaderAccessor) restoreVisibleBounds(snapshot readerAccessorVisibleSnapshot) {
	a.fileSize = snapshot.fileSize
	a.refreshStats()
}

func (a *rollingReaderAccessor) refreshVisibleBounds() (journalHeader, bool, uint64, error) {
	oldSize := a.fileSize
	info, err := a.file.Stat()
	if err != nil {
		return journalHeader{}, false, oldSize, err
	}
	if info.Size() < 0 {
		return journalHeader{}, false, oldSize, fmt.Errorf("%w: negative reader file size", errInvalidJournal)
	}
	newSize := uint64(info.Size())
	header, err := readHeaderFromFileAt(a.file, newSize)
	if err != nil {
		return journalHeader{}, false, oldSize, err
	}
	a.fileSize = newSize
	if err := a.backend.refreshFileSize(a.file, newSize); err != nil {
		a.fileSize = oldSize
		a.refreshStats()
		return journalHeader{}, false, oldSize, err
	}
	changed := newSize != oldSize
	if a.selectedModeID == ReaderAccessReadAt {
		a.markWindowsStale()
	}
	a.refreshStats()
	return header, changed, newSize, nil
}

func (a *rollingReaderAccessor) stats() ReaderAccessStats {
	a.refreshStats()
	return a.statsValue
}

func (a *rollingReaderAccessor) close() error {
	var errs []error
	for _, window := range a.windows {
		if err := a.backend.closeWindow(window); err != nil {
			errs = append(errs, err)
		}
		a.recordWindowClose()
	}
	a.windows = nil
	a.rowArenaSegments = nil
	a.rowArenaActiveSegments = 0
	a.rowArenaBytes = 0
	a.rowArenaCapacityBytes = 0
	if err := a.backend.close(); err != nil {
		errs = append(errs, err)
	}
	a.refreshStats()
	return errors.Join(errs...)
}

func (a *rollingReaderAccessor) rangeFitsWindow(offset, size uint64) bool {
	if size > a.windowSize {
		return false
	}
	base := a.windowBase(offset)
	end, ok := checkedAdd(offset, size)
	if !ok {
		return false
	}
	windowEnd, ok := checkedAdd(base, a.windowSize)
	return ok && end <= windowEnd
}

func (a *rollingReaderAccessor) windowBase(offset uint64) uint64 {
	return (offset / a.windowSize) * a.windowSize
}

func (a *rollingReaderAccessor) getWindow(offset, size uint64, rowPinned bool) (*readerAccessWindow, error) {
	end, ok := checkedAdd(offset, size)
	if !ok {
		return nil, fmt.Errorf("%w: reader request overflows", errInvalidJournal)
	}
	for _, window := range a.windows {
		windowEnd := window.base + window.size
		if offset >= window.base && end <= windowEnd {
			if window.stale && (!rowPinned || !window.rowPinned) {
				continue
			}
			a.clock++
			window.lastUsed = a.clock
			if rowPinned {
				window.rowPinned = true
			}
			a.refreshStats()
			return window, nil
		}
	}

	if len(a.windows) >= a.maxWindows {
		if err := a.evictOneWindow(); err != nil {
			return nil, err
		}
	}

	base := a.windowBase(offset)
	mapSize := minUint64(a.windowSize, a.fileSize-base)
	if mapSize == 0 {
		return nil, fmt.Errorf("%w: reader window maps no bytes", errInvalidJournal)
	}
	window, err := a.backend.mapWindow(a.file, base, mapSize)
	if err != nil {
		return nil, err
	}
	a.clock++
	window.lastUsed = a.clock
	window.rowPinned = rowPinned
	a.windows = append(a.windows, window)
	a.recordWindowOpen()
	a.refreshStats()
	return window, nil
}

func (a *rollingReaderAccessor) evictOneWindow() error {
	evictIndex := -1
	var oldest uint64
	for i, window := range a.windows {
		if window.rowPinned {
			continue
		}
		if evictIndex == -1 || window.lastUsed < oldest {
			evictIndex = i
			oldest = window.lastUsed
		}
	}
	if evictIndex < 0 {
		return errNoEvictableReaderWindow
	}
	window := a.windows[evictIndex]
	if err := a.backend.closeWindow(window); err != nil {
		return err
	}
	a.statsValue.EvictionCount++
	a.recordWindowClose()
	copy(a.windows[evictIndex:], a.windows[evictIndex+1:])
	a.windows[len(a.windows)-1] = nil
	a.windows = a.windows[:len(a.windows)-1]
	a.refreshStats()
	return nil
}

func (a *rollingReaderAccessor) markWindowsStale() {
	for _, window := range a.windows {
		window.stale = true
	}
	a.discardStaleUnpinnedWindows()
}

func (a *rollingReaderAccessor) discardStaleUnpinnedWindows() {
	for i := 0; i < len(a.windows); {
		window := a.windows[i]
		if !window.stale || window.rowPinned {
			i++
			continue
		}
		if err := a.backend.closeWindow(window); err == nil {
			a.recordWindowClose()
		}
		copy(a.windows[i:], a.windows[i+1:])
		a.windows[len(a.windows)-1] = nil
		a.windows = a.windows[:len(a.windows)-1]
	}
}

func (a *rollingReaderAccessor) tempCopy(offset, size uint64) ([]byte, error) {
	if size > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("%w: reader request too large", errInvalidJournal)
	}
	if cap(a.scratch) < int(size) {
		a.scratch = make([]byte, int(size))
	}
	buf := a.scratch[:int(size)]
	if err := readFileAtFull(a.file, buf, offset); err != nil {
		return nil, err
	}
	a.statsValue.TempCopyCount++
	a.statsValue.TempCopyBytes += size
	a.refreshStats()
	return buf, nil
}

func (a *rollingReaderAccessor) rowAlloc(size uint64) ([]byte, error) {
	if size > a.maxRowArenaBytes {
		return nil, fmt.Errorf("%w: reader row arena limit exceeded", errInvalidJournal)
	}
	if a.rowArenaBytes > a.maxRowArenaBytes-size {
		return nil, fmt.Errorf("%w: reader row arena limit exceeded", errInvalidJournal)
	}
	if size > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("%w: reader request too large", errInvalidJournal)
	}
	need := int(size)
	if need == 0 {
		return emptySlice(), nil
	}

	if a.rowArenaActiveSegments > 0 {
		last := a.rowArenaSegments[a.rowArenaActiveSegments-1]
		if cap(last)-len(last) >= need {
			start := len(last)
			last = last[:start+need]
			a.rowArenaSegments[a.rowArenaActiveSegments-1] = last
			a.rowArenaBytes += size
			a.updateRowArenaPeak()
			a.refreshStats()
			return last[start:], nil
		}
	}

	segmentIndex, err := a.nextRowArenaSegment(need)
	if err != nil {
		return nil, err
	}
	segment := a.rowArenaSegments[segmentIndex][:need]
	a.rowArenaSegments[segmentIndex] = segment
	a.rowArenaActiveSegments++
	a.rowArenaBytes += size
	a.updateRowArenaPeak()
	a.refreshStats()
	return segment, nil
}

func (a *rollingReaderAccessor) nextRowArenaSegment(need int) (int, error) {
	for i := a.rowArenaActiveSegments; i < len(a.rowArenaSegments); i++ {
		if cap(a.rowArenaSegments[i]) >= need {
			a.rowArenaSegments[a.rowArenaActiveSegments], a.rowArenaSegments[i] = a.rowArenaSegments[i], a.rowArenaSegments[a.rowArenaActiveSegments]
			return a.rowArenaActiveSegments, nil
		}
	}

	if a.rowArenaCapacityBytes > a.maxRowArenaBytes-uint64(need) {
		a.releaseInactiveRowArenaSegments()
	}
	if a.rowArenaCapacityBytes > a.maxRowArenaBytes-uint64(need) {
		return 0, fmt.Errorf("%w: reader row arena limit exceeded", errInvalidJournal)
	}

	segmentCap := maxInt(defaultReaderRowArenaSegment, need)
	remainingCapacity := a.maxRowArenaBytes - a.rowArenaCapacityBytes
	if uint64(segmentCap) > remainingCapacity {
		segmentCap = need
	}
	segment := make([]byte, 0, segmentCap)
	a.rowArenaCapacityBytes += uint64(cap(segment))
	a.rowArenaSegments = append(a.rowArenaSegments, segment)
	return len(a.rowArenaSegments) - 1, nil
}

func (a *rollingReaderAccessor) releaseInactiveRowArenaSegments() {
	for i := a.rowArenaActiveSegments; i < len(a.rowArenaSegments); i++ {
		a.rowArenaCapacityBytes -= uint64(cap(a.rowArenaSegments[i]))
		a.rowArenaSegments[i] = nil
	}
	a.rowArenaSegments = a.rowArenaSegments[:a.rowArenaActiveSegments]
}

func (a *rollingReaderAccessor) rowArenaSnapshot() rowArenaSnapshot {
	snapshot := rowArenaSnapshot{
		activeSegments: a.rowArenaActiveSegments,
		bytes:          a.rowArenaBytes,
	}
	if a.rowArenaActiveSegments > 0 {
		snapshot.lastSegmentLen = len(a.rowArenaSegments[a.rowArenaActiveSegments-1])
	}
	return snapshot
}

func (a *rollingReaderAccessor) restoreRowArenaSnapshot(snapshot rowArenaSnapshot) {
	for i := snapshot.activeSegments; i < a.rowArenaActiveSegments; i++ {
		a.rowArenaSegments[i] = a.rowArenaSegments[i][:0]
	}
	if snapshot.activeSegments > 0 {
		a.rowArenaSegments[snapshot.activeSegments-1] = a.rowArenaSegments[snapshot.activeSegments-1][:snapshot.lastSegmentLen]
	}
	a.rowArenaActiveSegments = snapshot.activeSegments
	a.rowArenaBytes = snapshot.bytes
}

func (a *rollingReaderAccessor) updateRowArenaPeak() {
	if a.rowArenaBytes > a.statsValue.RowArenaPeakBytes {
		a.statsValue.RowArenaPeakBytes = a.rowArenaBytes
	}
}

func (a *rollingReaderAccessor) checkBounds(offset, size uint64) error {
	end, ok := checkedAdd(offset, size)
	if !ok || end > a.fileSize {
		return fmt.Errorf("%w: reader access out of bounds", errInvalidJournal)
	}
	if end > uint64(int(^uint(0)>>1)) && size > uint64(int(^uint(0)>>1)) {
		return fmt.Errorf("%w: reader request too large", errInvalidJournal)
	}
	return nil
}

func (a *rollingReaderAccessor) refreshStats() {
	var mappedBytes uint64
	var readBufferBytes uint64
	var pinned int
	for _, window := range a.windows {
		mappedBytes += window.mappedBytes
		readBufferBytes += window.readBufferBytes
		if window.rowPinned {
			pinned++
		}
	}
	a.statsValue.RequestedAccessMode = a.requestedMode
	a.statsValue.SelectedAccessMode = a.selectedModeID
	a.statsValue.FallbackReason = a.fallbackReason
	a.statsValue.FileSize = a.fileSize
	a.statsValue.WindowSize = a.windowSize
	a.statsValue.MaxWindows = a.maxWindows
	a.statsValue.ActiveWindows = len(a.windows)
	a.statsValue.RowPinnedWindows = pinned
	a.statsValue.MappedBytes = mappedBytes
	a.statsValue.ReadBufferBytes = readBufferBytes
	a.statsValue.RowArenaBytes = a.rowArenaBytes
	a.statsValue.RowArenaLimitBytes = a.maxRowArenaBytes
}

func (a *rollingReaderAccessor) recordWindowOpen() {
	if a.selectedModeID == ReaderAccessReadAt {
		a.statsValue.ReadWindowCount++
		return
	}
	a.statsValue.MapCount++
}

func (a *rollingReaderAccessor) recordWindowClose() {
	if a.selectedModeID == ReaderAccessMmap {
		a.statsValue.UnmapCount++
	}
}

type readAtReaderBackend struct{}

func newReadAtReaderBackend() readAtReaderBackend {
	return readAtReaderBackend{}
}

func (readAtReaderBackend) mode() ReaderAccessMode {
	return ReaderAccessReadAt
}

func (readAtReaderBackend) mapWindow(file *os.File, base, size uint64) (*readerAccessWindow, error) {
	if size > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("%w: reader window too large", errInvalidJournal)
	}
	data := make([]byte, int(size))
	if err := readFileAtFull(file, data, base); err != nil {
		return nil, err
	}
	return &readerAccessWindow{
		base:            base,
		size:            size,
		data:            data,
		readBufferBytes: size,
	}, nil
}

func (readAtReaderBackend) closeWindow(*readerAccessWindow) error {
	return nil
}

func (readAtReaderBackend) refreshFileSize(*os.File, uint64) error {
	return nil
}

func (readAtReaderBackend) close() error {
	return nil
}

func readHeaderFromAccessor(accessor readerAccessor) (journalHeader, error) {
	size := minUint64(headerSize, accessor.size())
	if size < headerMinSize {
		return journalHeader{}, errInvalidJournal
	}
	buf, err := accessor.tempSlice(0, size)
	if err != nil {
		return journalHeader{}, err
	}
	return parseHeader(buf)
}

func readHeaderFromFileAt(file *os.File, fileSize uint64) (journalHeader, error) {
	size := minUint64(headerSize, fileSize)
	if size < headerMinSize {
		return journalHeader{}, errInvalidJournal
	}
	buf := make([]byte, int(size))
	if err := readFileAtFull(file, buf, 0); err != nil {
		return journalHeader{}, err
	}
	return parseHeader(buf)
}

func readFileAtFull(file *os.File, dst []byte, offset uint64) error {
	if len(dst) == 0 {
		return nil
	}
	if offset > uint64(int64(^uint64(0)>>1)) {
		return fmt.Errorf("%w: reader offset too large", errInvalidJournal)
	}
	n, err := file.ReadAt(dst, int64(offset))
	if err != nil && !(errors.Is(err, io.EOF) && n == len(dst)) {
		return err
	}
	if n != len(dst) {
		return io.ErrUnexpectedEOF
	}
	return nil
}

func emptySlice() []byte {
	return []byte{}
}
