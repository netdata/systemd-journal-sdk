package journal

import (
	"fmt"
	"math/bits"
)

const (
	objectCompressedMask      = objectCompressedXZ | objectCompressedLZ4 | objectCompressedZSTD
	compatibleSupportedMask   = compatibleSealed | compatibleTailEntryBootID | compatibleSealedContinuous
	graphTagObjectSize        = objectHeaderSize + 8 + 8 + tagLength
	graphInvalidObjectType    = 0
	graphMaxSupportedObjectID = objectTypeTag
)

type graphDataObject struct {
	hash                   uint64
	nextHashOffset         uint64
	nextFieldOffset        uint64
	entryOffset            uint64
	entryArrayOffset       uint64
	nEntries               uint64
	tailEntryArrayOffset   uint32
	tailEntryArrayNEntries uint32
}

type graphEntryObject struct {
	seqnum    uint64
	realtime  uint64
	monotonic uint64
	bootID    UUID
	items     []uint64
}

type graphEntryArray struct {
	next  uint64
	items []uint64
}

type graphWalkState struct {
	entrySeqnum       uint64
	entryMonotonic    uint64
	entryRealtime     uint64
	lastTagRealtime   uint64
	entryBootID       UUID
	entrySeqnumSet    bool
	entryMonotonicSet bool
	entryRealtimeSet  bool
}

type graphVerifier struct {
	source              verifyByteSource
	header              journalHeader
	compacted           bool
	spans               map[uint64]objectHeader
	order               []uint64
	dataObjects         map[uint64]graphDataObject
	fieldObjects        map[uint64]struct{}
	entryObjects        map[uint64]graphEntryObject
	entryArrays         map[uint64]graphEntryArray
	counts              map[uint8]uint64
	mainEntryArrayFound bool
}

func verifyObjectGraph(source verifyByteSource) error {
	v := &graphVerifier{
		source:       source,
		spans:        make(map[uint64]objectHeader),
		dataObjects:  make(map[uint64]graphDataObject),
		fieldObjects: make(map[uint64]struct{}),
		entryObjects: make(map[uint64]graphEntryObject),
		entryArrays:  make(map[uint64]graphEntryArray),
		counts:       make(map[uint8]uint64),
	}
	if err := v.readHeader(); err != nil {
		return err
	}
	if err := v.walkObjects(); err != nil {
		return err
	}
	if err := v.validateHeaderCounts(); err != nil {
		return err
	}
	if err := v.validateMainEntryArrayPresence(); err != nil {
		return err
	}
	if err := v.validateTailMetadata(); err != nil {
		return err
	}
	if err := v.validateGlobalEntryArray(); err != nil {
		return err
	}
	return v.validateDataHashTable()
}

func (v *graphVerifier) readHeader() error {
	if v.source.Len() < headerMinSize {
		return fmt.Errorf("file too small")
	}
	header, err := verifySourceHeader(v.source)
	if err != nil {
		return fmt.Errorf("invalid header: %w", err)
	}
	v.header = header
	v.compacted = header.isCompact()
	return v.validateHeader()
}

func (v *graphVerifier) validateHeader() error {
	if v.header.headerSize < headerMinSize {
		return fmt.Errorf("invalid header_size %d", v.header.headerSize)
	}
	if err := v.validateHeaderBounds(); err != nil {
		return err
	}
	if v.header.state != stateOffline && v.header.state != stateOnline && v.header.state != stateArchived {
		return fmt.Errorf("invalid journal state %d", v.header.state)
	}
	if v.header.compatibleFlags&^compatibleSupportedMask != 0 {
		return fmt.Errorf("unsupported compatible flags 0x%x", v.header.compatibleFlags)
	}
	if err := v.validateReservedHeaderBytes(); err != nil {
		return err
	}
	if v.compacted && v.source.Len() > journalCompactSizeMax {
		return fmt.Errorf("compact journal exceeds 32-bit size limit")
	}
	return nil
}

func (v *graphVerifier) validateHeaderBounds() error {
	if v.header.headerSize > v.source.Len() {
		return fmt.Errorf("header_size %d exceeds file size", v.header.headerSize)
	}
	if v.header.headerSize%objectAlignment != 0 {
		return fmt.Errorf("header_size %d is not aligned", v.header.headerSize)
	}
	if v.header.arenaSize > v.source.Len()-v.header.headerSize {
		return fmt.Errorf("header_size + arena_size exceeds file size")
	}
	return nil
}

func (v *graphVerifier) validateReservedHeaderBytes() error {
	for i := 17; i < 24; i++ {
		value, err := verifySourceByte(v.source, uint64(i))
		if err != nil {
			return err
		}
		if value != 0 {
			return fmt.Errorf("reserved header bytes are non-zero")
		}
	}
	return nil
}

func (v *graphVerifier) walkObjects() error {
	tail, done, err := v.objectWalkBounds()
	if err != nil || done {
		return err
	}

	offset := v.header.headerSize
	state := graphWalkState{}

	for {
		obj, alignedSize, err := v.readGraphObject(offset, tail)
		if err != nil {
			return err
		}

		v.recordObject(offset, obj)
		if err := v.processGraphObject(offset, obj, &state); err != nil {
			return err
		}

		if offset == tail {
			break
		}
		offset += alignedSize
	}

	return v.validateWalkResult(tail, state)
}

func (v *graphVerifier) objectWalkBounds() (uint64, bool, error) {
	tail := v.header.tailObjectOffset
	if tail == 0 {
		if v.header.nObjects != 0 {
			return 0, false, fmt.Errorf("tail_object_offset is zero with objects recorded")
		}
		return 0, true, nil
	}
	if tail < v.header.headerSize {
		return 0, false, fmt.Errorf("tail_object_offset is before header_size")
	}
	return tail, false, nil
}

func (v *graphVerifier) readGraphObject(offset uint64, tail uint64) (objectHeader, uint64, error) {
	if offset > tail {
		return objectHeader{}, 0, fmt.Errorf("object walk skipped past tail_object_offset")
	}
	if offset > v.source.Len()-objectHeaderSize {
		return objectHeader{}, 0, fmt.Errorf("object header at offset %d exceeds file bounds", offset)
	}
	typ, err := verifySourceByte(v.source, offset)
	if err != nil {
		return objectHeader{}, 0, err
	}
	flag, err := verifySourceByte(v.source, offset+1)
	if err != nil {
		return objectHeader{}, 0, err
	}
	size, err := verifySourceU64(v.source, offset+8)
	if err != nil {
		return objectHeader{}, 0, err
	}
	obj := objectHeader{
		typ:  typ,
		flag: flag,
		size: size,
	}
	alignedSize := align8(obj.size)
	if err := v.validateGraphObject(offset, obj, alignedSize); err != nil {
		return objectHeader{}, 0, err
	}
	return obj, alignedSize, nil
}

func (v *graphVerifier) validateGraphObject(offset uint64, obj objectHeader, alignedSize uint64) error {
	if obj.typ == graphInvalidObjectType && obj.size == 0 {
		return fmt.Errorf("zero object before tail at offset %d", offset)
	}
	if obj.typ < objectTypeData || obj.typ > graphMaxSupportedObjectID {
		return fmt.Errorf("unknown object type %d at offset %d", obj.typ, offset)
	}
	if obj.size < objectHeaderSize {
		return fmt.Errorf("object size %d too small at offset %d", obj.size, offset)
	}
	if alignedSize < obj.size || alignedSize == 0 {
		return fmt.Errorf("object size %d overflows alignment at offset %d", obj.size, offset)
	}
	if alignedSize > v.source.Len()-offset {
		return fmt.Errorf("object at offset %d exceeds file bounds", offset)
	}
	if offset%objectAlignment != 0 {
		return fmt.Errorf("object offset %d is not aligned", offset)
	}
	return v.validateGraphObjectFlags(offset, obj)
}

func (v *graphVerifier) validateGraphObjectFlags(offset uint64, obj objectHeader) error {
	flags := obj.flag
	if flags&^objectCompressedMask != 0 {
		return fmt.Errorf("object at offset %d has unknown flags 0x%x", offset, flags)
	}
	if bits.OnesCount8(flags&objectCompressedMask) > 1 {
		return fmt.Errorf("object at offset %d has multiple compression flags", offset)
	}
	if obj.typ != objectTypeData && flags != 0 {
		return fmt.Errorf("object type %d at offset %d has compression flags", obj.typ, offset)
	}
	return v.validateGraphCompressionFlag(offset, flags)
}

func (v *graphVerifier) validateGraphCompressionFlag(offset uint64, flags uint8) error {
	if flags&objectCompressedXZ != 0 && v.header.incompatibleFlags&incompatibleCompressedXZ == 0 {
		return fmt.Errorf("XZ DATA object without matching header flag at offset %d", offset)
	}
	if flags&objectCompressedLZ4 != 0 && v.header.incompatibleFlags&incompatibleCompressedLZ4 == 0 {
		return fmt.Errorf("LZ4 DATA object without matching header flag at offset %d", offset)
	}
	if flags&objectCompressedZSTD != 0 && v.header.incompatibleFlags&incompatibleCompressedZSTD == 0 {
		return fmt.Errorf("ZSTD DATA object without matching header flag at offset %d", offset)
	}
	return nil
}

func (v *graphVerifier) recordObject(offset uint64, obj objectHeader) {
	v.spans[offset] = obj
	v.order = append(v.order, offset)
	v.counts[obj.typ]++
}

func (v *graphVerifier) processGraphObject(offset uint64, obj objectHeader, state *graphWalkState) error {
	switch obj.typ {
	case objectTypeData:
		return v.parseData(offset, obj)
	case objectTypeField:
		return v.parseField(offset, obj)
	case objectTypeEntry:
		return v.processEntryObject(offset, obj, state)
	case objectTypeDataHashTable, objectTypeFieldHashTable:
		return v.parseHashTable(offset, obj)
	case objectTypeEntryArray:
		return v.processEntryArrayObject(offset, obj)
	case objectTypeTag:
		return v.processTagObject(offset, obj, state)
	}
	return nil
}

func (v *graphVerifier) processEntryObject(offset uint64, obj objectHeader, state *graphWalkState) error {
	entry, err := v.parseEntry(offset, obj)
	if err != nil {
		return err
	}
	if v.header.compatibleFlags&compatibleSealed != 0 && v.counts[objectTypeTag] == 0 {
		return fmt.Errorf("first entry before first tag at offset %d", offset)
	}
	if entry.realtime < state.lastTagRealtime {
		return fmt.Errorf("older entry after newer tag at offset %d", offset)
	}
	return v.updateEntryWalkState(offset, entry, state)
}

func (v *graphVerifier) updateEntryWalkState(offset uint64, entry graphEntryObject, state *graphWalkState) error {
	if !state.entrySeqnumSet && entry.seqnum != v.header.headEntrySeqnum {
		return fmt.Errorf("head entry seqnum mismatch at offset %d", offset)
	}
	if state.entrySeqnumSet && state.entrySeqnum >= entry.seqnum {
		return fmt.Errorf("entry seqnum out of sync at offset %d", offset)
	}
	state.entrySeqnum = entry.seqnum
	state.entrySeqnumSet = true
	if state.entryMonotonicSet && entry.bootID == state.entryBootID && state.entryMonotonic > entry.monotonic {
		return fmt.Errorf("entry monotonic out of sync at offset %d", offset)
	}
	state.entryMonotonic = entry.monotonic
	state.entryBootID = entry.bootID
	state.entryMonotonicSet = true
	if !state.entryRealtimeSet && entry.realtime != v.header.headEntryRealtime {
		return fmt.Errorf("head entry realtime mismatch at offset %d", offset)
	}
	state.entryRealtime = entry.realtime
	state.entryRealtimeSet = true
	return nil
}

func (v *graphVerifier) processEntryArrayObject(offset uint64, obj objectHeader) error {
	if err := v.parseEntryArray(offset, obj); err != nil {
		return err
	}
	if offset != v.header.entryArrayOffset {
		return nil
	}
	if v.mainEntryArrayFound {
		return fmt.Errorf("more than one main entry array")
	}
	v.mainEntryArrayFound = true
	return nil
}

func (v *graphVerifier) processTagObject(offset uint64, obj objectHeader, state *graphWalkState) error {
	if v.header.compatibleFlags&compatibleSealed == 0 {
		return fmt.Errorf("TAG object in unsealed file")
	}
	if obj.size != graphTagObjectSize {
		return fmt.Errorf("invalid TAG size at offset %d", offset)
	}
	seqnum, err := verifySourceU64(v.source, offset+16)
	if err != nil {
		return err
	}
	if seqnum != v.counts[objectTypeTag] {
		return fmt.Errorf("TAG seqnum mismatch at offset %d", offset)
	}
	if state.entryRealtimeSet {
		state.lastTagRealtime = state.entryRealtime
	}
	return nil
}

func (v *graphVerifier) validateWalkResult(tail uint64, state graphWalkState) error {
	if len(v.order) == 0 || v.order[len(v.order)-1] != tail {
		return fmt.Errorf("tail_object_offset does not point to walked tail")
	}
	if state.entrySeqnumSet && state.entrySeqnum != v.header.tailEntrySeqnum {
		return fmt.Errorf("tail_entry_seqnum mismatch")
	}
	if state.entryMonotonicSet &&
		v.header.compatibleFlags&compatibleTailEntryBootID != 0 &&
		state.entryBootID == v.header.tailEntryBootID &&
		state.entryMonotonic != v.header.tailEntryMonotonic {
		return fmt.Errorf("tail_entry_monotonic mismatch")
	}
	if state.entryRealtimeSet && state.entryRealtime != v.header.tailEntryRealtime {
		return fmt.Errorf("tail_entry_realtime mismatch")
	}
	return nil
}

func (v *graphVerifier) parseData(offset uint64, obj objectHeader) error {
	payloadOffset := v.dataObjectPayloadOffset()
	if obj.size <= payloadOffset {
		return fmt.Errorf("DATA object at offset %d has no payload", offset)
	}
	payload, err := v.source.Slice(offset+payloadOffset, obj.size-payloadOffset)
	if err != nil {
		return err
	}
	hashPayload, err := v.dataHashPayload(offset, obj.flag, payload)
	if err != nil {
		return err
	}
	storedHash, err := verifySourceU64(v.source, offset+16)
	if err != nil {
		return err
	}
	if computedHash := v.hash(hashPayload); storedHash != computedHash {
		return fmt.Errorf("DATA hash mismatch at offset %d: %#x != %#x", offset, storedHash, computedHash)
	}
	data, err := v.readGraphDataObject(offset, storedHash)
	if err != nil {
		return err
	}
	if err := v.validateDataObject(offset, data); err != nil {
		return err
	}
	v.dataObjects[offset] = data
	return nil
}

func (v *graphVerifier) dataObjectPayloadOffset() uint64 {
	if v.compacted {
		return compactDataObjectHeaderSize
	}
	return dataObjectHeaderSize
}

func (v *graphVerifier) dataHashPayload(offset uint64, flags uint8, payload []byte) ([]byte, error) {
	if flags == 0 {
		return payload, nil
	}
	hashPayload, err := decompressDataPayload(flags, payload)
	if err != nil {
		return nil, fmt.Errorf("DATA decompression failed at offset %d: %w", offset, err)
	}
	return hashPayload, nil
}

func (v *graphVerifier) readGraphDataObject(offset uint64, storedHash uint64) (graphDataObject, error) {
	entryOffset, err := verifySourceU64(v.source, offset+40)
	if err != nil {
		return graphDataObject{}, err
	}
	nEntries, err := verifySourceU64(v.source, offset+56)
	if err != nil {
		return graphDataObject{}, err
	}
	nextHashOffset, err := verifySourceU64(v.source, offset+24)
	if err != nil {
		return graphDataObject{}, err
	}
	nextFieldOffset, err := verifySourceU64(v.source, offset+32)
	if err != nil {
		return graphDataObject{}, err
	}
	entryArrayOffset, err := verifySourceU64(v.source, offset+48)
	if err != nil {
		return graphDataObject{}, err
	}
	if (entryOffset == 0) != (nEntries == 0) {
		return graphDataObject{}, fmt.Errorf("DATA object at offset %d has bad n_entries", offset)
	}
	data := graphDataObject{
		hash:             storedHash,
		nextHashOffset:   nextHashOffset,
		nextFieldOffset:  nextFieldOffset,
		entryOffset:      entryOffset,
		entryArrayOffset: entryArrayOffset,
		nEntries:         nEntries,
	}
	if v.compacted {
		data.tailEntryArrayOffset, err = verifySourceU32(v.source, offset+64)
		if err != nil {
			return graphDataObject{}, err
		}
		data.tailEntryArrayNEntries, err = verifySourceU32(v.source, offset+68)
		if err != nil {
			return graphDataObject{}, err
		}
	}
	return data, nil
}

func (v *graphVerifier) validateDataObject(offset uint64, data graphDataObject) error {
	if err := v.validOffset(data.nextHashOffset, fmt.Sprintf("DATA %d next_hash_offset", offset)); err != nil {
		return err
	}
	if err := v.validOffset(data.nextFieldOffset, fmt.Sprintf("DATA %d next_field_offset", offset)); err != nil {
		return err
	}
	if err := v.validOffset(data.entryOffset, fmt.Sprintf("DATA %d entry_offset", offset)); err != nil {
		return err
	}
	if err := v.validOffset(data.entryArrayOffset, fmt.Sprintf("DATA %d entry_array_offset", offset)); err != nil {
		return err
	}
	if data.nEntries < 2 && data.entryArrayOffset != 0 {
		return fmt.Errorf("DATA object at offset %d has unexpected entry array", offset)
	}
	if data.nEntries >= 2 && data.entryArrayOffset == 0 {
		return fmt.Errorf("DATA object at offset %d is missing entry array", offset)
	}
	return nil
}

func (v *graphVerifier) parseField(offset uint64, obj objectHeader) error {
	if obj.size <= fieldObjectHeaderSize {
		return fmt.Errorf("FIELD object at offset %d has no payload", offset)
	}
	payload, err := v.source.Slice(offset+fieldObjectHeaderSize, obj.size-fieldObjectHeaderSize)
	if err != nil {
		return err
	}
	storedHash, err := verifySourceU64(v.source, offset+16)
	if err != nil {
		return err
	}
	if computedHash := v.hash(payload); storedHash != computedHash {
		return fmt.Errorf("FIELD hash mismatch at offset %d: %#x != %#x", offset, storedHash, computedHash)
	}
	nextHashOffset, err := verifySourceU64(v.source, offset+24)
	if err != nil {
		return err
	}
	headDataOffset, err := verifySourceU64(v.source, offset+32)
	if err != nil {
		return err
	}
	if err := v.validOffset(nextHashOffset, fmt.Sprintf("FIELD %d next_hash_offset", offset)); err != nil {
		return err
	}
	if err := v.validOffset(headDataOffset, fmt.Sprintf("FIELD %d head_data_offset", offset)); err != nil {
		return err
	}
	v.fieldObjects[offset] = struct{}{}
	return nil
}

func (v *graphVerifier) parseEntry(offset uint64, obj objectHeader) (graphEntryObject, error) {
	itemSize := uint64(regularEntryItemSize)
	if v.compacted {
		itemSize = compactEntryItemSize
	}
	if obj.size < entryObjectHeaderSize {
		return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d is too small", offset)
	}
	if (obj.size-entryObjectHeaderSize)%itemSize != 0 {
		return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d has unaligned items", offset)
	}
	seqnum, err := verifySourceU64(v.source, offset+16)
	if err != nil {
		return graphEntryObject{}, err
	}
	realtime, err := verifySourceU64(v.source, offset+24)
	if err != nil {
		return graphEntryObject{}, err
	}
	monotonic, err := verifySourceU64(v.source, offset+32)
	if err != nil {
		return graphEntryObject{}, err
	}
	bootID, err := verifySourceUUID(v.source, offset+40)
	if err != nil {
		return graphEntryObject{}, err
	}
	entry := graphEntryObject{
		seqnum:    seqnum,
		realtime:  realtime,
		monotonic: monotonic,
		bootID:    bootID,
	}
	if entry.seqnum == 0 {
		return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d has zero seqnum", offset)
	}
	if entry.realtime == 0 {
		return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d has zero realtime", offset)
	}
	for itemOffset := offset + entryObjectHeaderSize; itemOffset < offset+obj.size; itemOffset += itemSize {
		var item uint64
		if v.compacted {
			value, err := verifySourceU32(v.source, itemOffset)
			if err != nil {
				return graphEntryObject{}, err
			}
			item = uint64(value)
		} else {
			value, err := verifySourceU64(v.source, itemOffset)
			if err != nil {
				return graphEntryObject{}, err
			}
			item = value
		}
		if item == 0 {
			return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d has zero item", offset)
		}
		if err := v.validOffset(item, fmt.Sprintf("ENTRY %d item", offset)); err != nil {
			return graphEntryObject{}, err
		}
		entry.items = append(entry.items, item)
	}
	if len(entry.items) == 0 {
		return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d has no items", offset)
	}
	v.entryObjects[offset] = entry
	return entry, nil
}

func (v *graphVerifier) parseHashTable(offset uint64, obj objectHeader) error {
	if obj.size < objectHeaderSize+hashItemSize {
		return fmt.Errorf("hash table at offset %d is too small", offset)
	}
	if (obj.size-objectHeaderSize)%hashItemSize != 0 {
		return fmt.Errorf("hash table at offset %d has unaligned items", offset)
	}
	var tableOffset, tableSize uint64
	if obj.typ == objectTypeDataHashTable {
		tableOffset = v.header.dataHashTableOffset
		tableSize = v.header.dataHashTableSize
	} else {
		tableOffset = v.header.fieldHashTableOffset
		tableSize = v.header.fieldHashTableSize
	}
	if tableOffset != offset+objectHeaderSize {
		return fmt.Errorf("hash table header offset mismatch at offset %d", offset)
	}
	if tableSize != obj.size-objectHeaderSize {
		return fmt.Errorf("hash table header size mismatch at offset %d", offset)
	}
	for itemOffset := offset + objectHeaderSize; itemOffset < offset+obj.size; itemOffset += hashItemSize {
		head, err := verifySourceU64(v.source, itemOffset)
		if err != nil {
			return err
		}
		tail, err := verifySourceU64(v.source, itemOffset+8)
		if err != nil {
			return err
		}
		if (head == 0) != (tail == 0) {
			return fmt.Errorf("hash bucket head/tail mismatch")
		}
		if err := v.validOffset(head, "hash bucket head"); err != nil {
			return err
		}
		if err := v.validOffset(tail, "hash bucket tail"); err != nil {
			return err
		}
	}
	return nil
}

func (v *graphVerifier) parseEntryArray(offset uint64, obj objectHeader) error {
	itemSize := uint64(regularOffsetArrayItemSize)
	if v.compacted {
		itemSize = compactOffsetArrayItemSize
	}
	if obj.size < offsetArrayObjectHeaderSize+itemSize {
		return fmt.Errorf("ENTRY_ARRAY object at offset %d is too small", offset)
	}
	if (obj.size-offsetArrayObjectHeaderSize)%itemSize != 0 {
		return fmt.Errorf("ENTRY_ARRAY object at offset %d has unaligned items", offset)
	}
	array := graphEntryArray{
		next: 0,
	}
	next, err := verifySourceU64(v.source, offset+16)
	if err != nil {
		return err
	}
	array.next = next
	if err := v.validOffset(array.next, fmt.Sprintf("ENTRY_ARRAY %d next", offset)); err != nil {
		return err
	}
	for itemOffset := offset + offsetArrayObjectHeaderSize; itemOffset < offset+obj.size; itemOffset += itemSize {
		var item uint64
		if v.compacted {
			value, err := verifySourceU32(v.source, itemOffset)
			if err != nil {
				return err
			}
			item = uint64(value)
		} else {
			value, err := verifySourceU64(v.source, itemOffset)
			if err != nil {
				return err
			}
			item = value
		}
		if item != 0 {
			if err := v.validOffset(item, fmt.Sprintf("ENTRY_ARRAY %d item", offset)); err != nil {
				return err
			}
		}
		array.items = append(array.items, item)
	}
	v.entryArrays[offset] = array
	return nil
}

func (v *graphVerifier) validateHeaderCounts() error {
	expected := map[string]uint64{
		"n_objects":      uint64(len(v.order)),
		"n_entries":      v.counts[objectTypeEntry],
		"n_data":         v.counts[objectTypeData],
		"n_fields":       v.counts[objectTypeField],
		"n_tags":         v.counts[objectTypeTag],
		"n_entry_arrays": v.counts[objectTypeEntryArray],
	}
	actual := map[string]uint64{
		"n_objects":      v.header.nObjects,
		"n_entries":      v.header.nEntries,
		"n_data":         v.header.nData,
		"n_fields":       v.header.nFields,
		"n_tags":         v.header.nTags,
		"n_entry_arrays": v.header.nEntryArrays,
	}
	fieldEnds := map[string]int{
		"n_objects":      152,
		"n_entries":      160,
		"n_data":         216,
		"n_fields":       224,
		"n_tags":         232,
		"n_entry_arrays": 240,
	}
	for field, value := range expected {
		if verifySourceHasHeaderField(v.source, v.header.headerSize, fieldEnds[field]) && actual[field] != value {
			return fmt.Errorf("header %s mismatch: got %d, walked %d", field, actual[field], value)
		}
	}
	return nil
}

func (v *graphVerifier) validateMainEntryArrayPresence() error {
	if v.header.entryArrayOffset != 0 && !v.mainEntryArrayFound {
		return fmt.Errorf("missing main entry array")
	}
	if v.header.nEntries != 0 && v.header.entryArrayOffset == 0 {
		return fmt.Errorf("entry_array_offset is zero with entries recorded")
	}
	return nil
}

func (v *graphVerifier) validateTailMetadata() error {
	if len(v.entryObjects) == 0 {
		if v.header.nEntries != 0 {
			return fmt.Errorf("entries recorded but no ENTRY objects found")
		}
		return nil
	}
	headOffset, tailOffset, head, tail := v.headTailEntries()
	if err := v.validateHeadTailEntries(head, tail); err != nil {
		return err
	}
	if verifySourceHasHeaderField(v.source, v.header.headerSize, 272) && v.header.tailEntryOffset != tailOffset {
		return fmt.Errorf("tail_entry_offset mismatch")
	}
	if headOffset == 0 {
		return fmt.Errorf("head entry offset is zero")
	}
	return nil
}

func (v *graphVerifier) headTailEntries() (uint64, uint64, graphEntryObject, graphEntryObject) {
	var headOffset, tailOffset uint64
	var head, tail graphEntryObject
	for offset, entry := range v.entryObjects {
		if headOffset == 0 || entry.seqnum < head.seqnum {
			headOffset = offset
			head = entry
		}
		if tailOffset == 0 || entry.seqnum > tail.seqnum {
			tailOffset = offset
			tail = entry
		}
	}
	return headOffset, tailOffset, head, tail
}

func (v *graphVerifier) validateHeadTailEntries(head, tail graphEntryObject) error {
	if v.header.headEntrySeqnum != head.seqnum {
		return fmt.Errorf("head_entry_seqnum mismatch")
	}
	if v.header.tailEntrySeqnum != tail.seqnum {
		return fmt.Errorf("tail_entry_seqnum mismatch")
	}
	if v.header.headEntryRealtime != head.realtime {
		return fmt.Errorf("head_entry_realtime mismatch")
	}
	if v.header.tailEntryRealtime != tail.realtime {
		return fmt.Errorf("tail_entry_realtime mismatch")
	}
	return v.validateTailEntryBootMetadata(tail)
}

func (v *graphVerifier) validateTailEntryBootMetadata(tail graphEntryObject) error {
	if v.header.compatibleFlags&compatibleTailEntryBootID == 0 {
		return nil
	}
	if v.header.tailEntryMonotonic != tail.monotonic {
		return fmt.Errorf("tail_entry_monotonic mismatch")
	}
	if v.header.tailEntryBootID != tail.bootID {
		return fmt.Errorf("tail_entry_boot_id mismatch")
	}
	return nil
}

func (v *graphVerifier) validateGlobalEntryArray() error {
	entries, err := v.walkEntryArrayChain(v.header.entryArrayOffset, v.header.nEntries, "global entry array")
	if err != nil {
		return err
	}
	if uint64(len(entries)) != v.header.nEntries {
		return fmt.Errorf("global entry array count mismatch")
	}
	var last uint64
	for idx, entryOffset := range entries {
		if entryOffset <= last {
			return fmt.Errorf("global entry array is not sorted")
		}
		if _, ok := v.entryObjects[entryOffset]; !ok {
			return fmt.Errorf("global entry array references missing ENTRY")
		}
		last = entryOffset
		if err := v.validateEntryDataLinks(entryOffset, idx+1 == len(entries)); err != nil {
			return err
		}
	}
	return nil
}

func (v *graphVerifier) validateDataHashTable() error {
	tableOffset := v.header.dataHashTableOffset
	tableSize := v.header.dataHashTableSize
	if tableOffset == 0 || tableSize == 0 {
		return nil
	}
	bucketCount := tableSize / hashItemSize
	for bucketIndex := uint64(0); bucketIndex < bucketCount; bucketIndex++ {
		itemOffset := tableOffset + bucketIndex*hashItemSize
		current, err := verifySourceU64(v.source, itemOffset)
		if err != nil {
			return err
		}
		tail, err := verifySourceU64(v.source, itemOffset+8)
		if err != nil {
			return err
		}
		last := uint64(0)
		seen := make(map[uint64]struct{})
		for current != 0 {
			if _, ok := seen[current]; ok {
				return fmt.Errorf("data hash chain cycle")
			}
			seen[current] = struct{}{}
			obj, ok := v.dataObjects[current]
			if !ok {
				return fmt.Errorf("data hash chain references missing DATA")
			}
			if obj.hash%bucketCount != bucketIndex {
				return fmt.Errorf("data hash bucket mismatch")
			}
			if err := v.validateDataEntryArray(current, obj); err != nil {
				return err
			}
			if obj.nextHashOffset != 0 && obj.nextHashOffset <= current {
				return fmt.Errorf("data hash chain points backwards")
			}
			last = current
			current = obj.nextHashOffset
		}
		if last != tail {
			return fmt.Errorf("data hash bucket tail mismatch")
		}
	}
	return nil
}

func (v *graphVerifier) validateEntryDataLinks(entryOffset uint64, lastEntry bool) error {
	entry := v.entryObjects[entryOffset]
	for _, dataOffset := range entry.items {
		data, ok := v.dataObjects[dataOffset]
		if !ok {
			return fmt.Errorf("entry references missing DATA object")
		}
		if !v.dataObjectInHashTable(dataOffset, data.hash) {
			return fmt.Errorf("entry DATA object missing from hash table")
		}
		referencesEntry, err := v.dataReferencesEntry(data, entryOffset)
		if err != nil {
			return err
		}
		if !referencesEntry && !lastEntry {
			return fmt.Errorf("entry not referenced by linked DATA object")
		}
	}
	return nil
}

func (v *graphVerifier) validateDataEntryArray(dataOffset uint64, data graphDataObject) error {
	if data.nEntries == 0 {
		return nil
	}
	if _, ok := v.entryObjects[data.entryOffset]; !ok {
		return fmt.Errorf("DATA inline entry is missing")
	}
	last := data.entryOffset
	if data.entryArrayOffset != 0 && data.nEntries < 2 {
		return fmt.Errorf("DATA entry array present with fewer than two entries")
	}
	entries, err := v.walkEntryArrayChain(data.entryArrayOffset, data.nEntries-1, fmt.Sprintf("DATA %d entry array", dataOffset))
	if err != nil {
		return err
	}
	for _, entryOffset := range entries {
		if entryOffset <= last {
			return fmt.Errorf("DATA entry array is not sorted")
		}
		last = entryOffset
	}
	return nil
}

func (v *graphVerifier) walkEntryArrayChain(startOffset, usedCount uint64, label string) ([]uint64, error) {
	done, err := validateEntryArrayChainStart(startOffset, usedCount, label)
	if err != nil || done {
		return nil, err
	}
	var entries []uint64
	remaining := usedCount
	current := startOffset
	seen := make(map[uint64]struct{})
	for remaining > 0 {
		array, err := v.nextEntryArrayChainObject(current, seen, label)
		if err != nil {
			return nil, err
		}
		usedHere := uint64(len(array.items))
		if remaining < usedHere {
			usedHere = remaining
		}
		entries, err = v.appendEntryArrayChainItems(entries, array, usedHere, label)
		if err != nil {
			return nil, err
		}
		remaining -= usedHere
		if remaining == 0 {
			break
		}
		if array.next == 0 {
			return nil, fmt.Errorf("%s ended early", label)
		}
		current = array.next
	}
	return entries, nil
}

func validateEntryArrayChainStart(startOffset, usedCount uint64, label string) (bool, error) {
	if usedCount == 0 {
		if startOffset != 0 {
			return false, fmt.Errorf("%s has start offset with zero entries", label)
		}
		return true, nil
	}
	if startOffset == 0 {
		return false, fmt.Errorf("%s is missing", label)
	}
	return false, nil
}

func (v *graphVerifier) nextEntryArrayChainObject(current uint64, seen map[uint64]struct{}, label string) (graphEntryArray, error) {
	if _, ok := seen[current]; ok {
		return graphEntryArray{}, fmt.Errorf("%s has a cycle", label)
	}
	seen[current] = struct{}{}
	array, ok := v.entryArrays[current]
	if !ok {
		return graphEntryArray{}, fmt.Errorf("%s references missing ENTRY_ARRAY", label)
	}
	if array.next != 0 && array.next <= current {
		return graphEntryArray{}, fmt.Errorf("%s next pointer is not increasing", label)
	}
	return array, nil
}

func (v *graphVerifier) appendEntryArrayChainItems(entries []uint64, array graphEntryArray, usedHere uint64, label string) ([]uint64, error) {
	for i := uint64(0); i < usedHere; i++ {
		item := array.items[i]
		if item == 0 {
			return nil, fmt.Errorf("%s has zero used item", label)
		}
		if _, ok := v.entryObjects[item]; !ok {
			return nil, fmt.Errorf("%s references missing ENTRY", label)
		}
		entries = append(entries, item)
	}
	return entries, nil
}

func (v *graphVerifier) dataObjectInHashTable(dataOffset, dataHash uint64) bool {
	tableOffset := v.header.dataHashTableOffset
	tableSize := v.header.dataHashTableSize
	if tableOffset == 0 || tableSize == 0 {
		return false
	}
	bucketCount := tableSize / hashItemSize
	bucket := dataHash % bucketCount
	current, err := verifySourceU64(v.source, tableOffset+bucket*hashItemSize)
	if err != nil {
		return false
	}
	seen := make(map[uint64]struct{})
	for current != 0 {
		if _, ok := seen[current]; ok {
			return false
		}
		seen[current] = struct{}{}
		if current == dataOffset {
			return true
		}
		obj, ok := v.dataObjects[current]
		if !ok {
			return false
		}
		current = obj.nextHashOffset
	}
	return false
}

func (v *graphVerifier) dataReferencesEntry(data graphDataObject, entryOffset uint64) (bool, error) {
	if data.entryOffset == entryOffset {
		return true, nil
	}
	if data.nEntries == 0 {
		return false, nil
	}
	entries, err := v.walkEntryArrayChain(data.entryArrayOffset, data.nEntries-1, "DATA entry array lookup")
	if err != nil {
		return false, err
	}
	for _, item := range entries {
		if item == entryOffset {
			return true, nil
		}
	}
	return false, nil
}

func (v *graphVerifier) validOffset(offset uint64, label string) error {
	if offset == 0 {
		return nil
	}
	if offset%objectAlignment != 0 {
		return fmt.Errorf("%s offset %d is not aligned", label, offset)
	}
	if offset < v.header.headerSize || offset > v.header.tailObjectOffset {
		return fmt.Errorf("%s offset %d outside object range", label, offset)
	}
	return nil
}

func (v *graphVerifier) hash(payload []byte) uint64 {
	if v.header.incompatibleFlags&incompatibleKeyedHash != 0 {
		return sipHash24(v.header.fileID, payload)
	}
	return jenkinsHash64(payload)
}

func decompressDataPayload(flags uint8, payload []byte) ([]byte, error) {
	switch {
	case flags&objectCompressedZSTD != 0:
		return zstdDecompress(payload)
	case flags&objectCompressedXZ != 0:
		return xzDecompress(payload)
	case flags&objectCompressedLZ4 != 0:
		return lz4Decompress(payload)
	default:
		return payload, nil
	}
}
