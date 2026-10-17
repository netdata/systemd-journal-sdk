package journal

import (
	"errors"
	"fmt"
	"sort"
)

// RecoveryReason identifies an observed unavailable journal tail.
type RecoveryReason uint8

const (
	RecoveryNone RecoveryReason = iota
	RecoveryMissingEntry
	RecoveryMissingEntryArray
)

// RecoveryStatus describes an observed intact prefix. A zero value reports no
// observed loss; it does not certify journal integrity.
type RecoveryStatus struct {
	Reason           RecoveryReason
	FirstLostOffset  uint64
	DeclaredEntries  uint64
	RecoveredEntries uint64
}

func (s RecoveryStatus) Incomplete() bool { return s.Reason != RecoveryNone }

// FileRecoveryStatus associates observed loss with a journal's file ID.
type FileRecoveryStatus struct {
	FileID UUID
	Status RecoveryStatus
}

type readerRecoveryState struct {
	RecoveryStatus
	tail entryHeader
}

func (s *readerRecoveryState) Incomplete() bool { return s != nil }
func (s *readerRecoveryState) status() RecoveryStatus {
	if s == nil {
		return RecoveryStatus{}
	}
	return s.RecoveryStatus
}
func sameRecoveryState(a, b *readerRecoveryState) bool {
	return a == b || (a != nil && b != nil && *a == *b)
}

func (r *Reader) RecoveryStatus() RecoveryStatus { return r.recovery.status() }

func (r *Reader) RecoveryStatuses() []FileRecoveryStatus {
	if !r.recovery.Incomplete() {
		return nil
	}
	return []FileRecoveryStatus{{FileID: r.header.fileID, Status: r.recovery.status()}}
}

func (dr *DirectoryReader) RecoveryStatuses() []FileRecoveryStatus {
	var out []FileRecoveryStatus
	for _, reader := range dr.files {
		if reader.recovery.Incomplete() {
			out = append(out, FileRecoveryStatus{FileID: reader.header.fileID, Status: reader.recovery.status()})
		}
	}
	return out
}

// SdJournalRecoveryStatuses returns diagnostics without changing iteration.
func SdJournalRecoveryStatuses(j *sdJournal) []FileRecoveryStatus {
	return j.reader.RecoveryStatuses()
}

type readerBoundsError struct{ offset, size, fileSize uint64 }

type readerTailError struct{ offset uint64 }

func (e readerTailError) Error() string {
	return fmt.Sprintf("%v: unpublished reader tail", errCorruptObject)
}
func (e readerTailError) Unwrap() error { return errCorruptObject }

func readerTailFailure(err error) (uint64, bool) {
	if bound, ok := tailBoundsFailure(err); ok {
		return bound.offset, true
	}
	var missing readerTailError
	if errors.As(err, &missing) {
		return missing.offset, true
	}
	return 0, false
}

func (e readerBoundsError) Error() string {
	return fmt.Sprintf("%v: reader access out of bounds", errInvalidJournal)
}

func (e readerBoundsError) Unwrap() error { return errInvalidJournal }

func tailBoundsFailure(err error) (readerBoundsError, bool) {
	var bound readerBoundsError
	if !errors.As(err, &bound) {
		return bound, false
	}
	_, valid := checkedAdd(bound.offset, bound.size)
	return bound, valid && (bound.offset > bound.fileSize || bound.size > bound.fileSize-bound.offset)
}

func (r *Reader) recoverPartialArray(offset, remaining uint64) ([]uint64, error) {
	header, capacity, err := r.readOffsetArrayHeader(offset)
	if err != nil {
		return nil, err
	}
	start, ok := checkedAdd(offset, offsetArrayObjectHeaderSize)
	if !ok || start > r.fileSize {
		return nil, readerBoundsError{offset, header.object.size, r.fileSize}
	}
	available := minUint64(minUint64(remaining, capacity), (r.fileSize-start)/r.offsetArrayItemSize())
	byteCount := available * r.offsetArrayItemSize()
	if byteCount > uint64(int(^uint(0)>>1)) {
		return nil, errCorruptObject
	}
	buf := make([]byte, byteCount)
	if err := r.readAt(buf, start); err != nil {
		return nil, err
	}
	return r.validEntryOffsetsFromArray(buf, r.offsetArrayItemSize())
}

func (r *Reader) retainRecoveredPrefix(offsets []uint64, reason RecoveryReason, lost uint64, previous *readerRecoveryState) error {
	if err := r.validateRecoveredSuffix(); err != nil {
		return err
	}
	status := RecoveryStatus{reason, lost, r.header.nEntries, uint64(len(offsets))}
	r.entryOffsets = offsets
	r.entryIndex = -1
	var tail entryHeader
	if len(offsets) != 0 {
		header, err := r.readEntryHeaderAt(offsets[len(offsets)-1])
		if err != nil {
			return err
		}
		tail = header
	}
	if previous != nil && previous.RecoveryStatus == status && previous.tail == tail {
		r.recovery = previous
	} else {
		r.recovery = &readerRecoveryState{RecoveryStatus: status, tail: tail}
	}
	return nil
}

// A missing tail may end a prefix, but must not hide a readable later entry.
// This additional native-index pass runs only after a failed tail access.
func (r *Reader) validateRecoveredSuffix() error {
	remaining := r.header.nEntries
	missing := false
	previous := uint64(0)
	highest := r.header.entryArrayOffset
	for offset := r.header.entryArrayOffset; remaining > 0; {
		if offset == 0 {
			return nil
		}
		header, capacity, err := r.readOffsetArrayHeader(offset)
		if err != nil {
			if _, ok := readerTailFailure(err); ok {
				return nil
			}
			return err
		}
		wanted := minUint64(remaining, capacity)
		start := offset + offsetArrayObjectHeaderSize
		available := uint64(0)
		if start <= r.fileSize {
			available = minUint64(wanted, (r.fileSize-start)/r.offsetArrayItemSize())
		}
		for i := uint64(0); i < available; i++ {
			buf, err := r.readSlice(start+i*r.offsetArrayItemSize(), r.offsetArrayItemSize())
			if err != nil {
				return err
			}
			entry := entryOffsetArrayItem(buf, r.offsetArrayItemSize())
			if entry == 0 {
				missing = true
				continue
			}
			valid, err := r.validEntryObjectOffset(entry)
			if err != nil {
				if _, ok := readerTailFailure(err); !ok {
					return err
				}
				missing = true
				continue
			}
			if valid && missing {
				return errCorruptObject
			}
			if valid && entry <= previous {
				return errCorruptObject
			}
			previous = entry
		}
		missing = missing || available < wanted
		remaining -= wanted
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
	return nil
}

func (r *Reader) effectiveHeader() journalHeader {
	header := r.header
	if r.recovery.Incomplete() {
		header.nEntries = r.recovery.RecoveredEntries
		header.tailEntrySeqnum = r.recovery.tail.seqnum
		header.tailEntryRealtime = r.recovery.tail.realtime
		header.tailEntryMonotonic = r.recovery.tail.monotonic
		header.tailEntryBootID = r.recovery.tail.bootID
	}
	return header
}

func (r *Reader) recoveredEntryPresent(offset uint64) bool {
	index := sort.Search(len(r.entryOffsets), func(i int) bool { return r.entryOffsets[i] >= offset })
	return index < len(r.entryOffsets) && r.entryOffsets[index] == offset
}

func (r *Reader) visitRecoveredDataEntryOffsets(header dataHeader, visit func(uint64) error) error {
	if header.nEntries == 0 || len(r.entryOffsets) == 0 {
		return nil
	}
	last := r.entryOffsets[len(r.entryOffsets)-1]
	if header.entryOffset == 0 {
		return errCorruptObject
	}
	if header.entryOffset > last {
		return nil
	}
	previous := header.entryOffset
	if header.entryOffset != 0 && r.recoveredEntryPresent(header.entryOffset) {
		if err := visit(header.entryOffset); err != nil {
			return err
		}
	}
	remaining := header.nEntries - 1
	highest := header.entryArrayOffset
	for offset := header.entryArrayOffset; offset != 0 && remaining > 0; {
		array, offsets, used, err := r.readEntryArrayChunk(offset, remaining)
		lost := err != nil
		knownBeyondTail := false
		if err != nil {
			lostOffset, recoverable := readerTailFailure(err)
			if !recoverable {
				return err
			}
			knownBeyondTail = lostOffset > last && lostOffset != offset && lostOffset != offset+offsetArrayObjectHeaderSize
			if lostOffset == offset+offsetArrayObjectHeaderSize {
				offsets, err = r.recoverPartialArray(offset, remaining)
				if err != nil {
					if entryLost, recoverable := readerTailFailure(err); !recoverable {
						return err
					} else {
						knownBeyondTail = entryLost > last && entryLost != offset && entryLost != offset+offsetArrayObjectHeaderSize
					}
				}
			}
		}
		for _, entry := range offsets {
			if entry > last {
				return nil
			}
			if entry <= previous {
				return errCorruptObject
			}
			previous = entry
			if r.recoveredEntryPresent(entry) {
				if visitErr := visit(entry); visitErr != nil {
					return visitErr
				}
			}
		}
		if lost {
			if !knownBeyondTail && previous < last {
				return errCorruptObject
			}
			return nil
		}
		remaining -= used
		if remaining > 0 && array.nextArrayOffset != 0 {
			if array.nextArrayOffset <= highest {
				if err := rejectArrayRevisit(r, header.entryArrayOffset, offset, array.nextArrayOffset, errCorruptObject); err != nil {
					return err
				}
			} else {
				highest = array.nextArrayOffset
			}
		}
		offset = array.nextArrayOffset
	}
	if remaining > 0 && previous < last {
		return errCorruptObject
	}
	return nil
}
