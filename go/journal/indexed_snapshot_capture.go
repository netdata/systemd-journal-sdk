package journal

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
)

func (s *IndexedSnapshot) captureBoundary(ctx context.Context) error {
	r, h := s.reader, s.reader.header
	arenaEnd, err := h.validateDeclaredArena(r.fileSize)
	if err != nil {
		return err
	}
	if err := s.captureObjectEnd(arenaEnd); err != nil {
		return err
	}
	if h.nEntries == 0 {
		return h.validateEmptyEntryMetadata()
	}
	if h.nEntries > s.objectEnd/entryObjectHeaderSize {
		return snapshotCorrupt("entry count exceeds file bounds")
	}
	entry, array, used, err := s.arrayTail(ctx, h.entryArrayOffset, h.nEntries)
	if err != nil {
		return err
	}
	s.maxEntry = entry
	if err := s.checkOffset(entry); err != nil {
		return err
	}
	eh, err := r.readEntryHeaderAt(entry)
	if err != nil {
		return err
	}
	if err := s.checkObject(entry, eh.object.size); err != nil {
		return err
	}
	if eh.seqnum != h.tailEntrySeqnum || eh.realtime != h.tailEntryRealtime || (h.compatibleFlags&compatibleTailEntryBootID != 0 && (eh.monotonic != h.tailEntryMonotonic || eh.bootID != h.tailEntryBootID)) {
		return snapshotCorrupt("tail metadata disagrees with committed entry count")
	}
	if h.headerSize >= 272 && h.tailEntryOffset != entry {
		return snapshotCorrupt("tail entry hint disagrees with committed entry count")
	}
	if h.tailEntryArrayHintUsable() && (uint64(h.tailEntryArrayOffset) != array || uint64(h.tailEntryArrayNEntries) != used) {
		return snapshotCorrupt("tail array hint disagrees with committed entry count")
	}
	return nil
}

func (s *IndexedSnapshot) captureObjectEnd(arenaEnd uint64) error {
	r, h := s.reader, s.reader.header
	if s.maxObject == 0 {
		s.objectEnd = h.headerSize
	} else {
		if err := s.checkOffset(s.maxObject); err != nil {
			return err
		}
		tail, err := readObjectHeaderAt(r.file, s.maxObject)
		if err != nil {
			return err
		}
		if err := h.validateArenaObject(s.maxObject, tail.size, arenaEnd); err != nil {
			return err
		}
		s.objectEnd = s.maxObject + tail.size
	}
	return nil
}

// arrayTail reads array headers and the terminal slot, not a vector of entries.
func (s *IndexedSnapshot) arrayTail(ctx context.Context, off, count uint64) (uint64, uint64, uint64, error) {
	for count > 0 {
		if err := ctx.Err(); err != nil {
			return 0, 0, 0, err
		}
		a, capacity, err := s.arrayHeader(off)
		if err != nil {
			return 0, 0, 0, err
		}
		if count <= capacity {
			var buf [8]byte
			size := s.reader.offsetArrayItemSize()
			if err := s.reader.readAt(buf[:size], off+offsetArrayObjectHeaderSize+(count-1)*size); err != nil {
				return 0, 0, 0, err
			}
			entry := entryOffsetArrayItem(buf[:], size)
			if err := s.checkOffset(entry); err != nil {
				return 0, 0, 0, err
			}
			return entry, off, count, nil
		}
		count -= capacity
		off = a.nextArrayOffset
	}
	return 0, 0, 0, snapshotCorrupt("empty terminal array")
}

func (s *IndexedSnapshot) validateCapturedPostings(ctx context.Context, off uint64, d dataHeader) error {
	if d.nEntries == 0 {
		return snapshotCorrupt("DATA has no committed posting")
	}
	if d.nEntries > s.reader.header.nEntries || d.entryOffset > s.maxEntry {
		return snapshotCorrupt("DATA count exceeds committed population")
	}
	if err := s.validateEntry(d.entryOffset); err != nil {
		return err
	}
	if d.nEntries == 1 {
		if d.entryArrayOffset != 0 {
			return snapshotCorrupt("single DATA posting has an array")
		}
		return s.validateDataTailHint(off, 0, 0)
	}
	last, array, used, err := s.arrayTail(ctx, d.entryArrayOffset, d.nEntries-1)
	if err != nil {
		return err
	}
	if last <= d.entryOffset || last > s.maxEntry {
		return snapshotCorrupt("DATA tail exceeds committed population")
	}
	if err := s.validateDataTailHint(off, array, used); err != nil {
		return err
	}
	return s.validateEntry(last)
}

func (s *IndexedSnapshot) validateEntry(off uint64) error {
	if err := s.checkOffset(off); err != nil {
		return err
	}
	e, err := s.reader.readEntryHeaderAt(off)
	if err != nil {
		return err
	}
	return s.checkObject(off, e.object.size)
}

func (s *IndexedSnapshot) bucket(hash, off, size uint64, typ uint8) (hashItem, error) {
	if off < objectHeaderSize || size < hashItemSize || size%hashItemSize != 0 {
		return hashItem{}, snapshotCorrupt("native hash index unavailable")
	}
	if err := s.checkObject(off-objectHeaderSize, size+objectHeaderSize); err != nil {
		return hashItem{}, err
	}
	var buf [objectHeaderSize]byte
	if err := s.reader.readAt(buf[:], off-objectHeaderSize); err != nil {
		return hashItem{}, err
	}
	h, err := parseObjectHeader(buf[:])
	if err != nil {
		return hashItem{}, err
	}
	if h.typ != typ || h.size != size+objectHeaderSize {
		return hashItem{}, snapshotCorrupt("invalid hash table object")
	}
	if err := s.reader.readAt(buf[:hashItemSize], off+(hash%(size/hashItemSize))*hashItemSize); err != nil {
		return hashItem{}, err
	}
	return parseHashItem(buf[:]), nil
}

func (s *IndexedSnapshot) findField(ctx context.Context, name []byte) (uint64, error) {
	if err := snapshotFieldName(name); err != nil {
		return 0, err
	}
	r := s.reader
	hash := r.hash(name)
	bucket, err := s.bucket(hash, r.header.fieldHashTableOffset, r.header.fieldHashTableSize, objectTypeFieldHashTable)
	if err != nil {
		return 0, err
	}
	for off := bucket.head; off != 0; {
		if err := ctx.Err(); err != nil {
			return 0, err
		}
		if err := s.checkOffset(off); err != nil {
			return 0, err
		}
		buf, err := r.readSlice(off, fieldObjectHeaderSize)
		if err != nil {
			return 0, err
		}
		h, err := parseFieldHeader(buf)
		if err != nil {
			return 0, err
		}
		if h.object.typ != objectTypeField || h.object.size < fieldObjectHeaderSize {
			return 0, snapshotCorrupt("invalid FIELD object")
		}
		if err := s.checkObject(off, h.object.size); err != nil {
			return 0, err
		}
		value, err := r.readSlice(off+fieldObjectHeaderSize, h.object.size-fieldObjectHeaderSize)
		if err != nil {
			return 0, err
		}
		if h.nextHashOffset != 0 && h.nextHashOffset <= off {
			return 0, snapshotCorrupt("invalid FIELD hash chain progress")
		}
		if h.nextHashOffset > s.maxObject {
			return 0, snapshotCorrupt("unpublished FIELD hash link")
		}
		if h.hash == hash && bytes.Equal(name, value) {
			if h.headDataOffset == 0 {
				return 0, snapshotCorrupt("FIELD has no DATA chain")
			}
			if err := s.checkOffset(h.headDataOffset); err != nil {
				return 0, err
			}
			return h.headDataOffset, nil
		}
		off = h.nextHashOffset
	}
	return 0, nil
}

func (s *IndexedSnapshot) findData(ctx context.Context, name, value []byte, capture bool) (uint64, dataHeader, error) {
	if err := snapshotFieldName(name); err != nil {
		return 0, dataHeader{}, err
	}
	r := s.reader
	payload := make([]byte, 0, len(name)+len(value)+1)
	payload = append(payload, name...)
	payload = append(payload, '=')
	payload = append(payload, value...)
	hash := r.hash(payload)
	bucket, err := s.bucket(hash, r.header.dataHashTableOffset, r.header.dataHashTableSize, objectTypeDataHashTable)
	if err != nil {
		return 0, dataHeader{}, err
	}
	for off := bucket.head; off != 0; {
		if err := ctx.Err(); err != nil {
			return 0, dataHeader{}, err
		}
		if !capture && off > s.maxObject {
			break
		}
		h, err := s.readData(off)
		if err != nil {
			return 0, h, err
		}
		if h.nextHashOffset != 0 && h.nextHashOffset <= off {
			return 0, h, snapshotCorrupt("invalid DATA hash chain progress")
		}
		if capture && h.nextHashOffset > s.maxObject {
			return 0, h, snapshotCorrupt("unpublished DATA hash link")
		}
		if h.hash == hash {
			equal := false
			err = r.visitDataPayloadWithHeader(off, h.object, func(actual []byte) error { equal = bytes.Equal(payload, actual); return nil })
			if err != nil {
				return 0, h, err
			}
			if equal {
				return off, h, nil
			}
		}
		off = h.nextHashOffset
	}
	return 0, dataHeader{}, nil
}

func (s *IndexedSnapshot) validateDataTailHint(off, array, used uint64) error {
	if !s.reader.header.isCompact() {
		return nil
	}
	var buf [8]byte
	if err := s.reader.readAt(buf[:], off+dataObjectHeaderSize); err != nil {
		return err
	}
	if uint64(binary.LittleEndian.Uint32(buf[:4])) != array || uint64(binary.LittleEndian.Uint32(buf[4:])) != used {
		return snapshotCorrupt("DATA tail hint disagrees with committed count")
	}
	return nil
}

func snapshotFieldName(name []byte) error {
	if len(name) == 0 || bytes.IndexByte(name, '=') >= 0 {
		return errors.New("journal: invalid snapshot field name")
	}
	return nil
}
