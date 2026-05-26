package journal

import (
	"encoding/binary"
	"errors"
	"testing"
)

func TestParseHeaderHistoricalFieldBoundaries(t *testing.T) {
	const (
		nData                  = 11
		nFields                = 22
		nTags                  = 33
		nEntryArrays           = 44
		dataHashChainDepth     = 55
		fieldHashChainDepth    = 66
		tailEntryArrayOffset   = 77
		tailEntryArrayNEntries = 88
		tailEntryOffset        = 99
	)

	tests := []struct {
		name                   string
		headerSize             int
		nData                  uint64
		nFields                uint64
		nTags                  uint64
		nEntryArrays           uint64
		dataHashChainDepth     uint64
		fieldHashChainDepth    uint64
		tailEntryArrayOffset   uint32
		tailEntryArrayNEntries uint32
		tailEntryOffset        uint64
	}{
		{name: "base", headerSize: 208},
		{name: "n_data", headerSize: 216, nData: nData},
		{name: "mid_n_data", headerSize: 220, nData: nData},
		{name: "n_fields", headerSize: 224, nData: nData, nFields: nFields},
		{name: "n_tags", headerSize: 232, nData: nData, nFields: nFields, nTags: nTags},
		{name: "n_entry_arrays", headerSize: 240, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays},
		{name: "data_hash_chain_depth", headerSize: 248, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth},
		{name: "mid_data_hash_chain_depth", headerSize: 250, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth},
		{name: "field_hash_chain_depth", headerSize: 256, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth, fieldHashChainDepth: fieldHashChainDepth},
		{name: "tail_entry_array_offset", headerSize: 260, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth, fieldHashChainDepth: fieldHashChainDepth, tailEntryArrayOffset: tailEntryArrayOffset},
		{name: "tail_entry_array_n_entries", headerSize: 264, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth, fieldHashChainDepth: fieldHashChainDepth, tailEntryArrayOffset: tailEntryArrayOffset, tailEntryArrayNEntries: tailEntryArrayNEntries},
		{name: "mid_tail_entry_array_n_entries", headerSize: 268, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth, fieldHashChainDepth: fieldHashChainDepth, tailEntryArrayOffset: tailEntryArrayOffset, tailEntryArrayNEntries: tailEntryArrayNEntries},
		{name: "tail_entry_offset", headerSize: 272, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth, fieldHashChainDepth: fieldHashChainDepth, tailEntryArrayOffset: tailEntryArrayOffset, tailEntryArrayNEntries: tailEntryArrayNEntries, tailEntryOffset: tailEntryOffset},
		{name: "future_header", headerSize: 300, nData: nData, nFields: nFields, nTags: nTags, nEntryArrays: nEntryArrays, dataHashChainDepth: dataHashChainDepth, fieldHashChainDepth: fieldHashChainDepth, tailEntryArrayOffset: tailEntryArrayOffset, tailEntryArrayNEntries: tailEntryArrayNEntries, tailEntryOffset: tailEntryOffset},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			buf := historicalHeaderFixture(tc.headerSize)
			header, err := parseHeader(buf)
			if err != nil {
				t.Fatalf("parseHeader() error = %v", err)
			}

			if header.nData != tc.nData ||
				header.nFields != tc.nFields ||
				header.nTags != tc.nTags ||
				header.nEntryArrays != tc.nEntryArrays ||
				header.dataHashChainDepth != tc.dataHashChainDepth ||
				header.fieldHashChainDepth != tc.fieldHashChainDepth ||
				header.tailEntryArrayOffset != tc.tailEntryArrayOffset ||
				header.tailEntryArrayNEntries != tc.tailEntryArrayNEntries ||
				header.tailEntryOffset != tc.tailEntryOffset {
				t.Fatalf("parsed historical fields = %+v", header)
			}
		})
	}
}

func TestParseHeaderRejectsTruncatedFutureHeaderPrefix(t *testing.T) {
	buf := historicalHeaderFixture(300)
	_, err := parseHeader(buf[:208])
	if !errors.Is(err, errInvalidJournal) {
		t.Fatalf("parseHeader() error = %v, want %v", err, errInvalidJournal)
	}
}

func historicalHeaderFixture(size int) []byte {
	bufferSize := headerSize
	if size > bufferSize {
		bufferSize = size
	}
	buf := make([]byte, bufferSize)
	copy(buf[0:8], []byte{'L', 'P', 'K', 'S', 'H', 'H', 'R', 'H'})
	binary.LittleEndian.PutUint32(buf[12:16], incompatibleKeyedHash)
	binary.LittleEndian.PutUint64(buf[88:96], uint64(size))
	binary.LittleEndian.PutUint64(buf[208:216], 11)
	binary.LittleEndian.PutUint64(buf[216:224], 22)
	binary.LittleEndian.PutUint64(buf[224:232], 33)
	binary.LittleEndian.PutUint64(buf[232:240], 44)
	binary.LittleEndian.PutUint64(buf[240:248], 55)
	binary.LittleEndian.PutUint64(buf[248:256], 66)
	binary.LittleEndian.PutUint32(buf[256:260], 77)
	binary.LittleEndian.PutUint32(buf[260:264], 88)
	binary.LittleEndian.PutUint64(buf[264:272], 99)
	if size < headerSize {
		return buf[:size]
	}
	return buf
}
