package journal

import (
	"encoding/binary"
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

type graphVerifier struct {
	data                []byte
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

func verifyObjectGraph(data []byte) error {
	v := &graphVerifier{
		data:         data,
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
	if len(v.data) < headerMinSize {
		return fmt.Errorf("file too small")
	}
	header, err := parseHeader(v.data)
	if err != nil {
		return fmt.Errorf("invalid header: %w", err)
	}
	v.header = header
	v.compacted = header.isCompact()

	if v.header.headerSize < headerMinSize {
		return fmt.Errorf("invalid header_size %d", v.header.headerSize)
	}
	if v.header.headerSize > uint64(len(v.data)) {
		return fmt.Errorf("header_size %d exceeds file size", v.header.headerSize)
	}
	if v.header.headerSize%objectAlignment != 0 {
		return fmt.Errorf("header_size %d is not aligned", v.header.headerSize)
	}
	if v.header.arenaSize > uint64(len(v.data))-v.header.headerSize {
		return fmt.Errorf("header_size + arena_size exceeds file size")
	}
	if v.header.state != stateOffline && v.header.state != stateOnline && v.header.state != stateArchived {
		return fmt.Errorf("invalid journal state %d", v.header.state)
	}
	if v.header.compatibleFlags&^compatibleSupportedMask != 0 {
		return fmt.Errorf("unsupported compatible flags 0x%x", v.header.compatibleFlags)
	}
	for i := 17; i < 24; i++ {
		if v.data[i] != 0 {
			return fmt.Errorf("reserved header bytes are non-zero")
		}
	}
	if v.compacted && uint64(len(v.data)) > journalCompactSizeMax {
		return fmt.Errorf("compact journal exceeds 32-bit size limit")
	}
	return nil
}

func (v *graphVerifier) walkObjects() error {
	tail := v.header.tailObjectOffset
	if tail == 0 {
		if v.header.nObjects != 0 {
			return fmt.Errorf("tail_object_offset is zero with objects recorded")
		}
		return nil
	}
	if tail < v.header.headerSize {
		return fmt.Errorf("tail_object_offset is before header_size")
	}

	offset := v.header.headerSize
	var entrySeqnum, entryMonotonic, entryRealtime, lastTagRealtime uint64
	var entryBootID UUID
	entrySeqnumSet := false
	entryMonotonicSet := false
	entryRealtimeSet := false

	for {
		if offset > tail {
			return fmt.Errorf("object walk skipped past tail_object_offset")
		}
		if offset > uint64(len(v.data))-objectHeaderSize {
			return fmt.Errorf("object header at offset %d exceeds file bounds", offset)
		}

		flags := v.data[offset+1]
		obj := objectHeader{
			typ:  v.data[offset],
			flag: flags,
			size: binary.LittleEndian.Uint64(v.data[offset+8 : offset+16]),
		}
		alignedSize := align8(obj.size)
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
		if alignedSize > uint64(len(v.data))-offset {
			return fmt.Errorf("object at offset %d exceeds file bounds", offset)
		}
		if offset%objectAlignment != 0 {
			return fmt.Errorf("object offset %d is not aligned", offset)
		}
		if flags&^objectCompressedMask != 0 {
			return fmt.Errorf("object at offset %d has unknown flags 0x%x", offset, flags)
		}
		if bits.OnesCount8(flags&objectCompressedMask) > 1 {
			return fmt.Errorf("object at offset %d has multiple compression flags", offset)
		}
		if obj.typ != objectTypeData && flags != 0 {
			return fmt.Errorf("object type %d at offset %d has compression flags", obj.typ, offset)
		}
		if flags&objectCompressedXZ != 0 && v.header.incompatibleFlags&incompatibleCompressedXZ == 0 {
			return fmt.Errorf("XZ DATA object without matching header flag at offset %d", offset)
		}
		if flags&objectCompressedLZ4 != 0 && v.header.incompatibleFlags&incompatibleCompressedLZ4 == 0 {
			return fmt.Errorf("LZ4 DATA object without matching header flag at offset %d", offset)
		}
		if flags&objectCompressedZSTD != 0 && v.header.incompatibleFlags&incompatibleCompressedZSTD == 0 {
			return fmt.Errorf("ZSTD DATA object without matching header flag at offset %d", offset)
		}

		v.spans[offset] = obj
		v.order = append(v.order, offset)
		v.counts[obj.typ]++

		switch obj.typ {
		case objectTypeData:
			if err := v.parseData(offset, obj); err != nil {
				return err
			}
		case objectTypeField:
			if err := v.parseField(offset, obj); err != nil {
				return err
			}
		case objectTypeEntry:
			entry, err := v.parseEntry(offset, obj)
			if err != nil {
				return err
			}
			if v.header.compatibleFlags&compatibleSealed != 0 && v.counts[objectTypeTag] == 0 {
				return fmt.Errorf("first entry before first tag at offset %d", offset)
			}
			if entry.realtime < lastTagRealtime {
				return fmt.Errorf("older entry after newer tag at offset %d", offset)
			}
			if !entrySeqnumSet && entry.seqnum != v.header.headEntrySeqnum {
				return fmt.Errorf("head entry seqnum mismatch at offset %d", offset)
			}
			if entrySeqnumSet && entrySeqnum >= entry.seqnum {
				return fmt.Errorf("entry seqnum out of sync at offset %d", offset)
			}
			entrySeqnum = entry.seqnum
			entrySeqnumSet = true
			if entryMonotonicSet && entry.bootID == entryBootID && entryMonotonic > entry.monotonic {
				return fmt.Errorf("entry monotonic out of sync at offset %d", offset)
			}
			entryMonotonic = entry.monotonic
			entryBootID = entry.bootID
			entryMonotonicSet = true
			if !entryRealtimeSet && entry.realtime != v.header.headEntryRealtime {
				return fmt.Errorf("head entry realtime mismatch at offset %d", offset)
			}
			entryRealtime = entry.realtime
			entryRealtimeSet = true
		case objectTypeDataHashTable, objectTypeFieldHashTable:
			if err := v.parseHashTable(offset, obj); err != nil {
				return err
			}
		case objectTypeEntryArray:
			if err := v.parseEntryArray(offset, obj); err != nil {
				return err
			}
			if offset == v.header.entryArrayOffset {
				if v.mainEntryArrayFound {
					return fmt.Errorf("more than one main entry array")
				}
				v.mainEntryArrayFound = true
			}
		case objectTypeTag:
			if v.header.compatibleFlags&compatibleSealed == 0 {
				return fmt.Errorf("TAG object in unsealed file")
			}
			if obj.size != graphTagObjectSize {
				return fmt.Errorf("invalid TAG size at offset %d", offset)
			}
			seqnum := binary.LittleEndian.Uint64(v.data[offset+16 : offset+24])
			if seqnum != v.counts[objectTypeTag] {
				return fmt.Errorf("TAG seqnum mismatch at offset %d", offset)
			}
			if entryRealtimeSet {
				lastTagRealtime = entryRealtime
			}
		}

		if offset == tail {
			break
		}
		offset += alignedSize
	}

	if len(v.order) == 0 || v.order[len(v.order)-1] != tail {
		return fmt.Errorf("tail_object_offset does not point to walked tail")
	}
	if entrySeqnumSet && entrySeqnum != v.header.tailEntrySeqnum {
		return fmt.Errorf("tail_entry_seqnum mismatch")
	}
	if entryMonotonicSet &&
		v.header.compatibleFlags&compatibleTailEntryBootID != 0 &&
		entryBootID == v.header.tailEntryBootID &&
		entryMonotonic != v.header.tailEntryMonotonic {
		return fmt.Errorf("tail_entry_monotonic mismatch")
	}
	if entryRealtimeSet && entryRealtime != v.header.tailEntryRealtime {
		return fmt.Errorf("tail_entry_realtime mismatch")
	}
	return nil
}

func (v *graphVerifier) parseData(offset uint64, obj objectHeader) error {
	payloadOffset := uint64(dataObjectHeaderSize)
	if v.compacted {
		payloadOffset = compactDataObjectHeaderSize
	}
	if obj.size <= payloadOffset {
		return fmt.Errorf("DATA object at offset %d has no payload", offset)
	}
	payload := v.data[offset+payloadOffset : offset+obj.size]
	hashPayload := payload
	if obj.flag != 0 {
		var err error
		hashPayload, err = decompressGraphPayload(obj.flag, payload)
		if err != nil {
			return fmt.Errorf("DATA decompression failed at offset %d: %w", offset, err)
		}
	}
	storedHash := binary.LittleEndian.Uint64(v.data[offset+16 : offset+24])
	if computedHash := v.hash(hashPayload); storedHash != computedHash {
		return fmt.Errorf("DATA hash mismatch at offset %d: %#x != %#x", offset, storedHash, computedHash)
	}
	entryOffset := binary.LittleEndian.Uint64(v.data[offset+40 : offset+48])
	nEntries := binary.LittleEndian.Uint64(v.data[offset+56 : offset+64])
	if (entryOffset == 0) != (nEntries == 0) {
		return fmt.Errorf("DATA object at offset %d has bad n_entries", offset)
	}
	data := graphDataObject{
		hash:             storedHash,
		nextHashOffset:   binary.LittleEndian.Uint64(v.data[offset+24 : offset+32]),
		nextFieldOffset:  binary.LittleEndian.Uint64(v.data[offset+32 : offset+40]),
		entryOffset:      entryOffset,
		entryArrayOffset: binary.LittleEndian.Uint64(v.data[offset+48 : offset+56]),
		nEntries:         nEntries,
	}
	if v.compacted {
		data.tailEntryArrayOffset = binary.LittleEndian.Uint32(v.data[offset+64 : offset+68])
		data.tailEntryArrayNEntries = binary.LittleEndian.Uint32(v.data[offset+68 : offset+72])
	}
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
	if nEntries < 2 && data.entryArrayOffset != 0 {
		return fmt.Errorf("DATA object at offset %d has unexpected entry array", offset)
	}
	if nEntries >= 2 && data.entryArrayOffset == 0 {
		return fmt.Errorf("DATA object at offset %d is missing entry array", offset)
	}
	v.dataObjects[offset] = data
	return nil
}

func (v *graphVerifier) parseField(offset uint64, obj objectHeader) error {
	if obj.size <= fieldObjectHeaderSize {
		return fmt.Errorf("FIELD object at offset %d has no payload", offset)
	}
	payload := v.data[offset+fieldObjectHeaderSize : offset+obj.size]
	storedHash := binary.LittleEndian.Uint64(v.data[offset+16 : offset+24])
	if computedHash := v.hash(payload); storedHash != computedHash {
		return fmt.Errorf("FIELD hash mismatch at offset %d: %#x != %#x", offset, storedHash, computedHash)
	}
	nextHashOffset := binary.LittleEndian.Uint64(v.data[offset+24 : offset+32])
	headDataOffset := binary.LittleEndian.Uint64(v.data[offset+32 : offset+40])
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
	entry := graphEntryObject{
		seqnum:    binary.LittleEndian.Uint64(v.data[offset+16 : offset+24]),
		realtime:  binary.LittleEndian.Uint64(v.data[offset+24 : offset+32]),
		monotonic: binary.LittleEndian.Uint64(v.data[offset+32 : offset+40]),
	}
	copy(entry.bootID[:], v.data[offset+40:offset+56])
	if entry.seqnum == 0 {
		return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d has zero seqnum", offset)
	}
	if entry.realtime == 0 {
		return graphEntryObject{}, fmt.Errorf("ENTRY object at offset %d has zero realtime", offset)
	}
	for itemOffset := offset + entryObjectHeaderSize; itemOffset < offset+obj.size; itemOffset += itemSize {
		var item uint64
		if v.compacted {
			item = uint64(binary.LittleEndian.Uint32(v.data[itemOffset : itemOffset+4]))
		} else {
			item = binary.LittleEndian.Uint64(v.data[itemOffset : itemOffset+8])
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
		head := binary.LittleEndian.Uint64(v.data[itemOffset : itemOffset+8])
		tail := binary.LittleEndian.Uint64(v.data[itemOffset+8 : itemOffset+16])
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
		next: binary.LittleEndian.Uint64(v.data[offset+16 : offset+24]),
	}
	if err := v.validOffset(array.next, fmt.Sprintf("ENTRY_ARRAY %d next", offset)); err != nil {
		return err
	}
	for itemOffset := offset + offsetArrayObjectHeaderSize; itemOffset < offset+obj.size; itemOffset += itemSize {
		var item uint64
		if v.compacted {
			item = uint64(binary.LittleEndian.Uint32(v.data[itemOffset : itemOffset+4]))
		} else {
			item = binary.LittleEndian.Uint64(v.data[itemOffset : itemOffset+8])
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
		if headerContainsField(v.data, v.header.headerSize, fieldEnds[field]) && actual[field] != value {
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
	if v.header.compatibleFlags&compatibleTailEntryBootID != 0 {
		if v.header.tailEntryMonotonic != tail.monotonic {
			return fmt.Errorf("tail_entry_monotonic mismatch")
		}
		if v.header.tailEntryBootID != tail.bootID {
			return fmt.Errorf("tail_entry_boot_id mismatch")
		}
	}
	if headerContainsField(v.data, v.header.headerSize, 272) && v.header.tailEntryOffset != tailOffset {
		return fmt.Errorf("tail_entry_offset mismatch")
	}
	if headOffset == 0 {
		return fmt.Errorf("head entry offset is zero")
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
		current := binary.LittleEndian.Uint64(v.data[itemOffset : itemOffset+8])
		tail := binary.LittleEndian.Uint64(v.data[itemOffset+8 : itemOffset+16])
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
	if usedCount == 0 {
		if startOffset != 0 {
			return nil, fmt.Errorf("%s has start offset with zero entries", label)
		}
		return nil, nil
	}
	if startOffset == 0 {
		return nil, fmt.Errorf("%s is missing", label)
	}
	var entries []uint64
	remaining := usedCount
	current := startOffset
	seen := make(map[uint64]struct{})
	for remaining > 0 {
		if _, ok := seen[current]; ok {
			return nil, fmt.Errorf("%s has a cycle", label)
		}
		seen[current] = struct{}{}
		array, ok := v.entryArrays[current]
		if !ok {
			return nil, fmt.Errorf("%s references missing ENTRY_ARRAY", label)
		}
		if array.next != 0 && array.next <= current {
			return nil, fmt.Errorf("%s next pointer is not increasing", label)
		}
		usedHere := uint64(len(array.items))
		if remaining < usedHere {
			usedHere = remaining
		}
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

func (v *graphVerifier) dataObjectInHashTable(dataOffset, dataHash uint64) bool {
	tableOffset := v.header.dataHashTableOffset
	tableSize := v.header.dataHashTableSize
	if tableOffset == 0 || tableSize == 0 {
		return false
	}
	bucketCount := tableSize / hashItemSize
	bucket := dataHash % bucketCount
	current := binary.LittleEndian.Uint64(v.data[tableOffset+bucket*hashItemSize : tableOffset+bucket*hashItemSize+8])
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

func decompressGraphPayload(flags uint8, payload []byte) ([]byte, error) {
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
