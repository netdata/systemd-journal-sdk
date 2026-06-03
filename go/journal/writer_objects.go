package journal

import (
	"bytes"
	"encoding/binary"
)

func (w *Writer) addData(payload []byte) (uint64, uint64, error) {
	hash := w.hash(payload)
	if offset, ok, err := w.findData(hash, payload); err != nil || ok {
		return offset, hash, err
	}

	objectPayload, compressionFlag := w.compressedDataPayload(payload)
	offset, err := w.writeDataObject(hash, objectPayload, compressionFlag)
	if err != nil {
		return 0, 0, err
	}

	if err := w.appendHashItem(w.header.dataHashTableOffset, w.header.dataHashTableSize, objectTypeData, hash, offset); err != nil {
		return 0, 0, err
	}
	w.header.nData++

	if err := w.hmacPutObject(offset, objectTypeData); err != nil {
		return 0, 0, err
	}

	if err := w.linkDataToField(offset, payload); err != nil {
		return 0, 0, err
	}

	return offset, hash, nil
}

func (w *Writer) writeDataObject(hash uint64, objectPayload []byte, compressionFlag uint8) (uint64, error) {
	offset := w.appendOffset
	payloadOffset := w.dataPayloadOffset()
	size := payloadOffset + uint64(len(objectPayload))
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, err
	}
	buf, direct, err := w.newObjectBuffer(offset, size)
	if err != nil {
		return 0, err
	}
	putDataHeader(buf[:dataObjectHeaderSize], dataHeader{
		object: objectHeader{typ: objectTypeData, flag: compressionFlag, size: size},
		hash:   hash,
	})
	copy(buf[payloadOffset:], objectPayload)
	if err := w.commitObjectBuffer(offset, buf, direct); err != nil {
		return 0, err
	}
	if err := w.objectAdded(offset, size); err != nil {
		return 0, err
	}
	return offset, nil
}

func (w *Writer) linkDataToField(offset uint64, payload []byte) error {
	eq := bytes.IndexByte(payload, '=')
	if eq <= 0 {
		return nil
	}
	fieldPayload := payload[:eq]
	fieldHash := w.hash(fieldPayload)
	fieldOffset, fieldHeadDataOffset, err := w.addField(fieldHash, fieldPayload)
	if err != nil {
		return err
	}
	if err := w.writeUint64At(offset+32, fieldHeadDataOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(fieldOffset+32, offset); err != nil {
		return err
	}
	w.fieldCache.insert(fieldHash, fieldPayload, fieldOffset, offset)
	return nil
}

func (w *Writer) addField(hash uint64, payload []byte) (uint64, uint64, error) {
	if offset, headDataOffset, ok := w.fieldCache.get(hash, payload); ok {
		return offset, headDataOffset, nil
	}
	offset, ok, err := w.findField(hash, payload)
	if err != nil {
		return 0, 0, err
	}
	if ok {
		field, err := w.readFieldHeader(offset)
		if err != nil {
			return 0, 0, err
		}
		w.fieldCache.insert(hash, payload, offset, field.headDataOffset)
		return offset, field.headDataOffset, nil
	}

	offset = w.appendOffset
	size := uint64(fieldObjectHeaderSize + len(payload))
	if err := w.ensureCompactObjectFits(offset, size); err != nil {
		return 0, 0, err
	}
	buf, direct, err := w.newObjectBuffer(offset, size)
	if err != nil {
		return 0, 0, err
	}
	putFieldHeader(buf[:fieldObjectHeaderSize], fieldHeader{
		object: objectHeader{typ: objectTypeField, size: size},
		hash:   hash,
	})
	copy(buf[fieldObjectHeaderSize:], payload)
	if err := w.commitObjectBuffer(offset, buf, direct); err != nil {
		return 0, 0, err
	}
	if err := w.objectAdded(offset, size); err != nil {
		return 0, 0, err
	}

	if err := w.appendHashItem(w.header.fieldHashTableOffset, w.header.fieldHashTableSize, objectTypeField, hash, offset); err != nil {
		return 0, 0, err
	}
	w.header.nFields++

	if err := w.hmacPutObject(offset, objectTypeField); err != nil {
		return 0, 0, err
	}

	w.fieldCache.insert(hash, payload, offset, 0)
	return offset, 0, nil
}

func (w *Writer) appendHashItem(tableOffset, tableSize uint64, typ uint8, hash, objectOffset uint64) error {
	bucketOffset := tableOffset + (hash%(tableSize/hashItemSize))*hashItemSize
	item, err := w.readHashItem(bucketOffset)
	if err != nil {
		return err
	}
	if item.tail != 0 {
		if err := w.writeUint64At(item.tail+24, objectOffset); err != nil {
			return err
		}
	} else {
		item.head = objectOffset
	}
	item.tail = objectOffset

	// Sanity check the previous tail type if this bucket was non-empty.
	if item.head != objectOffset {
		oh, err := w.readObjectHeader(item.head)
		if err != nil {
			return err
		}
		if oh.typ != typ {
			return errInvalidJournal
		}
	}
	if err := w.writeHashItem(bucketOffset, item); err != nil {
		return err
	}
	if item.head != objectOffset {
		return w.updateHashChainDepth(typ, item.head)
	}
	return nil
}

func (w *Writer) updateHashChainDepth(typ uint8, head uint64) error {
	var depth uint64
	for offset := head; offset != 0; {
		var next uint64
		switch typ {
		case objectTypeData:
			header, err := w.readDataHeader(offset)
			if err != nil {
				return err
			}
			next = header.nextHashOffset
		case objectTypeField:
			header, err := w.readFieldHeader(offset)
			if err != nil {
				return err
			}
			next = header.nextHashOffset
		default:
			return errInvalidJournal
		}
		if next != 0 {
			depth++
		}
		offset = next
	}
	switch typ {
	case objectTypeData:
		if depth > w.header.dataHashChainDepth {
			w.header.dataHashChainDepth = depth
		}
	case objectTypeField:
		if depth > w.header.fieldHashChainDepth {
			w.header.fieldHashChainDepth = depth
		}
	}
	return nil
}

func (w *Writer) findData(hash uint64, payload []byte) (uint64, bool, error) {
	bucketOffset := w.header.dataHashTableOffset + (hash%(w.header.dataHashTableSize/hashItemSize))*hashItemSize
	item, err := w.readHashItem(bucketOffset)
	if err != nil {
		return 0, false, err
	}

	depth := uint64(0)
	for offset := item.head; offset != 0; {
		header, err := w.readDataHeader(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash {
			stored, err := w.readDataPayload(header, offset)
			if err != nil {
				return 0, false, err
			}
			if bytes.Equal(stored, payload) {
				return offset, true, nil
			}
		}
		if header.nextHashOffset != 0 {
			depth++
			if depth > w.header.dataHashChainDepth {
				w.header.dataHashChainDepth = depth
			}
		}
		offset = header.nextHashOffset
	}
	return 0, false, nil
}

func (w *Writer) findField(hash uint64, payload []byte) (uint64, bool, error) {
	bucketOffset := w.header.fieldHashTableOffset + (hash%(w.header.fieldHashTableSize/hashItemSize))*hashItemSize
	item, err := w.readHashItem(bucketOffset)
	if err != nil {
		return 0, false, err
	}

	depth := uint64(0)
	for offset := item.head; offset != 0; {
		header, stored, err := w.readFieldObject(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash && bytes.Equal(stored, payload) {
			return offset, true, nil
		}
		if header.nextHashOffset != 0 {
			depth++
			if depth > w.header.fieldHashChainDepth {
				w.header.fieldHashChainDepth = depth
			}
		}
		offset = header.nextHashOffset
	}
	return 0, false, nil
}

func (w *Writer) readHashItem(offset uint64) (hashItem, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, hashItemSize); err != nil || ok {
			if err != nil {
				return hashItem{}, err
			}
			return parseHashItem(src), nil
		}
	}
	var buf [hashItemSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return hashItem{}, err
	}
	return parseHashItem(buf[:]), nil
}

func (w *Writer) writeHashItem(offset uint64, item hashItem) error {
	var buf [hashItemSize]byte
	putHashItem(buf[:], item)
	return w.writeAt(offset, buf[:])
}

func (w *Writer) readDataObject(offset uint64) (dataHeader, []byte, error) {
	header, err := w.readDataHeader(offset)
	if err != nil {
		return dataHeader{}, nil, err
	}
	payload, err := w.readDataPayload(header, offset)
	if err != nil {
		return dataHeader{}, nil, err
	}
	return header, payload, nil
}

func (w *Writer) readDataPayload(header dataHeader, offset uint64) ([]byte, error) {
	payloadOffset := w.dataPayloadOffset()
	if header.object.typ != objectTypeData || header.object.size < payloadOffset {
		return nil, errInvalidJournal
	}
	payloadSize := header.object.size - payloadOffset
	var payload []byte
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset+payloadOffset, payloadSize); err != nil || ok {
			if err != nil {
				return nil, err
			}
			payload = src
		}
	}
	if payload == nil {
		if payloadSize > uint64(int(^uint(0)>>1)) {
			return nil, errInvalidJournal
		}
		payload = make([]byte, int(payloadSize))
		if err := w.readAt(payload, offset+payloadOffset); err != nil {
			return nil, err
		}
	}
	return decompressDataPayload(header.object.flag, payload)
}

func (w *Writer) readFieldObject(offset uint64) (fieldHeader, []byte, error) {
	header, err := w.readFieldHeader(offset)
	if err != nil {
		return fieldHeader{}, nil, err
	}
	if header.object.typ != objectTypeField || header.object.size < fieldObjectHeaderSize {
		return fieldHeader{}, nil, errInvalidJournal
	}
	payload := make([]byte, header.object.size-fieldObjectHeaderSize)
	if err := w.readAt(payload, offset+fieldObjectHeaderSize); err != nil {
		return fieldHeader{}, nil, err
	}
	return header, payload, nil
}

func (w *Writer) readObjectHeader(offset uint64) (objectHeader, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, objectHeaderSize); err != nil || ok {
			if err != nil {
				return objectHeader{}, err
			}
			return parseObjectHeader(src)
		}
	}
	var buf [objectHeaderSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return objectHeader{}, err
	}
	return parseObjectHeader(buf[:])
}

func (w *Writer) readDataHeader(offset uint64) (dataHeader, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, dataObjectHeaderSize); err != nil || ok {
			if err != nil {
				return dataHeader{}, err
			}
			return parseDataHeader(src)
		}
	}
	var buf [dataObjectHeaderSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return dataHeader{}, err
	}
	return parseDataHeader(buf[:])
}

func (w *Writer) readFieldHeader(offset uint64) (fieldHeader, error) {
	if w.arena != nil {
		if src, ok, err := w.arena.directBytesAt(offset, fieldObjectHeaderSize); err != nil || ok {
			if err != nil {
				return fieldHeader{}, err
			}
			return parseFieldHeader(src)
		}
	}
	var buf [fieldObjectHeaderSize]byte
	if err := w.readAt(buf[:], offset); err != nil {
		return fieldHeader{}, err
	}
	return parseFieldHeader(buf[:])
}

func (w *Writer) writeUint64At(offset, value uint64) error {
	if w.arena != nil {
		if dst, ok, err := w.arena.directBytesAt(offset, 8); err != nil || ok {
			if err != nil {
				return err
			}
			binary.LittleEndian.PutUint64(dst, value)
			return nil
		}
	}
	var buf [8]byte
	binary.LittleEndian.PutUint64(buf[:], value)
	return w.writeAt(offset, buf[:])
}

func (w *Writer) writeUint32At(offset uint64, value uint32) error {
	if w.arena != nil {
		if dst, ok, err := w.arena.directBytesAt(offset, 4); err != nil || ok {
			if err != nil {
				return err
			}
			binary.LittleEndian.PutUint32(dst, value)
			return nil
		}
	}
	var buf [4]byte
	binary.LittleEndian.PutUint32(buf[:], value)
	return w.writeAt(offset, buf[:])
}

func (w *Writer) writeUUIDAt(offset uint64, value UUID) error {
	return w.writeAt(offset, value[:])
}
