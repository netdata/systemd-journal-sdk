package journal

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// DirectoryReader reads multiple journal files in journal order. A
// DirectoryReader is not safe for concurrent use by multiple goroutines.
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
	nonOverlapping    bool
}

func OpenDirectory(path string) (*DirectoryReader, error) {
	return OpenDirectoryWithOptions(path, DefaultReaderOptions())
}

func OpenDirectoryWithOptions(path string, opts ReaderOptions) (*DirectoryReader, error) {
	paths, err := collectJournalFiles(path)
	if err != nil {
		return nil, err
	}

	var readers []*Reader
	for _, filePath := range paths {
		r, err := OpenFileWithOptions(filePath, opts)
		if err != nil {
			continue
		}
		readers = append(readers, r)
	}

	return newDirectoryReader(readers, true)
}

func OpenFiles(paths []string) (*DirectoryReader, error) {
	return OpenFilesWithOptions(paths, DefaultReaderOptions())
}

func OpenFilesWithOptions(paths []string, opts ReaderOptions) (*DirectoryReader, error) {
	readers := make([]*Reader, 0, len(paths))
	for _, path := range paths {
		if !isJournalFileName(path) {
			return nil, fmt.Errorf("not a journal file: %s", path)
		}
		r, err := OpenFileWithOptions(path, opts)
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
		files:          readers,
		index:          -1,
		candidates:     make([]*directoryCandidate, len(readers)),
		bootNewest:     buildDirectoryBootNewest(readers),
		nonOverlapping: directoryFilesNonOverlapping(readers),
	}, nil
}

func directoryFilesNonOverlapping(readers []*Reader) bool {
	for i := 1; i < len(readers); i++ {
		prev := readers[i-1].header
		next := readers[i].header
		if prev.seqnumID != next.seqnumID ||
			prev.tailEntrySeqnum == 0 ||
			next.headEntrySeqnum == 0 ||
			prev.tailEntrySeqnum >= next.headEntrySeqnum ||
			prev.tailEntryRealtime == 0 ||
			next.headEntryRealtime == 0 ||
			prev.tailEntryRealtime >= next.headEntryRealtime {
			return false
		}
	}
	return len(readers) > 0
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

// VisitEntryPayloads calls visitor for each current DATA payload as FIELD=value
// bytes. Payloads are callback-scoped and must not be retained or mutated after
// the visitor returns; use EnumerateEntryPayload when row-level lifetime is
// required.
func (dr *DirectoryReader) VisitEntryPayloads(visitor func([]byte) error) error {
	r, err := dr.currentReader()
	if err != nil {
		return err
	}
	return r.VisitEntryPayloads(visitor)
}

func (dr *DirectoryReader) CollectEntryPayloads() ([][]byte, error) {
	r, err := dr.currentReader()
	if err != nil {
		return nil, err
	}
	return r.CollectEntryPayloads()
}

func (dr *DirectoryReader) GetEntryPayload(fieldName []byte) ([]byte, bool, error) {
	r, err := dr.currentReader()
	if err != nil {
		return nil, false, err
	}
	return r.GetEntryPayload(fieldName)
}

func (dr *DirectoryReader) GetRaw(fieldName []byte) ([]byte, bool, error) {
	r, err := dr.currentReader()
	if err != nil {
		return nil, false, err
	}
	return r.GetRaw(fieldName)
}

func (dr *DirectoryReader) GetRawValues(fieldName []byte) ([][]byte, error) {
	r, err := dr.currentReader()
	if err != nil {
		return nil, err
	}
	return r.GetRawValues(fieldName)
}

func (dr *DirectoryReader) EntryDataRestart() error {
	r, err := dr.currentReader()
	if err != nil {
		return err
	}
	return r.EntryDataRestart()
}

func (dr *DirectoryReader) EnumerateEntryPayload() ([]byte, bool, error) {
	r, err := dr.currentReader()
	if err != nil {
		return nil, false, err
	}
	return r.EnumerateEntryPayload()
}

func (dr *DirectoryReader) ClearEntryDataState() {
	if r, err := dr.currentReader(); err == nil {
		r.ClearEntryDataState()
	}
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
	if dr.currentKey != nil {
		return dr.currentKey.realtime, nil
	}
	r, err := dr.currentReader()
	if err != nil {
		return 0, err
	}
	return r.GetRealtimeUsec()
}

func (dr *DirectoryReader) GetCursor() (string, error) {
	if dr.currentKey != nil {
		return formatCursorFromDirectoryKey(*dr.currentKey), nil
	}
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
	currentSeqnumID, currentBootID, currentRealtime, currentSeqnum, err := ParseCursor(current)
	if err != nil {
		return false, err
	}
	wantSeqnumID, wantBootID, wantRealtime, wantSeqnum, err := ParseCursor(cursor)
	if err != nil {
		return false, nil
	}
	return currentSeqnumID == wantSeqnumID &&
		currentBootID == wantBootID &&
		currentRealtime == wantRealtime &&
		currentSeqnum == wantSeqnum, nil
}

func (dr *DirectoryReader) QueryUnique(fieldName string) ([][]byte, error) {
	var results [][]byte
	err := dr.VisitUnique(fieldName, func(value []byte) error {
		results = append(results, cloneBytes(value))
		return nil
	})
	return results, err
}

func (dr *DirectoryReader) VisitUnique(fieldName string, visit func([]byte) error) error {
	if len(dr.files) == 1 {
		return dr.files[0].VisitUnique(fieldName, visit)
	}

	unique := make(map[string]struct{})
	for _, r := range dr.files {
		err := r.VisitUnique(fieldName, func(value []byte) error {
			key := string(value)
			if _, exists := unique[key]; !exists {
				unique[key] = struct{}{}
				return visit(value)
			}
			return nil
		})
		if err != nil {
			return err
		}
	}

	return nil
}

func (dr *DirectoryReader) EnumerateFields() (map[string]struct{}, error) {
	fields := make(map[string]struct{})

	for _, r := range dr.files {
		rFields, err := r.EnumerateFields()
		if err != nil {
			return nil, err
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
	if dr.canStepSequential(direction, applyFilter) {
		return dr.stepSequential(direction)
	}
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

func (dr *DirectoryReader) canStepSequential(direction Direction, applyFilter bool) bool {
	if !dr.nonOverlapping || dr.realtimeSeek != nil || applyFilter && dr.filter != nil {
		return false
	}
	if dr.hasDirection && dr.direction != direction && dr.currentKey != nil {
		return false
	}
	return true
}

func (dr *DirectoryReader) stepSequential(direction Direction) (bool, error) {
	if len(dr.files) == 0 {
		dr.clearDirectoryPosition()
		return false, nil
	}
	if !dr.hasDirection || dr.direction != direction {
		if err := dr.resetSequentialDirection(direction); err != nil {
			return false, err
		}
	}
	if direction == DirectionForward {
		return dr.stepSequentialForward()
	}
	return dr.stepSequentialBackward()
}

func (dr *DirectoryReader) clearDirectoryPosition() {
	dr.index = -1
	dr.currentKey = nil
}

func (dr *DirectoryReader) resetSequentialDirection(direction Direction) error {
	for _, r := range dr.files {
		if err := seekReaderBoundary(r, direction); err != nil {
			return err
		}
	}
	if direction == DirectionForward {
		dr.index = 0
	} else {
		dr.index = len(dr.files) - 1
	}
	dr.resetCandidates()
	dr.currentKey = nil
	dr.realtimeSeekBound = nil
	dr.direction = direction
	dr.hasDirection = true
	return nil
}

func seekReaderBoundary(r *Reader, direction Direction) error {
	if direction == DirectionForward {
		return r.SeekHead()
	}
	return r.SeekTail()
}

func (dr *DirectoryReader) stepSequentialForward() (bool, error) {
	if dr.index < 0 {
		dr.index = 0
	}
	for dr.index < len(dr.files) {
		ok, err := dr.stepSequentialReader(DirectionForward)
		if err != nil || ok {
			return ok, err
		}
		dr.index++
	}
	dr.clearDirectoryPosition()
	return false, nil
}

func (dr *DirectoryReader) stepSequentialBackward() (bool, error) {
	if dr.index >= len(dr.files) {
		dr.index = len(dr.files) - 1
	}
	for dr.index >= 0 {
		ok, err := dr.stepSequentialReader(DirectionBackward)
		if err != nil || ok {
			return ok, err
		}
		dr.index--
	}
	dr.clearDirectoryPosition()
	return false, nil
}

func (dr *DirectoryReader) stepSequentialReader(direction Direction) (bool, error) {
	ok, err := stepReaderRaw(dr.files[dr.index], direction)
	if err != nil || !ok {
		return ok, err
	}
	key, err := dr.files[dr.index].currentDirectoryEntryKey()
	if err != nil {
		return false, err
	}
	dr.currentKey = &key
	return true, nil
}

func (dr *DirectoryReader) prepareMergeDirection(direction Direction) error {
	if len(dr.files) == 0 {
		return nil
	}

	if dr.realtimeSeek != nil {
		usec := *dr.realtimeSeek
		dr.realtimeSeek = nil
		if err := dr.seekAllRealtime(usec); err != nil {
			return err
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
		if err := dr.seekAllRealtime(dr.currentKey.realtime); err != nil {
			return err
		}
	} else if err := dr.seekAllBoundary(direction); err != nil {
		return err
	}

	dr.resetCandidates()
	dr.direction = direction
	dr.hasDirection = true
	return nil
}

func (dr *DirectoryReader) seekAllRealtime(usec uint64) error {
	for _, r := range dr.files {
		if err := r.SeekRealtimeUsec(usec); err != nil {
			return err
		}
	}
	return nil
}

func (dr *DirectoryReader) seekAllBoundary(direction Direction) error {
	for _, r := range dr.files {
		if err := seekReaderBoundary(r, direction); err != nil {
			return err
		}
	}
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

		matches, err := dr.readerEntryMatches(r, applyFilter)
		if err != nil {
			return err
		}
		if !matches {
			continue
		}

		key, err := r.currentDirectoryEntryKey()
		if err != nil {
			return err
		}
		if !dr.keyPassesRealtimeBound(key) {
			continue
		}
		if !dr.keyPassesCurrentPosition(key, direction) {
			continue
		}

		dr.candidates[readerIndex] = &directoryCandidate{
			readerIndex: readerIndex,
			key:         key,
		}
		return nil
	}
}

func (dr *DirectoryReader) readerEntryMatches(r *Reader, applyFilter bool) (bool, error) {
	if !applyFilter || dr.filter == nil {
		return true, nil
	}
	entry, err := r.GetEntry()
	if err != nil {
		return false, err
	}
	return dr.filter.matches(entry), nil
}

func (dr *DirectoryReader) keyPassesRealtimeBound(key directoryEntryKey) bool {
	if dr.realtimeSeekBound == nil {
		return true
	}
	bound := dr.realtimeSeekBound
	if bound.direction == DirectionForward {
		return key.realtime >= bound.usec
	}
	return key.realtime <= bound.usec
}

func (dr *DirectoryReader) keyPassesCurrentPosition(key directoryEntryKey, direction Direction) bool {
	if dr.currentKey == nil {
		return true
	}
	cmp := dr.compareEntryKeys(key, *dr.currentKey)
	if direction == DirectionForward {
		return cmp > 0
	}
	return cmp < 0
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
	offset, err := r.currentEntryOffset()
	if err != nil {
		return directoryEntryKey{}, err
	}
	hdr, err := r.readEntryHeaderAt(offset)
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
	if sameDirectoryEntryKey(a, b) {
		return 0
	}

	if cmp := compareSharedSeqnum(a, b); cmp != 0 {
		return cmp
	}
	if cmp := dr.compareBootAndMonotonic(a, b); cmp != 0 {
		return cmp
	}
	if cmp := cmpUint64(a.realtime, b.realtime); cmp != 0 {
		return cmp
	}
	return cmpUint64(a.xorHash, b.xorHash)
}

func sameDirectoryEntryKey(a, b directoryEntryKey) bool {
	return a.bootID == b.bootID &&
		a.monotonic == b.monotonic &&
		a.realtime == b.realtime &&
		a.xorHash == b.xorHash &&
		a.seqnumID == b.seqnumID &&
		a.seqnum == b.seqnum
}

func compareSharedSeqnum(a, b directoryEntryKey) int {
	if a.seqnumID != b.seqnumID {
		return 0
	}
	return cmpUint64(a.seqnum, b.seqnum)
}

func (dr *DirectoryReader) compareBootAndMonotonic(a, b directoryEntryKey) int {
	if a.bootID != b.bootID {
		return dr.compareBootIDs(a.bootID, b.bootID)
	}
	return cmpUint64(a.monotonic, b.monotonic)
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
