package journal

import (
	"bytes"
	"fmt"
	"unicode/utf8"
)

func (r *Reader) GetEntry() (*Entry, error) {
	if r.entryIndex < 0 || r.entryIndex >= len(r.entryOffsets) {
		return nil, errEndOfEntries
	}

	r.entryDataActive = false
	offset := r.entryOffsets[r.entryIndex]
	return r.readEntryAt(offset)
}

// VisitEntryPayloads calls visitor for each current DATA payload as FIELD=value
// bytes. Payloads are callback-scoped: uncompressed mmap mode may pass slices
// backed by the mapped file, so do not retain or mutate them after the visitor
// returns. Use EnumerateEntryPayload when row-level lifetime is required.
func (r *Reader) VisitEntryPayloads(visitor func([]byte) error) error {
	if visitor == nil {
		return nil
	}
	r.entryDataActive = false
	offsets, err := r.currentEntryDataOffsets()
	if err != nil {
		return err
	}
	for _, dataOff := range offsets {
		payload, err := r.readDataPayloadTemp(dataOff)
		if err != nil {
			return err
		}
		if err := visitor(payload); err != nil {
			return err
		}
	}
	return nil
}

// CollectEntryPayloads returns owned FIELD=value payload copies for the current
// entry.
func (r *Reader) CollectEntryPayloads() ([][]byte, error) {
	var payloads [][]byte
	err := r.VisitEntryPayloads(func(payload []byte) error {
		payloads = append(payloads, cloneBytes(payload))
		return nil
	})
	return payloads, err
}

// GetEntryPayload returns an owned FIELD=value payload copy for fieldName.
func (r *Reader) GetEntryPayload(fieldName []byte) ([]byte, bool, error) {
	var found []byte
	err := r.VisitEntryPayloads(func(payload []byte) error {
		if found != nil {
			return nil
		}
		if len(payload) > len(fieldName) &&
			bytes.Equal(payload[:len(fieldName)], fieldName) &&
			payload[len(fieldName)] == '=' {
			found = cloneBytes(payload)
		}
		return nil
	})
	if err != nil {
		return nil, false, err
	}
	return found, found != nil, nil
}

// GetRaw returns an owned value copy for fieldName.
func (r *Reader) GetRaw(fieldName []byte) ([]byte, bool, error) {
	payload, ok, err := r.GetEntryPayload(fieldName)
	if err != nil || !ok {
		return nil, ok, err
	}
	_, value, split := splitRawPayload(payload)
	if !split {
		return nil, false, errCorruptObject
	}
	return cloneBytes(value), true, nil
}

// GetRawValues returns owned value copies for every occurrence of fieldName.
func (r *Reader) GetRawValues(fieldName []byte) ([][]byte, error) {
	var values [][]byte
	err := r.VisitEntryPayloads(func(payload []byte) error {
		name, value, ok := splitRawPayload(payload)
		if ok && bytes.Equal(name, fieldName) {
			values = append(values, cloneBytes(value))
		}
		return nil
	})
	return values, err
}

// EntryDataRestart resets libsystemd-style DATA enumeration for the current
// entry.
func (r *Reader) EntryDataRestart() error {
	if _, err := r.currentEntryDataOffsets(); err != nil {
		return err
	}
	r.entryDataIndex = 0
	r.entryDataActive = true
	return nil
}

// EnumerateEntryPayload returns the next FIELD=value payload for the current
// entry. Returned slices stay valid for the current row after end-of-row
// enumeration and until the reader advances, seeks, clears/restarts DATA
// enumeration, refreshes/remaps the file, or closes. Use CollectEntryPayloads
// or copy the slice when longer ownership is required.
func (r *Reader) EnumerateEntryPayload() ([]byte, bool, error) {
	if !r.entryDataActive {
		if err := r.EntryDataRestart(); err != nil {
			return nil, false, err
		}
	}
	if r.entryDataIndex >= len(r.entryDataOffsets) {
		r.clearEntryDataState()
		return nil, false, nil
	}
	dataOff := r.entryDataOffsets[r.entryDataIndex]
	r.entryDataIndex++
	payload, err := r.readDataPayloadRow(dataOff)
	if err != nil {
		return nil, false, err
	}
	return payload, true, nil
}

func (r *Reader) readEntryAt(offset uint64) (*Entry, error) {
	entryHdr, entries, err := r.readEntryDataOffsetsAt(offset, nil)
	if err != nil {
		return nil, err
	}

	fields := make(map[string][]byte)
	fieldValues := make(map[string][][]byte)
	rawFieldValues := make(map[string][][]byte)
	payloads := make([][]byte, 0, len(entries))
	rawFields := make([]RawField, 0, len(entries))
	for _, dataOff := range entries {
		payload, err := r.readDataPayloadTemp(dataOff)
		if err != nil {
			return nil, fmt.Errorf("read data object at offset %d for entry at offset %d: %w", dataOff, offset, err)
		}
		nameBytes, value, ok := splitRawPayload(payload)
		if !ok {
			return nil, fmt.Errorf("%w: data object at offset %d has no field separator", errCorruptObject, dataOff)
		}

		payloadCopy := cloneBytes(payload)
		payloads = append(payloads, payloadCopy)
		nameCopy := cloneBytes(nameBytes)
		valueCopy := cloneBytes(value)
		rawFields = append(rawFields, RawField{Name: nameCopy, Value: valueCopy})
		key := rawFieldKey(nameBytes)
		rawFieldValues[key] = append(rawFieldValues[key], valueCopy)

		if utf8.Valid(nameBytes) {
			name := string(nameBytes)
			if _, ok := fields[name]; !ok {
				fields[name] = valueCopy
			}
			fieldValues[name] = append(fieldValues[name], valueCopy)
		}
	}

	cursor := r.makeCursor(offset, entryHdr)

	return &Entry{
		Fields:         fields,
		FieldValues:    fieldValues,
		Payloads:       payloads,
		RawFields:      rawFields,
		RawFieldValues: rawFieldValues,
		Seqnum:         entryHdr.seqnum,
		Realtime:       entryHdr.realtime,
		Monotonic:      entryHdr.monotonic,
		BootID:         entryHdr.bootID,
		Cursor:         cursor,
	}, nil
}

func (r *Reader) makeCursor(entryOffset uint64, hdr entryHeader) string {
	return fmt.Sprintf("s=%s;i=%x;b=%s;m=%x;t=%x;x=%x",
		r.header.seqnumID.String(),
		hdr.seqnum,
		hdr.bootID.String(),
		hdr.monotonic,
		hdr.realtime,
		hdr.xorHash)
}

func formatCursorFromDirectoryKey(key directoryEntryKey) string {
	return fmt.Sprintf("s=%s;i=%x;b=%s;m=%x;t=%x;x=%x",
		key.seqnumID.String(),
		key.seqnum,
		key.bootID.String(),
		key.monotonic,
		key.realtime,
		key.xorHash)
}

func (r *Reader) readDataPayloadTemp(offset uint64) ([]byte, error) {
	headerBuf, err := r.readSlice(offset, objectHeaderSize)
	if err != nil {
		return nil, err
	}

	objHdr, err := parseObjectHeader(headerBuf)
	if err != nil {
		return nil, err
	}

	if objHdr.typ != objectTypeData {
		return nil, errCorruptObject
	}
	payloadOffset := r.dataPayloadOffset()
	if objHdr.size < payloadOffset {
		return nil, errCorruptObject
	}

	payloadLen := objHdr.size - payloadOffset
	payload, err := r.readSlice(offset+payloadOffset, payloadLen)
	if err != nil {
		return nil, err
	}
	return decompressDataPayload(objHdr.flag, payload)
}

func (r *Reader) readDataPayloadRow(offset uint64) ([]byte, error) {
	headerBuf, err := r.readSlice(offset, objectHeaderSize)
	if err != nil {
		return nil, err
	}

	objHdr, err := parseObjectHeader(headerBuf)
	if err != nil {
		return nil, err
	}

	if objHdr.typ != objectTypeData {
		return nil, errCorruptObject
	}
	payloadOffset := r.dataPayloadOffset()
	if objHdr.size < payloadOffset {
		return nil, errCorruptObject
	}

	payloadLen := objHdr.size - payloadOffset
	payloadStart := offset + payloadOffset
	if objHdr.flag&objectCompressedMask != 0 {
		payload, err := r.readSlice(payloadStart, payloadLen)
		if err != nil {
			return nil, err
		}
		decompressed, err := decompressDataPayload(objHdr.flag, payload)
		if err != nil {
			return nil, err
		}
		return r.rowCopy(decompressed)
	}

	payload, err := r.readRowSlice(payloadStart, payloadLen)
	if err != nil {
		return nil, err
	}
	return payload, nil
}

func (r *Reader) visitDataPayloadWithHeader(offset uint64, header dataHeader, visit func([]byte) error) error {
	payloadOffset := r.dataPayloadOffset()
	if header.object.typ != objectTypeData || header.object.size < payloadOffset {
		return errCorruptObject
	}

	payloadLen := header.object.size - payloadOffset
	payload, err := r.readSlice(offset+payloadOffset, payloadLen)
	if err != nil {
		return err
	}
	payload, err = decompressDataPayload(header.object.flag, payload)
	if err != nil {
		return err
	}

	return visit(payload)
}

func (r *Reader) readDataHeaderAt(offset uint64) (dataHeader, error) {
	buf, err := r.readSlice(offset, dataObjectHeaderSize)
	if err != nil {
		return dataHeader{}, err
	}
	header, err := parseDataHeader(buf)
	if err != nil {
		return dataHeader{}, err
	}
	if header.object.typ != objectTypeData || header.object.size < r.dataPayloadOffset() {
		return dataHeader{}, errCorruptObject
	}
	return header, nil
}

func (r *Reader) readFieldObjectAt(offset uint64) (fieldHeader, []byte, error) {
	headerBuf, err := r.readSlice(offset, fieldObjectHeaderSize)
	if err != nil {
		return fieldHeader{}, nil, err
	}
	header, err := parseFieldHeader(headerBuf)
	if err != nil {
		return fieldHeader{}, nil, err
	}
	if header.object.typ != objectTypeField || header.object.size < fieldObjectHeaderSize {
		return fieldHeader{}, nil, errCorruptObject
	}
	payload, err := r.readSlice(offset+fieldObjectHeaderSize, header.object.size-fieldObjectHeaderSize)
	if err != nil {
		return fieldHeader{}, nil, err
	}
	return header, payload, nil
}

func (r *Reader) findFieldHeadDataOffset(field []byte) (uint64, bool, error) {
	if r.header.fieldHashTableOffset == 0 || r.header.fieldHashTableSize < hashItemSize {
		return 0, false, nil
	}
	hash := r.hash(field)
	buckets := r.header.fieldHashTableSize / hashItemSize
	if buckets == 0 {
		return 0, false, nil
	}
	bucketOffset := r.header.fieldHashTableOffset + (hash%buckets)*hashItemSize
	itemBuf, err := r.readSlice(bucketOffset, hashItemSize)
	if err != nil {
		return 0, false, err
	}
	item := parseHashItem(itemBuf)
	for offset := item.head; offset != 0; {
		header, payload, err := r.readFieldObjectAt(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash && bytes.Equal(payload, field) {
			return header.headDataOffset, true, nil
		}
		offset = header.nextHashOffset
	}
	return 0, false, nil
}
