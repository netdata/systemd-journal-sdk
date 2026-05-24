package journal

import (
	"encoding/binary"
	"errors"
)

const (
	headerMinSize               = 208
	headerSize                  = 272 // v260+ uses 272-byte header
	objectHeaderSize            = 16
	hashItemSize                = 16
	fieldObjectHeaderSize       = 40
	offsetArrayObjectHeaderSize = 24
	entryObjectHeaderSize       = 64
	dataObjectHeaderSize        = 64
	regularEntryItemSize        = 16
	objectAlignment             = 8

	// systemd v260.1 defaults for 64 MiB max_size:
	// data: MAX(64*1024*1024*4/768/3, 2047) = 116508
	// field: DEFAULT_FIELD_HASH_TABLE_SIZE = 1023
	defaultDataHashBuckets   = 116508
	defaultFieldHashBuckets  = 1023
	initialEntryArrayCap     = 4096
	initialDataEntryArrayCap = 64

	// systemd default max file size for hash table sizing
	defaultMaxFileSize = 64 * 1024 * 1024 // 64 MiB
	// systemd FILE_SIZE_INCREASE for preallocation rounding
	fileSizeIncrease = 8 * 1024 * 1024 // 8 MiB
	// systemd's DATA object decompression limit is 768 MiB.
	maxUncompressedDataObjectSize = 768 * 1024 * 1024
)

const (
	objectTypeData           = 1
	objectTypeField          = 2
	objectTypeEntry          = 3
	objectTypeDataHashTable  = 4
	objectTypeFieldHashTable = 5
	objectTypeEntryArray     = 6
)

const (
	stateOffline  = 0
	stateOnline   = 1
	stateArchived = 2

	incompatibleCompressedXZ   = 1 << 0
	incompatibleCompressedLZ4  = 1 << 1
	incompatibleKeyedHash      = 1 << 2
	incompatibleCompressedZSTD = 1 << 3
	incompatibleCompact        = 1 << 4

	// HEADER_COMPATIBLE_TAIL_ENTRY_BOOT_ID - set for new files (v260+)
	compatibleTailEntryBootID = 1 << 1
)

const (
	objectCompressedXZ   = 1 << 0
	objectCompressedLZ4  = 1 << 1
	objectCompressedZSTD = 1 << 2
)

const (
	CompressionNone = 0
	CompressionZSTD = 1
	CompressionXZ   = 2
	CompressionLZ4  = 3
)

const defaultCompressThreshold = 64

var (
	errInvalidJournal     = errors.New("invalid journal file")
	errUnsupportedJournal = errors.New("unsupported journal file")
	errWriterClosed       = errors.New("journal writer is closed")
	errFieldName          = errors.New("invalid journal field name")
	errEntryEmpty         = errors.New("journal entry has no fields")
)

type journalHeader struct {
	signature            [8]byte
	compatibleFlags      uint32
	incompatibleFlags    uint32
	state                uint8
	reserved             [7]byte
	fileID               UUID
	machineID            UUID
	tailEntryBootID      UUID
	seqnumID             UUID
	headerSize           uint64
	arenaSize            uint64
	dataHashTableOffset  uint64
	dataHashTableSize    uint64
	fieldHashTableOffset uint64
	fieldHashTableSize   uint64
	tailObjectOffset     uint64
	nObjects             uint64
	nEntries             uint64
	tailEntrySeqnum      uint64
	headEntrySeqnum      uint64
	entryArrayOffset     uint64
	headEntryRealtime    uint64
	tailEntryRealtime    uint64
	tailEntryMonotonic   uint64
	// Added in 187
	nData   uint64
	nFields uint64
	// Added in 189
	nTags        uint64
	nEntryArrays uint64
	// Added in 246
	dataHashChainDepth  uint64
	fieldHashChainDepth uint64
	// Added in 252
	tailEntryArrayOffset   uint32
	tailEntryArrayNEntries uint32
	// Added in 254
	tailEntryOffset uint64
}

type objectHeader struct {
	typ  uint8
	flag uint8
	size uint64
}

type hashItem struct {
	head uint64
	tail uint64
}

type dataHeader struct {
	object           objectHeader
	hash             uint64
	nextHashOffset   uint64
	nextFieldOffset  uint64
	entryOffset      uint64
	entryArrayOffset uint64
	nEntries         uint64
}

type fieldHeader struct {
	object         objectHeader
	hash           uint64
	nextHashOffset uint64
	headDataOffset uint64
}

type offsetArrayHeader struct {
	object          objectHeader
	nextArrayOffset uint64
}

type entryHeader struct {
	object    objectHeader
	seqnum    uint64
	realtime  uint64
	monotonic uint64
	bootID    UUID
	xorHash   uint64
}

func align8(v uint64) uint64 {
	return (v + objectAlignment - 1) &^ (objectAlignment - 1)
}

func putHeader(dst []byte, h journalHeader) {
	copy(dst[0:8], h.signature[:])
	binary.LittleEndian.PutUint32(dst[8:12], h.compatibleFlags)
	binary.LittleEndian.PutUint32(dst[12:16], h.incompatibleFlags)
	dst[16] = h.state
	copy(dst[24:40], h.fileID[:])
	copy(dst[40:56], h.machineID[:])
	copy(dst[56:72], h.tailEntryBootID[:])
	copy(dst[72:88], h.seqnumID[:])
	binary.LittleEndian.PutUint64(dst[88:96], h.headerSize)
	binary.LittleEndian.PutUint64(dst[96:104], h.arenaSize)
	binary.LittleEndian.PutUint64(dst[104:112], h.dataHashTableOffset)
	binary.LittleEndian.PutUint64(dst[112:120], h.dataHashTableSize)
	binary.LittleEndian.PutUint64(dst[120:128], h.fieldHashTableOffset)
	binary.LittleEndian.PutUint64(dst[128:136], h.fieldHashTableSize)
	binary.LittleEndian.PutUint64(dst[136:144], h.tailObjectOffset)
	binary.LittleEndian.PutUint64(dst[144:152], h.nObjects)
	binary.LittleEndian.PutUint64(dst[152:160], h.nEntries)
	binary.LittleEndian.PutUint64(dst[160:168], h.tailEntrySeqnum)
	binary.LittleEndian.PutUint64(dst[168:176], h.headEntrySeqnum)
	binary.LittleEndian.PutUint64(dst[176:184], h.entryArrayOffset)
	binary.LittleEndian.PutUint64(dst[184:192], h.headEntryRealtime)
	binary.LittleEndian.PutUint64(dst[192:200], h.tailEntryRealtime)
	binary.LittleEndian.PutUint64(dst[200:208], h.tailEntryMonotonic)
	// Added in 187
	binary.LittleEndian.PutUint64(dst[208:216], h.nData)
	binary.LittleEndian.PutUint64(dst[216:224], h.nFields)
	// Added in 189
	binary.LittleEndian.PutUint64(dst[224:232], h.nTags)
	binary.LittleEndian.PutUint64(dst[232:240], h.nEntryArrays)
	// Added in 246
	binary.LittleEndian.PutUint64(dst[240:248], h.dataHashChainDepth)
	binary.LittleEndian.PutUint64(dst[248:256], h.fieldHashChainDepth)
	// Added in 252
	binary.LittleEndian.PutUint32(dst[256:260], h.tailEntryArrayOffset)
	binary.LittleEndian.PutUint32(dst[260:264], h.tailEntryArrayNEntries)
	// Added in 254
	binary.LittleEndian.PutUint64(dst[264:272], h.tailEntryOffset)
}

func parseHeader(src []byte) (journalHeader, error) {
	if len(src) < headerMinSize {
		return journalHeader{}, errInvalidJournal
	}

	var h journalHeader
	copy(h.signature[:], src[0:8])
	if h.signature != [8]byte{'L', 'P', 'K', 'S', 'H', 'H', 'R', 'H'} {
		return journalHeader{}, errInvalidJournal
	}

	h.compatibleFlags = binary.LittleEndian.Uint32(src[8:12])
	h.incompatibleFlags = binary.LittleEndian.Uint32(src[12:16])
	h.state = src[16]
	copy(h.fileID[:], src[24:40])
	copy(h.machineID[:], src[40:56])
	copy(h.tailEntryBootID[:], src[56:72])
	copy(h.seqnumID[:], src[72:88])
	h.headerSize = binary.LittleEndian.Uint64(src[88:96])
	h.arenaSize = binary.LittleEndian.Uint64(src[96:104])
	h.dataHashTableOffset = binary.LittleEndian.Uint64(src[104:112])
	h.dataHashTableSize = binary.LittleEndian.Uint64(src[112:120])
	h.fieldHashTableOffset = binary.LittleEndian.Uint64(src[120:128])
	h.fieldHashTableSize = binary.LittleEndian.Uint64(src[128:136])
	h.tailObjectOffset = binary.LittleEndian.Uint64(src[136:144])
	h.nObjects = binary.LittleEndian.Uint64(src[144:152])
	h.nEntries = binary.LittleEndian.Uint64(src[152:160])
	h.tailEntrySeqnum = binary.LittleEndian.Uint64(src[160:168])
	h.headEntrySeqnum = binary.LittleEndian.Uint64(src[168:176])
	h.entryArrayOffset = binary.LittleEndian.Uint64(src[176:184])
	h.headEntryRealtime = binary.LittleEndian.Uint64(src[184:192])
	h.tailEntryRealtime = binary.LittleEndian.Uint64(src[192:200])
	h.tailEntryMonotonic = binary.LittleEndian.Uint64(src[200:208])
	if h.headerSize >= headerSize {
		if len(src) < headerSize {
			return journalHeader{}, errInvalidJournal
		}
		// Added in 187
		h.nData = binary.LittleEndian.Uint64(src[208:216])
		h.nFields = binary.LittleEndian.Uint64(src[216:224])
		// Added in 189
		h.nTags = binary.LittleEndian.Uint64(src[224:232])
		h.nEntryArrays = binary.LittleEndian.Uint64(src[232:240])
		// Added in 246
		h.dataHashChainDepth = binary.LittleEndian.Uint64(src[240:248])
		h.fieldHashChainDepth = binary.LittleEndian.Uint64(src[248:256])
		// Added in 252
		h.tailEntryArrayOffset = binary.LittleEndian.Uint32(src[256:260])
		h.tailEntryArrayNEntries = binary.LittleEndian.Uint32(src[260:264])
		// Added in 254
		h.tailEntryOffset = binary.LittleEndian.Uint64(src[264:272])
	}

	if h.headerSize < headerMinSize {
		return journalHeader{}, errUnsupportedJournal
	}
	if h.incompatibleFlags&incompatibleKeyedHash == 0 {
		return journalHeader{}, errUnsupportedJournal
	}
	return h, nil
}

func putObjectHeader(dst []byte, h objectHeader) {
	dst[0] = h.typ
	dst[1] = h.flag
	binary.LittleEndian.PutUint64(dst[8:16], h.size)
}

func parseObjectHeader(src []byte) (objectHeader, error) {
	if len(src) < objectHeaderSize {
		return objectHeader{}, errInvalidJournal
	}
	return objectHeader{
		typ:  src[0],
		flag: src[1],
		size: binary.LittleEndian.Uint64(src[8:16]),
	}, nil
}

func putHashItem(dst []byte, item hashItem) {
	binary.LittleEndian.PutUint64(dst[0:8], item.head)
	binary.LittleEndian.PutUint64(dst[8:16], item.tail)
}

func parseHashItem(src []byte) hashItem {
	return hashItem{
		head: binary.LittleEndian.Uint64(src[0:8]),
		tail: binary.LittleEndian.Uint64(src[8:16]),
	}
}

func putDataHeader(dst []byte, h dataHeader) {
	putObjectHeader(dst[0:16], h.object)
	binary.LittleEndian.PutUint64(dst[16:24], h.hash)
	binary.LittleEndian.PutUint64(dst[24:32], h.nextHashOffset)
	binary.LittleEndian.PutUint64(dst[32:40], h.nextFieldOffset)
	binary.LittleEndian.PutUint64(dst[40:48], h.entryOffset)
	binary.LittleEndian.PutUint64(dst[48:56], h.entryArrayOffset)
	binary.LittleEndian.PutUint64(dst[56:64], h.nEntries)
}

func parseDataHeader(src []byte) (dataHeader, error) {
	if len(src) < dataObjectHeaderSize {
		return dataHeader{}, errInvalidJournal
	}
	oh, err := parseObjectHeader(src[0:16])
	if err != nil {
		return dataHeader{}, err
	}
	return dataHeader{
		object:           oh,
		hash:             binary.LittleEndian.Uint64(src[16:24]),
		nextHashOffset:   binary.LittleEndian.Uint64(src[24:32]),
		nextFieldOffset:  binary.LittleEndian.Uint64(src[32:40]),
		entryOffset:      binary.LittleEndian.Uint64(src[40:48]),
		entryArrayOffset: binary.LittleEndian.Uint64(src[48:56]),
		nEntries:         binary.LittleEndian.Uint64(src[56:64]),
	}, nil
}

func putFieldHeader(dst []byte, h fieldHeader) {
	putObjectHeader(dst[0:16], h.object)
	binary.LittleEndian.PutUint64(dst[16:24], h.hash)
	binary.LittleEndian.PutUint64(dst[24:32], h.nextHashOffset)
	binary.LittleEndian.PutUint64(dst[32:40], h.headDataOffset)
}

func parseFieldHeader(src []byte) (fieldHeader, error) {
	if len(src) < fieldObjectHeaderSize {
		return fieldHeader{}, errInvalidJournal
	}
	oh, err := parseObjectHeader(src[0:16])
	if err != nil {
		return fieldHeader{}, err
	}
	return fieldHeader{
		object:         oh,
		hash:           binary.LittleEndian.Uint64(src[16:24]),
		nextHashOffset: binary.LittleEndian.Uint64(src[24:32]),
		headDataOffset: binary.LittleEndian.Uint64(src[32:40]),
	}, nil
}

func putOffsetArrayHeader(dst []byte, h offsetArrayHeader) {
	putObjectHeader(dst[0:16], h.object)
	binary.LittleEndian.PutUint64(dst[16:24], h.nextArrayOffset)
}

func parseOffsetArrayHeader(src []byte) (offsetArrayHeader, error) {
	if len(src) < offsetArrayObjectHeaderSize {
		return offsetArrayHeader{}, errInvalidJournal
	}
	oh, err := parseObjectHeader(src[0:16])
	if err != nil {
		return offsetArrayHeader{}, err
	}
	return offsetArrayHeader{object: oh, nextArrayOffset: binary.LittleEndian.Uint64(src[16:24])}, nil
}

func putEntryHeader(dst []byte, h entryHeader) {
	putObjectHeader(dst[0:16], h.object)
	binary.LittleEndian.PutUint64(dst[16:24], h.seqnum)
	binary.LittleEndian.PutUint64(dst[24:32], h.realtime)
	binary.LittleEndian.PutUint64(dst[32:40], h.monotonic)
	copy(dst[40:56], h.bootID[:])
	binary.LittleEndian.PutUint64(dst[56:64], h.xorHash)
}
