package journal

import "fmt"

// validateDeclaredArena bounds header-addressed objects by the declared arena,
// not physical preallocation. It reads no objects and is safe before append-open.
func (h *journalHeader) validateDeclaredArena(fileSize uint64) (uint64, error) {
	if h.headerSize < headerMinSize || h.headerSize%objectAlignment != 0 || h.headerSize > fileSize {
		return 0, fmt.Errorf("%w: invalid header extent", errInvalidJournal)
	}
	if h.arenaSize > fileSize-h.headerSize {
		return 0, fmt.Errorf("%w: declared arena exceeds file", errInvalidJournal)
	}
	end := h.headerSize + h.arenaSize
	if err := h.validateHeaderPopulation(); err != nil {
		return 0, err
	}
	if (h.tailObjectOffset == 0) != (h.nObjects == 0) {
		return 0, fmt.Errorf("%w: object count and tail disagree", errInvalidJournal)
	}
	for _, object := range []struct{ offset, size uint64 }{
		{h.tailObjectOffset, objectHeaderSize},
		{h.entryArrayOffset, offsetArrayObjectHeaderSize},
	} {
		if object.offset != 0 {
			if err := h.validateArenaObject(object.offset, object.size, end); err != nil {
				return 0, err
			}
		}
	}
	if err := h.validateArenaHashTables(end); err != nil {
		return 0, err
	}
	if h.headerSize >= 264 && h.tailEntryArrayOffset != 0 {
		itemSize := uint64(regularOffsetArrayItemSize)
		if h.isCompact() {
			itemSize = compactOffsetArrayItemSize
		}
		size := offsetArrayObjectHeaderSize + uint64(h.tailEntryArrayNEntries)*itemSize
		if err := h.validateArenaObject(uint64(h.tailEntryArrayOffset), size, end); err != nil {
			return 0, err
		}
	}
	if h.headerSize >= 272 && h.tailEntryOffset != 0 {
		if err := h.validateArenaObject(h.tailEntryOffset, entryObjectHeaderSize, end); err != nil {
			return 0, err
		}
	}
	return end, nil
}

func (h *journalHeader) validateHeaderPopulation() error {
	if h.nObjects > h.arenaSize/objectHeaderSize || h.nEntries > h.nObjects {
		return fmt.Errorf("%w: object population exceeds bounds", errInvalidJournal)
	}
	// Object types are disjoint; subtraction also avoids aggregate overflow.
	// Each nonzero hash-table offset declares one object without a count field.
	remaining := h.nObjects - h.nEntries
	for _, field := range []struct{ end, count uint64 }{
		{120, min(h.dataHashTableOffset, 1)},
		{136, min(h.fieldHashTableOffset, 1)},
		{216, h.nData},
		{224, h.nFields},
		{232, h.nTags},
		{240, h.nEntryArrays},
	} {
		if h.headerSize < field.end {
			continue
		}
		if field.count > remaining {
			return fmt.Errorf("%w: object type counts exceed object population", errInvalidJournal)
		}
		remaining -= field.count
	}
	return h.validateArrayPopulation()
}

func (h *journalHeader) validateArrayPopulation() error {
	if h.headerSize >= 240 && h.nEntryArrays < h.minimumEntryArrays() {
		return fmt.Errorf("%w: array count does not cover declared locations", errInvalidJournal)
	}
	if h.headerSize < 264 {
		return nil
	}
	offset, count := uint64(h.tailEntryArrayOffset), uint64(h.tailEntryArrayNEntries)
	if h.entryArrayOffset > offset || (h.entryArrayOffset == 0 && offset != 0) {
		return fmt.Errorf("%w: entry array tail disagrees with first array", errInvalidJournal)
	}
	if (offset == 0) != (count == 0) || count > h.nEntries {
		return fmt.Errorf("%w: entry array tail disagrees with entry population", errInvalidJournal)
	}
	return nil
}

// The first and cached tail may name the same array; DATA arrays can add more.
func (h *journalHeader) minimumEntryArrays() uint64 {
	var count uint64
	if h.entryArrayOffset != 0 {
		count++
	}
	if h.headerSize >= 264 && h.tailEntryArrayOffset != 0 && uint64(h.tailEntryArrayOffset) != h.entryArrayOffset {
		count++
	}
	return count
}

func (h *journalHeader) validateArenaHashTables(end uint64) error {
	for _, table := range []struct{ offset, size uint64 }{
		{h.dataHashTableOffset, h.dataHashTableSize},
		{h.fieldHashTableOffset, h.fieldHashTableSize},
	} {
		if table.offset == 0 && table.size == 0 {
			continue
		}
		if table.offset < objectHeaderSize || table.size < hashItemSize || table.size%hashItemSize != 0 {
			return fmt.Errorf("%w: invalid hash table extent", errInvalidJournal)
		}
		size, ok := checkedAdd(objectHeaderSize, table.size)
		if !ok {
			return fmt.Errorf("%w: hash table extent overflows", errInvalidJournal)
		}
		if err := h.validateArenaObject(table.offset-objectHeaderSize, size, end); err != nil {
			return err
		}
	}
	return nil
}

func (h *journalHeader) validateArenaObject(offset, size, end uint64) error {
	if offset < h.headerSize || offset%objectAlignment != 0 || offset > h.tailObjectOffset {
		return fmt.Errorf("%w: object offset outside declared arena", errInvalidJournal)
	}
	if size < objectHeaderSize || offset > end || size > end-offset {
		return fmt.Errorf("%w: object exceeds declared arena", errInvalidJournal)
	}
	return nil
}

// An empty rotated file may inherit tailEntrySeqnum. All metadata describing
// entries in this file must be empty; absent historical fields are not active.
func (h *journalHeader) validateEmptyEntryMetadata() error {
	if h.nEntries != 0 {
		return nil
	}
	if h.headEntrySeqnum != 0 || h.headEntryRealtime != 0 || h.tailEntryRealtime != 0 || h.tailEntryMonotonic != 0 || h.entryArrayOffset != 0 {
		return fmt.Errorf("%w: entry metadata present for empty population", errInvalidJournal)
	}
	if h.headerSize >= 264 && (h.tailEntryArrayOffset != 0 || h.tailEntryArrayNEntries != 0) {
		return fmt.Errorf("%w: entry array tail present for empty population", errInvalidJournal)
	}
	if h.headerSize >= 272 && (h.tailEntryOffset != 0 || (h.compatibleFlags&compatibleTailEntryBootID != 0 && !isZeroUUID(h.tailEntryBootID))) {
		return fmt.Errorf("%w: entry tail present for empty population", errInvalidJournal)
	}
	return nil
}
