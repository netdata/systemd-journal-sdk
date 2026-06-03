package journal

import (
	"encoding/binary"
	"fmt"
)

func nextEntryArrayCapacity(index, previousCapacity uint64) uint64 {
	capacity := previousCapacity
	if index > capacity {
		capacity = (index + 1) * 2
	} else {
		capacity *= 2
	}
	if capacity < 4 {
		capacity = 4
	}
	return capacity
}

func (w *Writer) appendToEntryArray(entryOffset uint64) error {
	if w.header.entryArrayOffset == 0 {
		return w.initEntryArray(entryOffset)
	}

	tailOffset, err := w.entryArrayTailOffset()
	if err != nil {
		return err
	}

	_, cap, err := w.readOffsetArrayHeader(tailOffset)
	if err != nil {
		return err
	}
	tailEntries, err := w.entryArrayTailEntries(tailOffset)
	if err != nil {
		return err
	}
	if tailEntries < cap {
		return w.appendToExistingEntryArrayTail(tailOffset, tailEntries, entryOffset)
	}

	newOffset, err := w.allocateOffsetArray(nextEntryArrayCapacity(w.header.nEntries, cap))
	if err != nil {
		return err
	}
	if err := w.writeUint64At(tailOffset+16, newOffset); err != nil {
		return err
	}
	if err := w.writeArrayItem(newOffset, 0, entryOffset); err != nil {
		return err
	}
	w.header.tailEntryArrayOffset = uint32(newOffset)
	w.header.tailEntryArrayNEntries = 1
	return nil
}

func (w *Writer) initEntryArray(entryOffset uint64) error {
	arrayOffset, err := w.allocateOffsetArray(4)
	if err != nil {
		return err
	}
	w.header.entryArrayOffset = arrayOffset
	w.header.tailEntryArrayOffset = uint32(arrayOffset)
	w.header.tailEntryArrayNEntries = 1
	return w.writeArrayItem(arrayOffset, 0, entryOffset)
}

func (w *Writer) entryArrayTailOffset() (uint64, error) {
	tailOffset := uint64(w.header.tailEntryArrayOffset)
	if tailOffset != 0 {
		return tailOffset, nil
	}
	tailOffset = w.header.entryArrayOffset
	for remaining := w.header.nEntries; ; {
		header, cap, err := w.readOffsetArrayHeader(tailOffset)
		if err != nil {
			return 0, err
		}
		if remaining < cap || header.nextArrayOffset == 0 {
			return tailOffset, nil
		}
		remaining -= cap
		tailOffset = header.nextArrayOffset
	}
}

func (w *Writer) entryArrayTailEntries(tailOffset uint64) (uint64, error) {
	tailEntries := uint64(w.header.tailEntryArrayNEntries)
	if tailEntries != 0 {
		return tailEntries, nil
	}
	tailEntries = w.header.nEntries
	for offset := w.header.entryArrayOffset; offset != 0 && offset != tailOffset; {
		h, c, err := w.readOffsetArrayHeader(offset)
		if err != nil {
			return 0, err
		}
		tailEntries -= c
		offset = h.nextArrayOffset
	}
	return tailEntries, nil
}

func (w *Writer) appendToExistingEntryArrayTail(tailOffset, tailEntries, entryOffset uint64) error {
	if err := w.writeArrayItem(tailOffset, tailEntries, entryOffset); err != nil {
		return err
	}
	w.header.tailEntryArrayOffset = uint32(tailOffset)
	w.header.tailEntryArrayNEntries = uint32(tailEntries + 1)
	return nil
}

func (w *Writer) allocateOffsetArray(capacity uint64) (uint64, error) {
	offset := w.appendOffset
	size := uint64(offsetArrayObjectHeaderSize) + capacity*w.offsetArrayItemSize()
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, err
	}
	buf, direct, err := w.newObjectBuffer(offset, size)
	if err != nil {
		return 0, err
	}
	putOffsetArrayHeader(buf[:offsetArrayObjectHeaderSize], offsetArrayHeader{
		object: objectHeader{typ: objectTypeEntryArray, size: size},
	})
	if err := w.commitObjectBuffer(offset, buf, direct); err != nil {
		return 0, err
	}
	if err := w.objectAdded(offset, size); err != nil {
		return 0, err
	}
	w.header.nEntryArrays++
	if err := w.publishObjectMetadata(); err != nil {
		return 0, err
	}
	if err := w.hmacPutObject(offset, objectTypeEntryArray); err != nil {
		return 0, err
	}
	return offset, nil
}

func (w *Writer) readOffsetArrayHeader(offset uint64) (offsetArrayHeader, uint64, error) {
	var src []byte
	if w.arena != nil {
		if data, ok, err := w.arena.directBytesAt(offset, offsetArrayObjectHeaderSize); err != nil || ok {
			if err != nil {
				return offsetArrayHeader{}, 0, err
			}
			src = data
		}
	}
	var buf [offsetArrayObjectHeaderSize]byte
	if src == nil {
		if err := w.readAt(buf[:], offset); err != nil {
			return offsetArrayHeader{}, 0, err
		}
		src = buf[:]
	}
	header, err := parseOffsetArrayHeader(src)
	if err != nil {
		return offsetArrayHeader{}, 0, err
	}
	if header.object.typ != objectTypeEntryArray || header.object.size < offsetArrayObjectHeaderSize {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	itemSize := w.offsetArrayItemSize()
	if (header.object.size-offsetArrayObjectHeaderSize)%itemSize != 0 {
		return offsetArrayHeader{}, 0, errInvalidJournal
	}
	return header, (header.object.size - offsetArrayObjectHeaderSize) / itemSize, nil
}

func (w *Writer) writeArrayItem(arrayOffset, index, entryOffset uint64) error {
	itemOffset := arrayOffset + offsetArrayObjectHeaderSize + index*w.offsetArrayItemSize()
	if w.compact {
		if entryOffset > journalCompactSizeMax {
			return fmt.Errorf("%w: compact entry offset exceeds 32-bit range", errInvalidJournal)
		}
		return w.writeUint32At(itemOffset, uint32(entryOffset))
	}
	return w.writeUint64At(itemOffset, entryOffset)
}

func (w *Writer) linkDataToEntry(dataOffset, entryOffset uint64) error {
	header, err := w.readDataHeader(dataOffset)
	if err != nil {
		return err
	}
	switch header.nEntries {
	case 0:
		return w.linkFirstEntryToData(dataOffset, entryOffset)
	case 1:
		return w.linkSecondEntryToData(dataOffset, entryOffset)
	default:
		return w.linkLaterEntryToData(dataOffset, entryOffset, header)
	}
}

func (w *Writer) linkFirstEntryToData(dataOffset, entryOffset uint64) error {
	if err := w.writeUint64At(dataOffset+40, entryOffset); err != nil {
		return err
	}
	return w.writeUint64At(dataOffset+56, 1)
}

func (w *Writer) linkSecondEntryToData(dataOffset, entryOffset uint64) error {
	arrayOffset, err := w.allocateOffsetArray(4)
	if err != nil {
		return err
	}
	if err := w.writeArrayItem(arrayOffset, 0, entryOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(dataOffset+48, arrayOffset); err != nil {
		return err
	}
	if w.compact {
		if err := w.writeCompactDataTail(dataOffset, arrayOffset, 1); err != nil {
			return err
		}
	}
	return w.writeUint64At(dataOffset+56, 2)
}

func (w *Writer) linkLaterEntryToData(dataOffset, entryOffset uint64, header dataHeader) error {
	if header.entryArrayOffset == 0 {
		return errInvalidJournal
	}
	currentCount := header.nEntries - 1
	tailOffset, tailEntries, err := w.appendToDataEntryArrayTail(dataOffset, header.entryArrayOffset, currentCount, entryOffset)
	if err != nil {
		return err
	}
	if w.compact {
		if err := w.writeCompactDataTail(dataOffset, tailOffset, tailEntries); err != nil {
			return err
		}
	}
	return w.writeUint64At(dataOffset+56, header.nEntries+1)
}

func (w *Writer) appendToDataEntryArrayTail(dataOffset, entryArrayOffset, currentCount, entryOffset uint64) (uint64, uint64, error) {
	tailOffset, tailEntries, ok, err := w.appendToCompactDataEntryArrayTail(dataOffset, currentCount, entryOffset)
	if err != nil || ok {
		return tailOffset, tailEntries, err
	}
	return w.appendToDataEntryArray(entryArrayOffset, currentCount, entryOffset)
}

func (w *Writer) appendToCompactDataEntryArrayTail(dataOffset, currentCount, entryOffset uint64) (uint64, uint64, bool, error) {
	if !w.compact {
		return 0, 0, false, nil
	}
	tailOffset, tailEntries, ok, err := w.readCompactDataTail(dataOffset)
	if err != nil {
		return 0, 0, false, err
	}
	if !ok || tailEntries == 0 || tailEntries > currentCount {
		return 0, 0, false, nil
	}
	header, cap, err := w.readOffsetArrayHeader(tailOffset)
	if err != nil || header.nextArrayOffset != 0 || tailEntries > cap {
		return 0, 0, false, nil
	}
	if tailEntries < cap {
		return w.appendToExistingCompactDataTail(tailOffset, tailEntries, entryOffset)
	}
	return w.appendNewCompactDataTail(tailOffset, currentCount, cap, entryOffset)
}

func (w *Writer) appendToExistingCompactDataTail(tailOffset, tailEntries, entryOffset uint64) (uint64, uint64, bool, error) {
	if err := w.writeArrayItem(tailOffset, tailEntries, entryOffset); err != nil {
		return 0, 0, false, err
	}
	return tailOffset, tailEntries + 1, true, nil
}

func (w *Writer) appendNewCompactDataTail(tailOffset, currentCount, cap, entryOffset uint64) (uint64, uint64, bool, error) {
	newOffset, err := w.allocateOffsetArray(nextEntryArrayCapacity(currentCount, cap))
	if err != nil {
		return 0, 0, false, err
	}
	if err := w.writeUint64At(tailOffset+16, newOffset); err != nil {
		return 0, 0, false, err
	}
	if err := w.writeArrayItem(newOffset, 0, entryOffset); err != nil {
		return 0, 0, false, err
	}
	return newOffset, 1, true, nil
}

func (w *Writer) readCompactDataTail(dataOffset uint64) (uint64, uint64, bool, error) {
	if !w.compact {
		return 0, 0, false, nil
	}
	tailFieldOffset := dataOffset + compactDataTailOffsetOffset
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(tailFieldOffset, 8); err != nil || ok {
			if err != nil {
				return 0, 0, false, err
			}
			tailOffset := uint64(binary.LittleEndian.Uint32(src[0:4]))
			tailEntries := uint64(binary.LittleEndian.Uint32(src[4:8]))
			return tailOffset, tailEntries, tailOffset != 0 && tailEntries != 0, nil
		}
	}
	var buf [8]byte
	if err := w.readAt(buf[:], tailFieldOffset); err != nil {
		return 0, 0, false, err
	}
	tailOffset := uint64(binary.LittleEndian.Uint32(buf[0:4]))
	tailEntries := uint64(binary.LittleEndian.Uint32(buf[4:8]))
	return tailOffset, tailEntries, tailOffset != 0 && tailEntries != 0, nil
}

func (w *Writer) writeCompactDataTail(dataOffset, tailOffset, tailEntries uint64) error {
	if tailOffset > journalCompactSizeMax || tailEntries > uint64(^uint32(0)) {
		return fmt.Errorf("%w: compact DATA tail exceeds 32-bit range", errInvalidJournal)
	}
	if err := w.writeUint32At(dataOffset+compactDataTailOffsetOffset, uint32(tailOffset)); err != nil {
		return err
	}
	return w.writeUint32At(dataOffset+compactDataTailEntriesOffset, uint32(tailEntries))
}

func (w *Writer) appendToDataEntryArray(arrayOffset, currentCount, entryOffset uint64) (uint64, uint64, error) {
	remaining := currentCount
	offset := arrayOffset
	for {
		header, cap, err := w.readOffsetArrayHeader(offset)
		if err != nil {
			return 0, 0, err
		}
		if remaining < cap {
			if err := w.writeArrayItem(offset, remaining, entryOffset); err != nil {
				return 0, 0, err
			}
			return offset, remaining + 1, nil
		}
		remaining -= cap
		if header.nextArrayOffset == 0 {
			newOffset, err := w.allocateOffsetArray(nextEntryArrayCapacity(currentCount, cap))
			if err != nil {
				return 0, 0, err
			}
			if err := w.writeUint64At(offset+16, newOffset); err != nil {
				return 0, 0, err
			}
			if err := w.writeArrayItem(newOffset, 0, entryOffset); err != nil {
				return 0, 0, err
			}
			return newOffset, 1, nil
		}
		offset = header.nextArrayOffset
	}
}

func (w *Writer) entryItemSize() uint64 {
	if w.compact {
		return compactEntryItemSize
	}
	return regularEntryItemSize
}

func (w *Writer) offsetArrayItemSize() uint64 {
	if w.compact {
		return compactOffsetArrayItemSize
	}
	return regularOffsetArrayItemSize
}

func (w *Writer) dataPayloadOffset() uint64 {
	if w.compact {
		return compactDataObjectHeaderSize
	}
	return dataObjectHeaderSize
}

func (w *Writer) ensureCompactObjectFits(offset, size uint64) error {
	if !w.compact {
		return nil
	}
	if offset > journalCompactSizeMax || align8(offset+size) > journalCompactSizeMax {
		return fmt.Errorf("%w: compact journal cannot exceed 4 GiB", errInvalidJournal)
	}
	return nil
}
