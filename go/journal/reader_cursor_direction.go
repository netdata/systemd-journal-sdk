package journal

import "sort"

func parseDirectionalCursor(cursor string) (parsedCursorLocation, error) {
	want, err := parseCursorLocation(cursor, true)
	if err != nil {
		return want, err
	}
	for _, id := range []string{want.seqnumID, want.bootID} {
		if id == "" {
			continue
		}
		if len(id) != 32 {
			return want, ErrInvalidCursor
		}
		for _, c := range id {
			if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
				return want, ErrInvalidCursor
			}
		}
	}
	want.seqnumSet = want.seqnumSet && want.seqnumID != ""
	want.monotonicSet = want.monotonicSet && want.bootID != ""
	return want, nil
}

// SeekCursorDirection selects the first entry at or beyond cursor in direction.
// Existing SeekCursor retains its original forward-positioning behavior.
func (r *Reader) SeekCursorDirection(cursor string, direction Direction) error {
	want, err := parseDirectionalCursor(cursor)
	if err != nil {
		return ErrInvalidCursor
	}
	if direction != DirectionForward && direction != DirectionBackward {
		return ErrUnsupported
	}
	return r.seekCursorDirection(want, direction)
}

func (r *Reader) seekCursorDirection(want parsedCursorLocation, direction Direction) error {
	if r.accessor == nil {
		return errInvalidJournal
	}
	r.clearRecoveryEOF()
	r.clearCurrentEntryState()
	if r.filter != nil && r.filter.retry {
		r.accessor.invalidateCopiedWindows()
		r.filter.retry = false
	}
	for {
		before := r.RecoveryStatus()
		var offset uint64
		var err error
		if r.filter != nil {
			if err = r.filter.build(r); err == nil {
				offset, err = seekFilterCursor(r.filter.expr, r, want, direction)
			}
		} else {
			offset, err = r.cursorInitialOffset(nil, want, direction)
		}
		var hdr entryHeader
		if err == nil && offset != 0 {
			hdr, err = r.readEntryHeaderAt(offset)
			if err == nil {
				err = r.validateFilteredEntryHeader(offset, hdr)
			}
		}
		if err != nil {
			if filterTailFailure(err) {
				changed, recoveryErr := r.refreshFilterRecovery()
				if recoveryErr != nil {
					return recoveryErr
				}
				if changed {
					continue
				}
			}
			if r.filter != nil {
				r.filter.walkValid, r.filter.retry = false, true
			}
			return err
		}
		if before != r.RecoveryStatus() {
			continue
		}
		if offset == 0 {
			if direction == DirectionForward {
				return r.SeekTail()
			}
			return r.SeekHead()
		}
		index := sort.Search(len(r.entryOffsets), func(i int) bool { return r.entryOffsets[i] >= offset })
		if index >= len(r.entryOffsets) || r.entryOffsets[index] != offset {
			if r.filter == nil {
				return errCorruptObject
			}
			index = -2
		}
		r.entryIndex, r.realtimeSeek = index, nil
		r.currentHeader, r.currentHeaderOffset, r.currentHeaderValid = hdr, offset, true
		if r.filter != nil {
			f := r.filter
			f.overrideOffset, f.walkOffset, f.walkValid = offset, offset, true
			f.backward, f.retry = direction == DirectionBackward, false
		}
		return nil
	}
}

type cursorMetadataKey uint8

const (
	cursorSequence cursorMetadataKey = iota
	cursorRealtime
	cursorMonotonic
	cursorOffset
)

func (r *Reader) cursorKey(offset uint64, key cursorMetadataKey) (uint64, error) {
	if key == cursorOffset {
		return offset, nil
	}
	header, err := r.readEntryHeaderAt(offset)
	if err != nil {
		return 0, err
	}
	switch key {
	case cursorSequence:
		return header.seqnum, nil
	case cursorMonotonic:
		return header.monotonic, nil
	default:
		return header.realtime, nil
	}
}

func (r *Reader) cursorBound(posting *filterPostingCursor, key cursorMetadataKey, needle uint64, direction Direction) (uint64, error) {
	if posting != nil {
		return posting.seekCursorKey(r, key, needle, direction)
	}
	if uint64(len(r.entryOffsets)) != r.effectiveHeader().nEntries {
		if err := r.refreshFilterOffsets(); err != nil {
			return 0, err
		}
	}
	left, right := 0, len(r.entryOffsets)
	for left < right {
		mid := left + (right-left)/2
		value, err := r.cursorKey(r.entryOffsets[mid], key)
		if err != nil {
			return 0, err
		}
		if value < needle || direction == DirectionBackward && value == needle {
			left = mid + 1
		} else {
			right = mid
		}
	}
	if direction == DirectionBackward {
		left--
	}
	if left < 0 || left >= len(r.entryOffsets) {
		return 0, nil
	}
	return r.entryOffsets[left], nil
}

func (r *Reader) cursorBootPosting(boot string) (*filterPostingCursor, error) {
	offset, found, err := r.findDataOffsetByPayload([]byte("_BOOT_ID=" + boot))
	if err != nil || !found {
		return nil, err
	}
	posting, err := r.newFilterPostingCursor(offset)
	return &posting, err
}

func (r *Reader) cursorMonotonicOffset(posting *filterPostingCursor, boot string, needle uint64, direction Direction) (uint64, error) {
	boots, err := r.cursorBootPosting(boot)
	if err != nil || boots == nil {
		return 0, err
	}
	offset, err := r.cursorBound(boots, cursorMonotonic, needle, direction)
	if err != nil || offset == 0 || posting == nil {
		return offset, err
	}
	for {
		candidate, err := posting.seekRealtime(r, offset, direction)
		if err != nil || candidate == 0 {
			return 0, err
		}
		if candidate == offset {
			return offset, nil
		}
		offset, err = boots.seekRealtime(r, candidate, direction)
		if err != nil || offset == 0 {
			return 0, err
		}
		if offset == candidate {
			return offset, nil
		}
	}
}

func (r *Reader) cursorAdjacentBoot(posting *filterPostingCursor, boot string, direction Direction) (uint64, error) {
	var previous uint64
	for {
		boots, err := r.cursorBootPosting(boot)
		if err != nil || boots == nil {
			return 0, err
		}
		boundary, needle, opposite := uint64(0), uint64(0), DirectionForward
		if direction == DirectionForward {
			needle, opposite = ^uint64(0), DirectionBackward
		}
		boundary, err = boots.seekRealtime(r, needle, opposite)
		if err != nil || boundary == 0 {
			return 0, err
		}
		if direction == DirectionForward {
			if boundary == ^uint64(0) {
				return 0, nil
			}
			boundary++
		} else {
			boundary--
		}
		offset, err := r.cursorBound(nil, cursorOffset, boundary, direction)
		if err != nil || offset == 0 || posting == nil {
			return offset, err
		}
		if previous != 0 && (direction == DirectionForward && offset <= previous || direction == DirectionBackward && offset >= previous) {
			return 0, errCorruptObject
		}
		previous = offset
		header, err := r.readEntryHeaderAt(offset)
		if err != nil {
			return 0, err
		}
		boot = header.bootID.String()
		needle = 0
		if direction == DirectionBackward {
			needle = ^uint64(0)
		}
		candidate, err := r.cursorMonotonicOffset(posting, boot, needle, direction)
		if err != nil || candidate != 0 {
			return candidate, err
		}
	}
}

func (r *Reader) cursorInitialOffset(posting *filterPostingCursor, want parsedCursorLocation, direction Direction) (uint64, error) {
	if want.seqnumSet && want.seqnumID == r.header.seqnumID.String() {
		return r.cursorBound(posting, cursorSequence, want.seqnum, direction)
	}
	if want.monotonicSet {
		offset, err := r.cursorMonotonicOffset(posting, want.bootID, want.monotonic, direction)
		if err != nil || offset != 0 {
			return offset, err
		}
	}
	if want.realtimeSet {
		return r.cursorBound(posting, cursorRealtime, want.realtime, direction)
	}
	if want.monotonicSet {
		return r.cursorAdjacentBoot(posting, want.bootID, direction)
	}
	needle := uint64(0)
	if direction == DirectionBackward {
		needle = ^uint64(0)
	}
	return r.cursorBound(posting, cursorOffset, needle, direction)
}

func seekFilterCursor(expr filterExpr, r *Reader, want parsedCursorLocation, direction Direction) (uint64, error) {
	switch value := expr.(type) {
	case *matchExpr:
		return r.cursorInitialOffset(&value.posting, want, direction)
	case falseExpr:
		return 0, nil
	case orExpr:
		var best uint64
		for _, child := range value {
			offset, err := seekFilterCursor(child, r, want, direction)
			if err != nil {
				return 0, err
			}
			if offset != 0 && (best == 0 || direction == DirectionForward && offset < best || direction == DirectionBackward && offset > best) {
				best = offset
			}
		}
		return best, nil
	case andExpr:
		var best uint64
		for _, child := range value {
			offset, err := seekFilterCursor(child, r, want, direction)
			if err != nil || offset == 0 {
				return 0, err
			}
			if best == 0 || direction == DirectionForward && offset > best || direction == DirectionBackward && offset < best {
				best = offset
			}
		}
		if best == 0 {
			return 0, nil
		}
		return value.seek(r, best, direction)
	default:
		return 0, ErrMalformedFilter
	}
}

func (p *filterPostingCursor) seekCursorKey(r *Reader, key cursorMetadataKey, needle uint64, direction Direction) (uint64, error) {
	p.node, p.slot, p.value = -1, 0, p.inline
	if p.count == 0 {
		return 0, nil
	}
	inline, err := r.cursorKey(p.inline, key)
	if err != nil {
		return 0, err
	}
	if direction == DirectionForward && inline >= needle {
		return p.inline, nil
	}
	if direction == DirectionBackward && inline > needle {
		return 0, nil
	}
	if p.count == 1 {
		if direction == DirectionBackward {
			return p.inline, nil
		}
		return 0, nil
	}
	for index := 0; ; index++ {
		if index == len(p.nodes) {
			if err := p.appendNode(r); err != nil {
				return 0, err
			}
		}
		node := p.nodes[index]
		left, right := uint64(0), node.length
		for left < right {
			mid := left + (right-left)/2
			value, err := p.readSlot(r, index, mid)
			if err != nil {
				return 0, err
			}
			bound, err := r.cursorKey(value, key)
			if err != nil {
				return 0, err
			}
			if bound < needle || direction == DirectionBackward && bound == needle {
				left = mid + 1
			} else {
				right = mid
			}
		}
		if direction == DirectionBackward && left == 0 {
			return p.value, nil
		}
		slot := left
		if direction == DirectionBackward || left == node.length {
			slot--
		}
		value, err := p.readSlot(r, index, slot)
		if err != nil {
			return 0, err
		}
		// Retain the last visited posting even when a sibling wins an OR query.
		p.node, p.slot, p.value = index, slot, value
		if left < node.length {
			return value, nil
		}
		if node.remaining == node.length {
			if direction == DirectionBackward {
				return value, nil
			}
			return 0, nil
		}
	}
}

// SeekCursorDirection positions a merged reader using each file's native index.
func (dr *DirectoryReader) SeekCursorDirection(cursor string, direction Direction) error {
	want, err := parseDirectionalCursor(cursor)
	if err != nil {
		return ErrInvalidCursor
	}
	if direction != DirectionForward && direction != DirectionBackward {
		return ErrUnsupported
	}
	for {
		dr.clearDirectoryPosition()
		dr.realtimeSeek, dr.realtimeSeekBound = nil, nil
		dr.resetCandidates()
		dr.direction, dr.hasDirection, dr.merging = direction, true, true
		dr.filteredCandidates = dr.filter != nil
		dr.orderDirty = false
		best, exact := -1, -1
		for i, reader := range dr.files {
			if err := reader.seekCursorDirection(want, direction); err != nil {
				return err
			}
			key, err := reader.currentDirectoryEntryKey()
			if err == errEndOfEntries {
				continue
			}
			if err != nil {
				return err
			}
			dr.candidates[i] = &directoryCandidate{readerIndex: i, key: key}
			if want.seqnumSet && key.seqnum == want.seqnum && key.seqnumID.String() == want.seqnumID {
				exact = i
			}
			if best < 0 {
				best = i
				continue
			}
			cmp := dr.compareEntryKeys(key, dr.candidates[best].key)
			if direction == DirectionForward && cmp < 0 || direction == DirectionBackward && cmp > 0 {
				best = i
			}
		}
		if dr.orderDirty {
			dr.bootNewest = buildDirectoryBootNewest(dr.files)
			continue
		}
		if exact >= 0 {
			best = exact
		}
		if best < 0 {
			return nil
		}
		key := dr.candidates[best].key
		// Files without an initial candidate can become eligible at the selected row.
		nextWant := parsedCursorLocation{seqnumID: key.seqnumID.String(), seqnum: key.seqnum, seqnumSet: true,
			bootID: key.bootID.String(), monotonic: key.monotonic, monotonicSet: true, realtime: key.realtime, realtimeSet: true}
		for i, reader := range dr.files {
			if i == best {
				continue
			}
			if candidate := dr.candidates[i]; candidate != nil {
				cmp := dr.compareEntryKeys(candidate.key, key)
				if direction == DirectionForward && cmp > 0 || direction == DirectionBackward && cmp < 0 {
					continue
				}
				dr.candidates[i] = nil
			}
			if err := reader.seekCursorDirection(nextWant, direction); err != nil {
				return err
			}
			candidateKey, err := reader.currentDirectoryEntryKey()
			if err == errEndOfEntries {
				continue
			}
			if err != nil {
				return err
			}
			cmp := dr.compareEntryKeys(candidateKey, key)
			if direction == DirectionForward && cmp > 0 || direction == DirectionBackward && cmp < 0 {
				dr.candidates[i] = &directoryCandidate{readerIndex: i, key: candidateKey}
			}
		}
		if dr.orderDirty {
			dr.bootNewest = buildDirectoryBootNewest(dr.files)
			continue
		}
		dr.index, dr.currentKey = best, &key
		dr.candidates[best] = nil
		return nil
	}
}

// SdJournalSeekCursorDirection is an eager directional extension to the facade.
func SdJournalSeekCursorDirection(j *sdJournal, cursor string, direction Direction) error {
	j.resetIterators()
	switch reader := j.reader.(type) {
	case *Reader:
		return reader.SeekCursorDirection(cursor, direction)
	case *DirectoryReader:
		return reader.SeekCursorDirection(cursor, direction)
	default:
		return ErrUnsupported
	}
}

func (j *sdJournal) SeekCursorDirection(cursor string, direction Direction) error {
	return SdJournalSeekCursorDirection(j, cursor, direction)
}
