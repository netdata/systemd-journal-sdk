package journal

import (
	"encoding/binary"
	"errors"
	"syscall"
	"testing"
)

type uniqueHeaderControlAccess struct {
	readerAccessor
	data           []byte
	err            error
	offset, extent uint64
	calls          int
}

func (a *uniqueHeaderControlAccess) tempSlice(offset, extent uint64) ([]byte, error) {
	a.offset, a.extent = offset, extent
	a.calls++
	return a.data, a.err
}

func TestUniqueHeaderAccessorPreservesGenericContract(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for length := 0; length <= dataObjectHeaderSize; length++ {
			for _, typ := range []byte{0, objectTypeData, objectTypeEntry} {
				for _, size := range []uint64{0, 63, 64, 71, 72, ^uint64(0)} {
					for _, accessErr := range []error{nil, syscall.EIO} {
						data := make([]byte, dataObjectHeaderSize)
						data[0], data[1] = typ, 0xff
						binary.LittleEndian.PutUint64(data[8:16], size)
						binary.LittleEndian.PutUint64(data[32:40], 0xfedcba9876543210)
						oldAccess := &uniqueHeaderControlAccess{data: data[:length], err: accessErr}
						newAccess := &uniqueHeaderControlAccess{data: data[:length], err: accessErr}
						oldReader, newReader := Reader{accessor: oldAccess}, Reader{accessor: newAccess}
						if compact {
							oldReader.header.incompatibleFlags = incompatibleCompact
							newReader.header.incompatibleFlags = incompatibleCompact
						}
						oldReader.configureLayout()
						newReader.configureLayout()
						old, oldErr := oldReader.readDataHeaderAt(1)
						got, gotErr := newReader.readUniqueDataHeaderAt(1)
						if !errors.Is(gotErr, oldErr) || !errors.Is(oldErr, gotErr) {
							t.Fatalf("compact=%v length=%d type=%d size=%d access=%v: errors %v / %v", compact, length, typ, size, accessErr, oldErr, gotErr)
						}
						if gotErr == nil && (got.object != old.object || got.nextFieldOffset != old.nextFieldOffset) {
							t.Fatalf("different decoded unique header: %+v / %+v", old, got)
						}
						if oldAccess.calls != 1 || newAccess.calls != 1 || oldAccess.offset != 1 || newAccess.offset != 1 || oldAccess.extent != dataObjectHeaderSize || newAccess.extent != dataObjectHeaderSize {
							t.Fatalf("different access request: %+v / %+v", oldAccess, newAccess)
						}
						for i := range data {
							data[i] = 0
						}
						if gotErr == nil && (got.object != old.object || got.nextFieldOffset != old.nextFieldOffset) {
							t.Fatal("header retained borrowed storage")
						}
					}
				}
			}
		}
	}
	var reader Reader
	_, oldErr := reader.readDataHeaderAt(1)
	_, gotErr := reader.readUniqueDataHeaderAt(1)
	if !errors.Is(oldErr, errInvalidJournal) || !errors.Is(gotErr, oldErr) {
		t.Fatalf("nil accessor: %v / %v", oldErr, gotErr)
	}
}
