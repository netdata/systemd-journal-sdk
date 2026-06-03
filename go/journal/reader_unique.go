package journal

import (
	"bytes"
	"fmt"
	"unicode/utf8"
)

func (r *Reader) QueryUnique(fieldName string) ([][]byte, error) {
	var results [][]byte
	err := r.VisitUnique(fieldName, func(value []byte) error {
		results = append(results, cloneBytes(value))
		return nil
	})
	return results, err
}

func (r *Reader) VisitUnique(fieldName string, visit func([]byte) error) error {
	field := []byte(fieldName)

	offset, ok, err := r.findFieldHeadDataOffset(field)
	if err != nil || !ok {
		return err
	}

	for offset != 0 {
		header, err := r.readDataHeaderAt(offset)
		if err != nil {
			return err
		}
		err = r.visitDataPayloadWithHeader(offset, header, func(payload []byte) error {
			if len(payload) <= len(field) || !bytes.Equal(payload[:len(field)], field) || payload[len(field)] != '=' {
				return fmt.Errorf("%w: field data object at offset %d does not match %q", errCorruptObject, offset, fieldName)
			}
			return visit(payload[len(field)+1:])
		})
		if err != nil {
			return err
		}

		offset = header.nextFieldOffset
	}

	return nil
}

func (r *Reader) EnumerateFields() (map[string]struct{}, error) {
	fields := make(map[string]struct{})

	if r.header.fieldHashTableOffset == 0 || r.header.fieldHashTableSize < hashItemSize {
		return r.enumerateFieldsByEntryScan()
	}
	buckets := r.header.fieldHashTableSize / hashItemSize
	for i := uint64(0); i < buckets; i++ {
		itemBuf, err := r.readSlice(r.header.fieldHashTableOffset+i*hashItemSize, hashItemSize)
		if err != nil {
			return r.enumerateFieldsByEntryScan()
		}
		item := parseHashItem(itemBuf)
		for offset := item.head; offset != 0; {
			header, payload, err := r.readFieldObjectAt(offset)
			if err != nil {
				return r.enumerateFieldsByEntryScan()
			}
			if utf8.Valid(payload) {
				fields[string(payload)] = struct{}{}
			}
			offset = header.nextHashOffset
		}
	}

	return fields, nil
}

func (r *Reader) enumerateFieldsByEntryScan() (map[string]struct{}, error) {
	fields := make(map[string]struct{})
	for _, offset := range r.entryOffsets {
		entry, err := r.readEntryAt(offset)
		if err != nil {
			continue
		}
		for name := range entry.Fields {
			fields[name] = struct{}{}
		}
	}
	return fields, nil
}
