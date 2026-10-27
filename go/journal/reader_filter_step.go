package journal

import (
	"errors"
	"sort"
)

func (r *Reader) normalizeFilteredPosition(direction Direction) error {
	if r.entryIndex != -2 || r.filter == nil {
		return nil
	}
	offset := r.filter.overrideOffset
	if uint64(len(r.entryOffsets)) != r.effectiveHeader().nEntries {
		if err := r.refreshFilterOffsets(); err != nil {
			return err
		}
	}
	r.entryIndex = sort.Search(len(r.entryOffsets), func(i int) bool { return r.entryOffsets[i] >= offset })
	if direction == DirectionForward && (r.entryIndex >= len(r.entryOffsets) || r.entryOffsets[r.entryIndex] != offset) {
		r.entryIndex--
	}
	return nil
}

func (r *Reader) stepFiltered(direction Direction) (bool, error) {
	r.clearCurrentEntryState()
	if r.accessor == nil {
		return false, errInvalidJournal
	}
	if r.recovery != nil && r.recovery.eofValid {
		if unchanged, err := r.unchangedRecoveryEOF(direction); err != nil || unchanged {
			return false, err
		}
	}
	f := r.filter
	if f.retry {
		// Retry with fresh bytes, including any cached by getters after failure.
		r.accessor.invalidateCopiedWindows()
		f.retry = false
	}
	for attempt := 0; ; attempt++ {
		if err := f.build(r); err != nil {
			f.retry = true
			return false, err
		}
		value, err := r.filteredOffset(direction)
		if err == nil && value != 0 {
			index := r.entryIndex
			if direction == DirectionForward {
				index++
			} else {
				index--
			}
			if index < 0 || index >= len(r.entryOffsets) || r.entryOffsets[index] != value {
				index = sort.Search(len(r.entryOffsets), func(i int) bool { return r.entryOffsets[i] >= value })
			}
			if index >= len(r.entryOffsets) || r.entryOffsets[index] != value {
				index = -2
			}
			var header entryHeader
			if err == nil {
				header, err = r.readEntryHeaderAt(value)
				if errors.Is(err, errCorruptObject) {
					_, classified := r.validEntryObjectOffset(value)
					if classified != nil {
						err = classified
					}
				}
			}
			if err == nil {
				err = r.validateFilteredEntryHeader(value, header)
			}
			if err == nil {
				r.clearRecoveryEOF()
				r.entryIndex = index
				f.overrideOffset = value
				r.currentHeader, r.currentHeaderOffset, r.currentHeaderValid = header, value, true
				r.realtimeSeek = nil
				f.walkOffset, f.walkValid = value, true
				f.backward = direction == DirectionBackward
				f.retry = false
				return true, nil
			}
		}
		if err != nil {
			f.retry = true
			f.walkValid = false
			invalidateFilterWalk(f.expr)
			if !filterTailFailure(err) || !r.allowRecovery {
				return false, err
			}
		}
		if attempt != 0 || err == nil && !r.recovery.Incomplete() {
			if err == nil {
				r.rememberRecoveryEOF(direction)
			}
			return false, err
		}
		changed, refreshErr := r.refreshFilterRecovery()
		if refreshErr != nil {
			return false, refreshErr
		}
		if !changed {
			if err == nil {
				r.rememberRecoveryEOF(direction)
			}
			return false, err
		}
	}
}

func (r *Reader) validateFilteredEntryHeader(offset uint64, header entryHeader) error {
	size := header.object.size
	if offset < r.header.headerSize || offset&7 != 0 || size < entryObjectHeaderSize ||
		(size-entryObjectHeaderSize)&(r.entryItemSize()-1) != 0 {
		return errCorruptObject
	}
	arenaEnd, ok := checkedAdd(r.header.headerSize, r.header.arenaSize)
	if !ok || offset > arenaEnd || size > arenaEnd-offset {
		return errCorruptObject
	}
	if offset > r.fileSize || size > r.fileSize-offset {
		return readerBoundsError{offset, size, r.fileSize}
	}
	return nil
}

func (r *Reader) filteredOffset(direction Direction) (uint64, error) {
	f := r.filter
	if r.effectiveHeader().nEntries == 0 && r.realtimeSeek == nil {
		return 0, nil
	}
	needle := uint64(1)
	reset := !f.walkValid || f.backward != (direction == DirectionBackward)
	if r.realtimeSeek != nil {
		var index int
		var err error
		if direction == DirectionForward {
			index, err = r.filterRealtimeIndex(*r.realtimeSeek, false)
		} else {
			index, err = r.filterRealtimeIndex(*r.realtimeSeek, true)
			index--
		}
		if err != nil {
			return 0, err
		}
		if f.stale {
			if err := f.build(r); err != nil {
				return 0, err
			}
		}
		if index < 0 || index >= len(r.entryOffsets) {
			return 0, nil
		}
		f.walkValid = false
		return seekFilterRealtime(f.expr, r, r.entryOffsets[index], direction)
	} else if r.entryIndex < 0 && r.entryIndex != -2 {
		if direction == DirectionBackward {
			return 0, nil
		}
		reset = true
	} else if r.entryIndex >= len(r.entryOffsets) {
		if direction == DirectionForward {
			return 0, nil
		}
		needle = ^uint64(0)
		reset = true
	} else {
		current, err := r.currentEntryOffset()
		if err != nil {
			return 0, err
		}
		reset = reset || f.walkOffset != current
		if direction == DirectionForward {
			if current == ^uint64(0) {
				return 0, nil
			}
			needle = current + 1
		} else {
			needle = current - 1
		}
		if reset {
			prepareResolvedFilterWalk(f.expr, direction)
			f.walkOffset, f.walkValid = current, true
			f.backward = direction == DirectionBackward
		}
		return f.expr.seek(r, needle, direction)
	}
	if reset {
		f.walkValid = false
		if err := f.expr.reset(r, direction); err != nil {
			return 0, err
		}
	}
	return f.expr.seek(r, needle, direction)
}

// The filtered seek path must propagate metadata read failures instead of
// converting a failed binary-search comparison into an apparent result.
func (r *Reader) filterRealtimeIndex(usec uint64, after bool) (int, error) {
	if uint64(len(r.entryOffsets)) != r.effectiveHeader().nEntries {
		if err := r.refreshFilterOffsets(); err != nil {
			return 0, err
		}
	}
	left, right := 0, len(r.entryOffsets)
	for left < right {
		mid := left + (right-left)/2
		realtime, err := r.entryRealtimeAtIndex(mid)
		if err != nil {
			return 0, err
		}
		if realtime < usec || after && realtime == usec {
			left = mid + 1
		} else {
			right = mid
		}
	}
	return left, nil
}
