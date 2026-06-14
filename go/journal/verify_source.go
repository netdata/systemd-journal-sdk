package journal

import (
	"encoding/binary"
	"fmt"
)

type verifyByteSource interface {
	Len() uint64
	Slice(offset, size uint64) ([]byte, error)
}

type readerVerifySource struct {
	reader *Reader
}

func (s readerVerifySource) Len() uint64 {
	// Verifier readers are opened with snapshot bounds; this is the immutable
	// byte view that structural and sealed verification operate on.
	return s.reader.fileSize
}

func (s readerVerifySource) Slice(offset, size uint64) ([]byte, error) {
	end, ok := checkedAdd(offset, size)
	if !ok {
		return nil, fmt.Errorf("slice %d..+%d overflows", offset, size)
	}
	if end > s.Len() {
		return nil, fmt.Errorf("slice %d..%d exceeds file bounds", offset, end)
	}
	if size > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("slice %d..%d exceeds platform bounds", offset, end)
	}
	buf := make([]byte, int(size))
	if err := s.reader.readAt(buf, offset); err != nil {
		return nil, err
	}
	return buf, nil
}

func verifySourceByte(source verifyByteSource, offset uint64) (byte, error) {
	buf, err := source.Slice(offset, 1)
	if err != nil {
		return 0, err
	}
	return buf[0], nil
}

func verifySourceU32(source verifyByteSource, offset uint64) (uint32, error) {
	buf, err := source.Slice(offset, 4)
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint32(buf), nil
}

func verifySourceU64(source verifyByteSource, offset uint64) (uint64, error) {
	buf, err := source.Slice(offset, 8)
	if err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint64(buf), nil
}

func verifySourceUUID(source verifyByteSource, offset uint64) (UUID, error) {
	buf, err := source.Slice(offset, 16)
	if err != nil {
		return UUID{}, err
	}
	var id UUID
	copy(id[:], buf)
	return id, nil
}

func verifySourceHeader(source verifyByteSource) (journalHeader, error) {
	size := minUint64(headerSize, source.Len())
	if size < headerMinSize {
		return journalHeader{}, errInvalidJournal
	}
	buf, err := source.Slice(0, size)
	if err != nil {
		return journalHeader{}, err
	}
	return parseHeader(buf)
}

func verifySourceHasHeaderField(source verifyByteSource, headerSize uint64, end int) bool {
	return headerSize >= uint64(end) && source.Len() >= uint64(end)
}
