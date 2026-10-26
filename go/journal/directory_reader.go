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

const directoryUniqueCacheCapacity = 8

// DirectoryReader reads multiple journal files in journal order. A
// DirectoryReader is not safe for concurrent use by multiple goroutines.
type DirectoryReader struct {
	files              []*Reader
	index              int
	filter             *filterBuilder
	realtimeSeek       *uint64
	realtimeSeekBound  *directoryRealtimeSeekBound
	candidates         []*directoryCandidate
	currentKey         *directoryEntryKey
	direction          Direction
	hasDirection       bool
	filteredCandidates bool
	merging            bool
	retryCopies        bool
	bootNewest         map[UUID]directoryBootNewest
	nonOverlapping     bool
	orderDirty         bool
	uniqueCache        map[directoryUniqueCacheKey][][]byte
	uniqueCacheOrder   []directoryUniqueCacheKey
	uniqueState        *directoryUniqueState
	uniqueCacheBuilds  int
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
	paths = dedupeOpenFilePaths(paths)
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

func dedupeOpenFilePaths(paths []string) []string {
	out := make([]string, 0, len(paths))
	seen := make(map[string]struct{}, len(paths))
	for _, path := range paths {
		key := canonicalOpenFileKey(path)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		out = append(out, path)
	}
	return out
}

func canonicalOpenFileKey(path string) string {
	abs, err := filepath.Abs(path)
	if err != nil {
		abs = path
	}
	if resolved, err := filepath.EvalSymlinks(abs); err == nil {
		return resolved
	}
	return abs
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

	dr := &DirectoryReader{
		files:          readers,
		index:          -1,
		candidates:     make([]*directoryCandidate, len(readers)),
		bootNewest:     buildDirectoryBootNewest(readers),
		nonOverlapping: directoryFilesNonOverlapping(readers),
		uniqueCache:    make(map[directoryUniqueCacheKey][][]byte),
	}
	for _, reader := range readers {
		reader.directoryOrderDirty = &dr.orderDirty
	}
	return dr, nil
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

type directoryUniqueCacheKey string

type directoryUniqueState struct {
	key   directoryUniqueCacheKey
	index int
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
		header := r.effectiveHeader()
		bootID := header.tailEntryBootID
		if uuidIsZero(bootID) {
			continue
		}
		current, ok := boots[bootID]
		if !ok || header.tailEntryMonotonic > current.monotonic {
			boots[bootID] = directoryBootNewest{
				machineID: header.machineID,
				monotonic: header.tailEntryMonotonic,
				realtime:  header.tailEntryRealtime,
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
	dr.invalidateFilterCandidates()
	for _, reader := range dr.files {
		reader.AddMatch(data)
		dr.filter = reader.filter
	}
}

func (dr *DirectoryReader) AddDisjunction() error {
	dr.invalidateFilterCandidates()
	for _, reader := range dr.files {
		if err := reader.AddDisjunction(); err != nil {
			return err
		}
		dr.filter = reader.filter
	}
	return nil
}

func (dr *DirectoryReader) AddConjunction() error {
	dr.invalidateFilterCandidates()
	for _, reader := range dr.files {
		if err := reader.AddConjunction(); err != nil {
			return err
		}
		dr.filter = reader.filter
	}
	return nil
}

func (dr *DirectoryReader) FlushMatches() {
	for _, reader := range dr.files {
		reader.FlushMatches()
	}
	dr.filter = nil
	dr.realtimeSeek = nil
	dr.resetMergeState()
}

func (dr *DirectoryReader) GetRealtimeUsec() (uint64, error) {
	if dr.index < 0 || dr.index >= len(dr.files) {
		return 0, errEndOfEntries
	}
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
	if dr.index < 0 || dr.index >= len(dr.files) {
		return "", errEndOfEntries
	}
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

func (dr *DirectoryReader) QueryUnique(fieldName string) ([][]byte, error) {
	var results [][]byte
	err := dr.VisitUnique(fieldName, func(value []byte) error {
		results = append(results, cloneBytes(value))
		return nil
	})
	return results, err
}

func (dr *DirectoryReader) VisitUnique(fieldName string, visit func([]byte) error) error {
	key, err := dr.ensureUniqueCache(fieldName)
	if err != nil {
		return err
	}
	for _, payload := range dr.uniqueCache[key] {
		value, err := stripPayloadFieldValue(fieldName, payload)
		if err != nil {
			return err
		}
		if err := visit(value); err != nil {
			return err
		}
	}
	return nil
}

func (dr *DirectoryReader) QueryUniqueState(fieldName string) error {
	dr.ClearUniqueState()
	key, err := dr.ensureUniqueCache(fieldName)
	if err != nil {
		return err
	}
	dr.uniqueState = &directoryUniqueState{key: key}
	return nil
}

func (dr *DirectoryReader) RestartUniqueState() error {
	if dr.uniqueState != nil {
		dr.uniqueState.index = 0
	}
	return nil
}

func (dr *DirectoryReader) ClearUniqueState() {
	dr.uniqueState = nil
	for _, r := range dr.files {
		r.ClearUniqueState()
	}
}

func (dr *DirectoryReader) EnumerateUniquePayload() ([]byte, bool, error) {
	if dr.uniqueState == nil {
		return nil, false, nil
	}
	payloads := dr.uniqueCache[dr.uniqueState.key]
	if payloads == nil {
		return nil, false, fmt.Errorf("directory unique state references a missing cache entry")
	}
	if dr.uniqueState.index >= len(payloads) {
		return nil, false, nil
	}
	payload := cloneBytes(payloads[dr.uniqueState.index])
	dr.uniqueState.index++
	return payload, true, nil
}

func (dr *DirectoryReader) ensureUniqueCache(fieldName string) (directoryUniqueCacheKey, error) {
	if err := dr.refreshUniqueHeaders(); err != nil {
		return "", err
	}
	key := dr.uniqueCacheKey(fieldName)
	if _, ok := dr.uniqueCache[key]; ok {
		dr.touchUniqueCacheKey(key)
		return key, nil
	}
	payloads, err := dr.buildUniqueCachePayloads(fieldName)
	if err != nil {
		return "", err
	}
	refreshedKey := dr.uniqueCacheKey(fieldName)
	if refreshedKey != key {
		payloads, err = dr.buildUniqueCachePayloads(fieldName)
		if err != nil {
			return "", err
		}
	}
	finalKey := key
	if refreshedKey != key {
		finalKey = refreshedKey
	}
	dr.uniqueCache[finalKey] = payloads
	dr.touchUniqueCacheKey(finalKey)
	dr.enforceUniqueCacheCapacity()
	dr.uniqueCacheBuilds++
	return finalKey, nil
}

func (dr *DirectoryReader) refreshUniqueHeaders() error {
	for _, r := range dr.files {
		if err := r.refreshUniqueHeader(); err != nil {
			return err
		}
	}
	return nil
}

func (dr *DirectoryReader) touchUniqueCacheKey(key directoryUniqueCacheKey) {
	for i, existing := range dr.uniqueCacheOrder {
		if existing == key {
			copy(dr.uniqueCacheOrder[i:], dr.uniqueCacheOrder[i+1:])
			dr.uniqueCacheOrder = dr.uniqueCacheOrder[:len(dr.uniqueCacheOrder)-1]
			break
		}
	}
	dr.uniqueCacheOrder = append(dr.uniqueCacheOrder, key)
}

func (dr *DirectoryReader) enforceUniqueCacheCapacity() {
	var activeKey directoryUniqueCacheKey
	var hasActiveKey bool
	if dr.uniqueState != nil {
		activeKey = dr.uniqueState.key
		hasActiveKey = true
	}
	skippedActive := false
	for len(dr.uniqueCacheOrder) > directoryUniqueCacheCapacity {
		evicted := dr.uniqueCacheOrder[0]
		dr.uniqueCacheOrder = dr.uniqueCacheOrder[1:]
		if hasActiveKey && evicted == activeKey {
			if skippedActive {
				dr.uniqueCacheOrder = append([]directoryUniqueCacheKey{evicted}, dr.uniqueCacheOrder...)
				return
			}
			dr.uniqueCacheOrder = append(dr.uniqueCacheOrder, evicted)
			skippedActive = true
			continue
		}
		delete(dr.uniqueCache, evicted)
	}
}

func (dr *DirectoryReader) buildUniqueCachePayloads(fieldName string) ([][]byte, error) {
	seen := make(map[string]struct{})
	payloads := make([][]byte, 0)
	for _, r := range dr.files {
		err := r.VisitUnique(fieldName, func(value []byte) error {
			key := string(value)
			if _, exists := seen[key]; exists {
				return nil
			}
			seen[key] = struct{}{}
			payloads = append(payloads, payloadFromFieldValue(fieldName, value))
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	return payloads, nil
}

func (dr *DirectoryReader) uniqueCacheKey(fieldName string) directoryUniqueCacheKey {
	var b strings.Builder
	b.WriteString(fieldName)
	for _, r := range dr.files {
		h := r.header
		fmt.Fprintf(
			&b,
			"|%s:%d:%d:%d:%d:%d:%d:%d:%d:%d:%s",
			h.fileID.String(),
			h.nObjects,
			h.nEntries,
			h.nData,
			h.nFields,
			h.headEntrySeqnum,
			h.tailEntrySeqnum,
			h.headEntryRealtime,
			h.tailEntryRealtime,
			h.tailEntryMonotonic,
			h.tailEntryBootID.String(),
		)
		if r.recovery.Incomplete() {
			fmt.Fprintf(&b, ":recovery:%d:%d:%d:%d", r.recovery.Reason, r.recovery.FirstLostOffset, r.recovery.RecoveredEntries, r.fileSize)
		}
	}
	return directoryUniqueCacheKey(b.String())
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
	dr.merging = false
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
	dr.merging = false
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
	dr.merging = false
	value := usec
	dr.realtimeSeek = &value
	dr.realtimeSeekBound = nil
	dr.currentKey = nil
	dr.hasDirection = false
	dr.resetCandidates()
	return nil
}

type directoryReaderPosition struct {
	index    int
	realtime *uint64
	override uint64
}

func (r *Reader) directoryPosition() directoryReaderPosition {
	position := directoryReaderPosition{index: r.entryIndex, realtime: r.realtimeSeek}
	if r.filter != nil {
		position.override = r.filter.overrideOffset
	}
	return position
}

func (r *Reader) restoreDirectoryPosition(position directoryReaderPosition) {
	r.clearCurrentEntryState()
	r.entryIndex, r.realtimeSeek = position.index, position.realtime
	if r.filter != nil {
		r.filter.overrideOffset = position.override
		r.filter.walkValid = false
		r.filter.retry = true
		invalidateFilterWalk(r.filter.expr)
	}
}

func (dr *DirectoryReader) stepMerged(direction Direction, applyFilter bool) (bool, error) {
	if dr.retryCopies {
		// Getters may have cached damaged bytes again before the caller repaired them.
		for _, reader := range dr.files {
			reader.clearCurrentEntryState()
			if reader.accessor != nil {
				reader.accessor.invalidateCopiedWindows()
			}
		}
		dr.retryCopies = false
	}
	index := dr.index
	var selected *Reader
	var before directoryReaderPosition
	if dr.currentKey != nil && index >= 0 && index < len(dr.files) {
		selected = dr.files[index]
		before = selected.directoryPosition()
	} else {
		index = -1
	}
	ok, err := dr.stepMergedInner(direction, applyFilter)
	if err != nil {
		dr.retryCopies = true
		dr.index = index
		if selected != nil {
			selected.restoreDirectoryPosition(before)
			// Recompute only the candidate whose child was restored.
			dr.candidates[index] = nil
		} else if !dr.merging {
			dr.hasDirection = false
		}
	}
	return ok, err
}

func (dr *DirectoryReader) stepMergedInner(direction Direction, applyFilter bool) (bool, error) {
	filtered := applyFilter && dr.filter != nil
	if dr.filteredCandidates != filtered {
		dr.filteredCandidates = filtered
		if !dr.hasSequentialPosition() {
			dr.hasDirection = false
		}
		dr.resetCandidates()
	}
	if dr.canStepSequential(direction, applyFilter) {
		return dr.stepSequential(direction, applyFilter)
	}
	dr.merging = true
retry:
	if err := dr.prepareMergeDirection(direction); err != nil {
		return false, err
	}

	var best *directoryCandidate
	for i := range dr.files {
		if err := dr.fillCandidate(i, direction, applyFilter); err != nil {
			return false, err
		}
		if dr.orderDirty {
			dr.restartRecoveryMerge()
			goto retry
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

func (dr *DirectoryReader) restartRecoveryMerge() {
	dr.orderDirty = false
	dr.bootNewest = buildDirectoryBootNewest(dr.files)
	// A recovery transition invalidates an existing sequential traversal proof.
	dr.nonOverlapping = false
	if dr.currentKey == nil && dr.realtimeSeekBound != nil {
		usec := dr.realtimeSeekBound.usec
		dr.realtimeSeek = &usec
	}
	dr.hasDirection = false
	dr.resetCandidates()
}

func (dr *DirectoryReader) canStepSequential(direction Direction, applyFilter bool) bool {
	if !dr.nonOverlapping || dr.merging || dr.realtimeSeek != nil || !dr.hasDirection && dr.currentKey != nil {
		return false
	}
	if dr.hasDirection && dr.direction != direction && dr.currentKey != nil {
		return false
	}
	return true
}

func (dr *DirectoryReader) stepSequential(direction Direction, applyFilter bool) (bool, error) {
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
		return dr.stepSequentialForward(applyFilter)
	}
	return dr.stepSequentialBackward(applyFilter)
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

func (dr *DirectoryReader) stepSequentialForward(applyFilter bool) (bool, error) {
	if dr.index < 0 {
		dr.index = 0
	}
	for dr.index < len(dr.files) {
		ok, err := dr.stepSequentialReader(DirectionForward, applyFilter)
		if err != nil || ok {
			return ok, err
		}
		dr.index++
	}
	dr.finishSequentialEnd(applyFilter)
	return false, nil
}

func (dr *DirectoryReader) stepSequentialBackward(applyFilter bool) (bool, error) {
	if dr.index >= len(dr.files) {
		dr.index = len(dr.files) - 1
	}
	for dr.index >= 0 {
		ok, err := dr.stepSequentialReader(DirectionBackward, applyFilter)
		if err != nil || ok {
			return ok, err
		}
		dr.index--
	}
	dr.finishSequentialEnd(applyFilter)
	return false, nil
}

func (dr *DirectoryReader) finishSequentialEnd(applyFilter bool) {
	dr.index = -1
	if !applyFilter || dr.filter == nil {
		dr.currentKey = nil
	}
}

func (dr *DirectoryReader) stepSequentialReader(direction Direction, applyFilter bool) (bool, error) {
	r := dr.files[dr.index]
	before := r.directoryPosition()
	ok, err := stepReader(r, direction, applyFilter)
	if err == nil && dr.orderDirty {
		dr.restartRecoveryMerge()
		return dr.stepMergedInner(direction, applyFilter)
	}
	if err != nil || !ok {
		return ok, err
	}
	key, err := r.currentDirectoryEntryKey()
	if err != nil {
		r.restoreDirectoryPosition(before)
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
		before := r.directoryPosition()
		ok, err := stepReader(r, direction, applyFilter)
		if err != nil {
			return err
		}
		if dr.orderDirty {
			return nil
		}
		if !ok {
			return nil
		}

		key, err := r.currentDirectoryEntryKey()
		if err != nil {
			r.restoreDirectoryPosition(before)
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

func stepReader(r *Reader, direction Direction, applyFilter bool) (bool, error) {
	if applyFilter {
		if direction == DirectionForward {
			return r.Step()
		}
		return r.StepBack()
	}
	return stepReaderRaw(r, direction)
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

func (dr *DirectoryReader) invalidateFilterCandidates() {
	if dr.hasSequentialPosition() {
		// Sequential children have no prefetched rows to reposition.
		dr.resetCandidates()
		return
	}
	if dr.currentKey == nil && dr.realtimeSeek == nil && dr.realtimeSeekBound != nil {
		usec := dr.realtimeSeekBound.usec
		dr.realtimeSeek = &usec
	}
	// Revalidate changed matches from the committed key, not prefetched rows.
	dr.merging = dr.currentKey != nil
	dr.hasDirection = false
	dr.realtimeSeekBound = nil
	dr.resetCandidates()
}

func (dr *DirectoryReader) hasSequentialPosition() bool {
	return dr.nonOverlapping && !dr.merging && dr.hasDirection && dr.currentKey != nil && dr.index >= 0 && dr.index < len(dr.files)
}

func (dr *DirectoryReader) resetMergeState() {
	dr.merging = false
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
		header := r.effectiveHeader()
		if r.recovery.Incomplete() && header.nEntries == 0 {
			continue
		}
		bootID := header.tailEntryBootID
		key := bootID.String()

		if entry, ok := bootMap[key]; ok {
			if header.headEntrySeqnum < entry.firstSeq {
				entry.firstSeq = header.headEntrySeqnum
			}
			if header.headEntryRealtime < entry.firstTime {
				entry.firstTime = header.headEntryRealtime
			}
			if header.tailEntrySeqnum > entry.lastSeq {
				entry.lastSeq = header.tailEntrySeqnum
			}
			if header.tailEntryRealtime > entry.lastTime {
				entry.lastTime = header.tailEntryRealtime
			}
		} else {
			bootMap[key] = &bootEntry{
				bootID:    bootID,
				firstSeq:  header.headEntrySeqnum,
				lastSeq:   header.tailEntrySeqnum,
				firstTime: header.headEntryRealtime,
				lastTime:  header.tailEntryRealtime,
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
