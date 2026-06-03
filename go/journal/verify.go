package journal

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"hash"
	"io"
	"strconv"
)

// VerificationError indicates a journal file failed structural integrity verification.
type VerificationError struct {
	Reason string
}

func (e *VerificationError) Error() string {
	if e == nil {
		return "verification error"
	}
	return e.Reason
}

// VerifyFile validates the structural integrity of a journal file.
// It opens the file, validates the header, and walks all entries and
// their referenced data objects. Compressed files are decompressed.
// For sealed journals, this validates structure only; use VerifyFileWithKey
// when TAG/HMAC verification is required.
func VerifyFile(path string) error {
	data, err := readJournalFileBytes(path)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: corrupt or unreadable file: %v", err)}
	}
	if err := verifyObjectGraph(data); err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: corrupt object graph: %v", err)}
	}

	r, err := OpenFile(path)
	if err != nil {
		// Any open or decompression failure is a verification failure.
		msg := fmt.Sprintf("journal verification failed: corrupt or unreadable file: %v", err)
		return &VerificationError{Reason: msg}
	}
	defer r.Close()

	if err := r.SeekHead(); err != nil {
		return &VerificationError{
			Reason: fmt.Sprintf("journal verification failed: seek failed: %v", err),
		}
	}

	var lastMonotonic uint64
	var lastBootID UUID
	entryMonotonicSet := false
	for {
		ok, err := r.Step()
		if err != nil {
			return &VerificationError{
				Reason: fmt.Sprintf("journal verification failed: corrupt entry chain: %v", err),
			}
		}
		if !ok {
			break
		}
		entry, err := r.GetEntry()
		if err != nil {
			return &VerificationError{
				Reason: fmt.Sprintf("journal verification failed: corrupt entry data: %v", err),
			}
		}
		if entryMonotonicSet && entry.BootID == lastBootID && lastMonotonic > entry.Monotonic {
			return &VerificationError{
				Reason: fmt.Sprintf("journal verification failed: entry monotonic out of sync (%d > %d)", lastMonotonic, entry.Monotonic),
			}
		}
		lastMonotonic = entry.Monotonic
		lastBootID = entry.BootID
		entryMonotonicSet = true
	}
	return nil
}

// VerifyFileWithKey validates the integrity of a journal file.
// For sealed files, it parses the verification key and validates TAG/HMAC chains.
// For unsealed files, it behaves like VerifyFile.
func VerifyFileWithKey(path string, verificationKey string) error {
	data, err := readJournalFileBytes(path)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: corrupt or unreadable file: %v", err)}
	}

	if len(data) < headerMinSize {
		return &VerificationError{Reason: "journal verification failed: file too small"}
	}
	if err := verifyObjectGraph(data); err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: corrupt object graph: %v", err)}
	}

	headerBytes := data
	if len(headerBytes) > headerSize {
		headerBytes = headerBytes[:headerSize]
	}
	header, err := parseHeader(headerBytes)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: invalid header: %v", err)}
	}

	sealed := header.compatibleFlags&compatibleSealed != 0
	if !sealed {
		return VerifyFile(path)
	}

	seed, startEpoch, intervalUsec, err := parseVerificationKey(verificationKey)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: %v", err)}
	}

	if err := verifySealed(data, header, seed, startEpoch, intervalUsec); err != nil {
		return err
	}
	return VerifyFile(path)
}

func readJournalFileBytes(path string) ([]byte, error) {
	f, cleanupPath, err := openJournalFile(path)
	if err != nil {
		return nil, err
	}
	defer closeJournalFile(f, cleanupPath)

	return io.ReadAll(f)
}

// parseVerificationKey parses a systemd-style verification key.
// Format: 12 seed bytes as hex pairs (optional dash separators), then /START_HEX-INTERVAL_HEX.
func parseVerificationKey(key string) (seed [12]byte, start, interval uint64, err error) {
	seed, i, err := parseVerificationSeed(key)
	if err != nil {
		return seed, 0, 0, err
	}
	if i >= len(key) || key[i] != '/' {
		return seed, 0, 0, fmt.Errorf("invalid verification key: missing / separator")
	}
	start, interval, err = parseVerificationRange(key, i+1)
	return seed, start, interval, err
}

func parseVerificationSeed(key string) ([12]byte, int, error) {
	var seed [12]byte
	i := 0
	for c := range seed {
		next, b, err := parseVerificationSeedByte(key, i)
		if err != nil {
			return seed, 0, err
		}
		seed[c] = b
		i = next
	}
	return seed, i, nil
}

func parseVerificationSeedByte(key string, start int) (int, byte, error) {
	i := start
	for i < len(key) && key[i] == '-' {
		i++
	}
	if i+2 > len(key) {
		return 0, 0, fmt.Errorf("invalid verification key: seed too short")
	}
	b, err := strconv.ParseUint(key[i:i+2], 16, 8)
	if err != nil {
		return 0, 0, fmt.Errorf("invalid verification key: bad seed hex")
	}
	return i + 2, byte(b), nil
}

func parseVerificationRange(key string, startOffset int) (uint64, uint64, error) {
	start, next, err := parseVerificationHexValue(key, startOffset, "start")
	if err != nil {
		return 0, 0, err
	}
	if next >= len(key) || key[next] != '-' {
		return 0, 0, fmt.Errorf("invalid verification key: bad start hex")
	}

	interval, next, err := parseVerificationHexValue(key, next+1, "interval")
	if err != nil {
		return 0, 0, err
	}
	if next != len(key) {
		return 0, 0, fmt.Errorf("invalid verification key: trailing data")
	}
	if interval == 0 {
		return 0, 0, fmt.Errorf("invalid verification key: zero interval")
	}
	return start, interval, nil
}

func parseVerificationHexValue(key string, offset int, name string) (uint64, int, error) {
	next, ok := consumeVerificationHex(key, offset)
	if !ok {
		return 0, 0, fmt.Errorf("invalid verification key: bad %s hex", name)
	}
	value, err := strconv.ParseUint(key[offset:next], 16, 64)
	if err != nil {
		return 0, 0, fmt.Errorf("invalid verification key: bad %s hex", name)
	}
	return value, next, nil
}

func consumeVerificationHex(s string, start int) (int, bool) {
	i := start
	for i < len(s) && isHexDigit(s[i]) {
		i++
	}
	return i, i > start
}

func isHexDigit(b byte) bool {
	return ('0' <= b && b <= '9') || ('a' <= b && b <= 'f') || ('A' <= b && b <= 'F')
}

type sealedVerifyObject struct {
	offset      uint64
	typ         uint8
	flags       uint8
	size        uint64
	alignedSize uint64
}

type sealedVerifyEntry struct {
	seqnum    uint64
	realtime  uint64
	monotonic uint64
	bootID    UUID
}

type sealedVerifyState struct {
	data         []byte
	header       journalHeader
	seed         [12]byte
	msk          []byte
	state0       []byte
	startEpoch   uint64
	intervalUsec uint64
	isCompact    bool
	headerSize   uint64
	fileSize     uint64

	nObjects         uint64
	nEntries         uint64
	nTags            uint64
	nData            uint64
	nFields          uint64
	nEntryArrays     uint64
	nDataHashTables  uint64
	nFieldHashTables uint64
	lastTagEnd       uint64
	lastEpoch        uint64
	lastTagRealtime  uint64

	entrySeqnum       uint64
	entrySeqnumSet    bool
	entryMonotonic    uint64
	entryMonotonicSet bool
	entryBootID       UUID
	entryRealtime     uint64
	entryRealtimeSet  bool
	maxEntryRealtime  uint64
	minEntryRealtime  uint64
}

func verifySealed(data []byte, header journalHeader, seed [12]byte, startEpoch, intervalUsec uint64) error {
	state, err := newSealedVerifyState(data, header, seed, startEpoch, intervalUsec)
	if err != nil {
		return err
	}
	return state.run()
}

func newSealedVerifyState(data []byte, header journalHeader, seed [12]byte, startEpoch, intervalUsec uint64) (*sealedVerifyState, error) {
	fileSize := uint64(len(data))
	if header.headerSize < headerMinSize || header.headerSize > fileSize {
		return nil, &VerificationError{Reason: fmt.Sprintf("invalid header_size %d", header.headerSize)}
	}
	msk, mpk, err := fsprgGenMK(seed[:], fsprgRecommendedSecpar)
	if err != nil {
		return nil, &VerificationError{Reason: fmt.Sprintf("FSS setup failed: %v", err)}
	}
	return &sealedVerifyState{
		data:             data,
		header:           header,
		seed:             seed,
		msk:              msk,
		state0:           fsprgGenState0(mpk, seed[:]),
		startEpoch:       startEpoch,
		intervalUsec:     intervalUsec,
		isCompact:        header.incompatibleFlags&incompatibleCompact != 0,
		headerSize:       header.headerSize,
		fileSize:         fileSize,
		minEntryRealtime: ^uint64(0),
	}, nil
}

func (s *sealedVerifyState) run() error {
	p := s.headerSize
	for s.header.tailObjectOffset != 0 {
		obj, err := s.readObject(p)
		if err != nil {
			return err
		}
		if err := s.verifyObject(obj); err != nil {
			return err
		}
		if p == s.header.tailObjectOffset {
			break
		}
		p += obj.alignedSize
	}
	return s.verifyFinalCounts()
}

func (s *sealedVerifyState) readObject(offset uint64) (sealedVerifyObject, error) {
	if offset > s.header.tailObjectOffset {
		return sealedVerifyObject{}, &VerificationError{Reason: fmt.Sprintf("object offset %d exceeds tail_object_offset %d", offset, s.header.tailObjectOffset)}
	}
	if offset > s.fileSize-objectHeaderSize {
		return sealedVerifyObject{}, &VerificationError{Reason: fmt.Sprintf("object header at offset %d exceeds file bounds", offset)}
	}
	obj := sealedVerifyObject{
		offset: offset,
		typ:    s.data[offset],
		flags:  s.data[offset+1],
		size:   binary.LittleEndian.Uint64(s.data[offset+8 : offset+16]),
	}
	obj.alignedSize = align8(obj.size)
	if err := s.verifyObjectEnvelope(obj); err != nil {
		return sealedVerifyObject{}, err
	}
	if err := s.verifyObjectFlags(obj); err != nil {
		return sealedVerifyObject{}, err
	}
	return obj, nil
}

func (s *sealedVerifyState) verifyObjectEnvelope(obj sealedVerifyObject) error {
	if obj.size < objectHeaderSize {
		return &VerificationError{Reason: fmt.Sprintf("object size %d too small at offset %d", obj.size, obj.offset)}
	}
	if obj.alignedSize < obj.size || obj.alignedSize == 0 {
		return &VerificationError{Reason: fmt.Sprintf("object size %d overflows alignment at offset %d", obj.size, obj.offset)}
	}
	if obj.alignedSize > s.fileSize-obj.offset {
		return &VerificationError{Reason: fmt.Sprintf("object at offset %d with aligned size %d exceeds file bounds", obj.offset, obj.alignedSize)}
	}
	return nil
}

func (s *sealedVerifyState) verifyObjectFlags(obj sealedVerifyObject) error {
	if objectCompressionFlagCount(obj.flags) > 1 {
		return &VerificationError{Reason: fmt.Sprintf("multiple compression flags at offset %d", obj.offset)}
	}
	if err := s.verifyEnabledCompressionFlag(obj); err != nil {
		return err
	}
	if obj.flags&^(objectCompressedXZ|objectCompressedLZ4|objectCompressedZSTD) != 0 {
		return &VerificationError{Reason: fmt.Sprintf("unknown object flags 0x%x at offset %d", obj.flags, obj.offset)}
	}
	if obj.typ != objectTypeData && obj.flags != 0 {
		return &VerificationError{Reason: fmt.Sprintf("object type %d at offset %d has compression flags", obj.typ, obj.offset)}
	}
	return nil
}

func objectCompressionFlagCount(flags uint8) int {
	count := 0
	for _, flag := range []uint8{objectCompressedXZ, objectCompressedLZ4, objectCompressedZSTD} {
		if flags&flag != 0 {
			count++
		}
	}
	return count
}

func (s *sealedVerifyState) verifyEnabledCompressionFlag(obj sealedVerifyObject) error {
	if obj.flags&objectCompressedXZ != 0 && s.header.incompatibleFlags&incompatibleCompressedXZ == 0 {
		return &VerificationError{Reason: fmt.Sprintf("XZ object in file without XZ support at offset %d", obj.offset)}
	}
	if obj.flags&objectCompressedLZ4 != 0 && s.header.incompatibleFlags&incompatibleCompressedLZ4 == 0 {
		return &VerificationError{Reason: fmt.Sprintf("LZ4 object in file without LZ4 support at offset %d", obj.offset)}
	}
	if obj.flags&objectCompressedZSTD != 0 && s.header.incompatibleFlags&incompatibleCompressedZSTD == 0 {
		return &VerificationError{Reason: fmt.Sprintf("ZSTD object in file without ZSTD support at offset %d", obj.offset)}
	}
	return nil
}

func (s *sealedVerifyState) verifyObject(obj sealedVerifyObject) error {
	s.nObjects++
	switch obj.typ {
	case objectTypeData:
		s.nData++
	case objectTypeField:
		s.nFields++
	case objectTypeEntry:
		return s.verifyEntryObject(obj)
	case objectTypeDataHashTable:
		s.nDataHashTables++
	case objectTypeFieldHashTable:
		s.nFieldHashTables++
	case objectTypeEntryArray:
		s.nEntryArrays++
	case objectTypeTag:
		return s.verifyTagObject(obj)
	default:
		return &VerificationError{Reason: fmt.Sprintf("unknown object type %d at offset %d", obj.typ, obj.offset)}
	}
	return nil
}

func (s *sealedVerifyState) verifyEntryObject(obj sealedVerifyObject) error {
	if s.nTags == 0 {
		return &VerificationError{Reason: fmt.Sprintf("first entry before first tag at offset %d", obj.offset)}
	}
	entry := s.readSealedEntry(obj.offset)
	if err := s.verifyEntryRealtimeFloor(obj, entry); err != nil {
		return err
	}
	if err := s.verifyEntrySeqnum(obj, entry.seqnum); err != nil {
		return err
	}
	if err := s.verifyEntryMonotonic(obj, entry); err != nil {
		return err
	}
	if err := s.verifyEntryRealtimeHead(obj, entry.realtime); err != nil {
		return err
	}
	s.recordEntryRealtime(entry.realtime)
	s.nEntries++
	return nil
}

func (s *sealedVerifyState) readSealedEntry(offset uint64) sealedVerifyEntry {
	var entry sealedVerifyEntry
	entry.seqnum = binary.LittleEndian.Uint64(s.data[offset+16 : offset+24])
	entry.realtime = binary.LittleEndian.Uint64(s.data[offset+24 : offset+32])
	entry.monotonic = binary.LittleEndian.Uint64(s.data[offset+32 : offset+40])
	copy(entry.bootID[:], s.data[offset+40:offset+56])
	return entry
}

func (s *sealedVerifyState) verifyEntryRealtimeFloor(obj sealedVerifyObject, entry sealedVerifyEntry) error {
	if s.entryRealtimeSet && entry.realtime < s.lastTagRealtime {
		return &VerificationError{Reason: fmt.Sprintf("older entry after newer tag at offset %d (%d < %d)", obj.offset, entry.realtime, s.lastTagRealtime)}
	}
	return nil
}

func (s *sealedVerifyState) verifyEntrySeqnum(obj sealedVerifyObject, seqnum uint64) error {
	if !s.entrySeqnumSet && seqnum != s.header.headEntrySeqnum {
		return &VerificationError{Reason: fmt.Sprintf("head entry seqnum mismatch at offset %d", obj.offset)}
	}
	if s.entrySeqnumSet && s.entrySeqnum >= seqnum {
		return &VerificationError{Reason: fmt.Sprintf("entry seqnum out of sync at offset %d", obj.offset)}
	}
	s.entrySeqnum = seqnum
	s.entrySeqnumSet = true
	return nil
}

func (s *sealedVerifyState) verifyEntryMonotonic(obj sealedVerifyObject, entry sealedVerifyEntry) error {
	if s.entryMonotonicSet && entry.bootID == s.entryBootID && s.entryMonotonic > entry.monotonic {
		return &VerificationError{Reason: fmt.Sprintf("entry monotonic out of sync at offset %d", obj.offset)}
	}
	s.entryMonotonic = entry.monotonic
	s.entryBootID = entry.bootID
	s.entryMonotonicSet = true
	return nil
}

func (s *sealedVerifyState) verifyEntryRealtimeHead(obj sealedVerifyObject, realtime uint64) error {
	if !s.entryRealtimeSet && realtime != s.header.headEntryRealtime {
		return &VerificationError{Reason: fmt.Sprintf("head entry realtime mismatch at offset %d", obj.offset)}
	}
	s.entryRealtime = realtime
	s.entryRealtimeSet = true
	return nil
}

func (s *sealedVerifyState) recordEntryRealtime(realtime uint64) {
	if realtime > s.maxEntryRealtime {
		s.maxEntryRealtime = realtime
	}
	if realtime < s.minEntryRealtime {
		s.minEntryRealtime = realtime
	}
}

func (s *sealedVerifyState) verifyTagObject(obj sealedVerifyObject) error {
	if obj.size != objectHeaderSize+8+8+tagLength {
		return &VerificationError{Reason: fmt.Sprintf("invalid tag object size %d at offset %d", obj.size, obj.offset)}
	}
	seqnum := binary.LittleEndian.Uint64(s.data[obj.offset+16 : obj.offset+24])
	epoch := binary.LittleEndian.Uint64(s.data[obj.offset+24 : obj.offset+32])
	if err := s.verifyTagSeqnum(obj, seqnum); err != nil {
		return err
	}
	if err := s.verifyTagEpoch(obj, epoch); err != nil {
		return err
	}
	rt, err := s.verifyTagRealtimeWindow(obj, epoch)
	if err != nil {
		return err
	}
	if err := s.verifyTagHMAC(obj, epoch); err != nil {
		return err
	}
	s.recordTag(obj, epoch, rt)
	return nil
}

func (s *sealedVerifyState) verifyTagSeqnum(obj sealedVerifyObject, seqnum uint64) error {
	if seqnum != s.nTags+1 {
		return &VerificationError{Reason: fmt.Sprintf("tag seqnum mismatch: got %d, want %d at offset %d", seqnum, s.nTags+1, obj.offset)}
	}
	return nil
}

func (s *sealedVerifyState) verifyTagEpoch(obj sealedVerifyObject, epoch uint64) error {
	if s.header.compatibleFlags&compatibleSealedContinuous != 0 {
		return s.verifyContinuousTagEpoch(obj, epoch)
	}
	if epoch < s.lastEpoch {
		return &VerificationError{Reason: fmt.Sprintf("epoch out of sync: got %d, last %d at offset %d", epoch, s.lastEpoch, obj.offset)}
	}
	return nil
}

func (s *sealedVerifyState) verifyContinuousTagEpoch(obj sealedVerifyObject, epoch uint64) error {
	ok := s.nTags == 0 || (s.nTags == 1 && epoch == s.lastEpoch) || epoch == s.lastEpoch+1
	if !ok {
		return &VerificationError{Reason: fmt.Sprintf("epoch not continuous: got %d, last %d at offset %d", epoch, s.lastEpoch, obj.offset)}
	}
	return nil
}

func (s *sealedVerifyState) verifyTagRealtimeWindow(obj sealedVerifyObject, epoch uint64) (uint64, error) {
	rt, rtEnd, err := tagRealtimeRange(s.startEpoch, epoch, s.intervalUsec)
	if err != nil {
		return 0, &VerificationError{Reason: err.Error()}
	}
	if s.entryRealtimeSet && s.entryRealtime >= rtEnd {
		return 0, &VerificationError{Reason: fmt.Sprintf("entry realtime %d too late for tag end %d at offset %d", s.entryRealtime, rtEnd, obj.offset)}
	}
	if s.maxEntryRealtime >= rtEnd {
		return 0, &VerificationError{Reason: fmt.Sprintf("max entry realtime %d too late for tag end %d at offset %d", s.maxEntryRealtime, rtEnd, obj.offset)}
	}
	if s.minEntryRealtime < rt {
		return 0, &VerificationError{Reason: fmt.Sprintf("entry realtime %d too early for tag start %d at offset %d", s.minEntryRealtime, rt, obj.offset)}
	}
	return rt, nil
}

func (s *sealedVerifyState) verifyTagHMAC(obj sealedVerifyObject, epoch uint64) error {
	hm := s.newTagHMAC(epoch)
	if s.nTags == 0 {
		s.writeFirstTagHeaderHMAC(hm)
	}
	if err := s.writeTagObjectHMACs(hm, obj.offset); err != nil {
		return err
	}
	computed := hm.Sum(nil)
	stored := s.data[obj.offset+32 : obj.offset+32+tagLength]
	if !hmac.Equal(computed, stored) {
		return &VerificationError{Reason: fmt.Sprintf("tag failed verification at offset %d", obj.offset)}
	}
	return nil
}

func (s *sealedVerifyState) newTagHMAC(epoch uint64) hash.Hash {
	state := fsprgSeek(s.state0, epoch, s.msk, s.seed[:])
	return hmac.New(sha256.New, fsprgGetKey(state, tagLength, 0))
}

func (s *sealedVerifyState) writeFirstTagHeaderHMAC(hm hash.Hash) {
	hm.Write(s.data[0:16])
	hm.Write(s.data[24:56])
	hm.Write(s.data[72:96])
	hm.Write(s.data[104:136])
}

func (s *sealedVerifyState) writeTagObjectHMACs(hm hash.Hash, tagOffset uint64) error {
	q := s.lastTagEnd
	if s.nTags == 0 {
		q = s.headerSize
	}
	for q <= tagOffset {
		obj, err := s.readHMACObject(q)
		if err != nil {
			return err
		}
		hmacObject(hm, s.data, q, obj.typ, obj.size, s.isCompact)
		q += obj.alignedSize
	}
	return nil
}

func (s *sealedVerifyState) readHMACObject(offset uint64) (sealedVerifyObject, error) {
	if offset > s.fileSize-objectHeaderSize {
		return sealedVerifyObject{}, &VerificationError{Reason: fmt.Sprintf("HMAC object header at offset %d exceeds file bounds", offset)}
	}
	obj := sealedVerifyObject{
		offset: offset,
		typ:    s.data[offset],
		size:   binary.LittleEndian.Uint64(s.data[offset+8 : offset+16]),
	}
	obj.alignedSize = align8(obj.size)
	if obj.size < objectHeaderSize {
		return sealedVerifyObject{}, &VerificationError{Reason: fmt.Sprintf("HMAC object size %d too small at offset %d", obj.size, offset)}
	}
	if obj.alignedSize < obj.size || obj.alignedSize == 0 {
		return sealedVerifyObject{}, &VerificationError{Reason: fmt.Sprintf("HMAC object size %d overflows alignment at offset %d", obj.size, offset)}
	}
	if obj.alignedSize > s.fileSize-offset {
		return sealedVerifyObject{}, &VerificationError{Reason: fmt.Sprintf("HMAC object at offset %d with aligned size %d exceeds file bounds", offset, obj.alignedSize)}
	}
	return obj, nil
}

func (s *sealedVerifyState) recordTag(obj sealedVerifyObject, epoch, realtime uint64) {
	s.nTags++
	s.lastTagEnd = obj.offset + obj.alignedSize
	s.lastEpoch = epoch
	s.lastTagRealtime = realtime
	s.minEntryRealtime = ^uint64(0)
}

func (s *sealedVerifyState) verifyFinalCounts() error {
	if s.nObjects != s.header.nObjects {
		return &VerificationError{Reason: fmt.Sprintf("object count mismatch: got %d, want %d", s.nObjects, s.header.nObjects)}
	}
	if s.nEntries != s.header.nEntries {
		return &VerificationError{Reason: fmt.Sprintf("entry count mismatch: got %d, want %d", s.nEntries, s.header.nEntries)}
	}
	if s.nTags != s.header.nTags {
		return &VerificationError{Reason: fmt.Sprintf("tag count mismatch: got %d, want %d", s.nTags, s.header.nTags)}
	}
	return nil
}

func tagRealtimeRange(startEpoch, epoch, intervalUsec uint64) (uint64, uint64, error) {
	const maxUint64 = ^uint64(0)
	if startEpoch > maxUint64-epoch {
		return 0, 0, fmt.Errorf("tag realtime overflow")
	}
	absoluteEpoch := startEpoch + epoch
	if absoluteEpoch > maxUint64/intervalUsec {
		return 0, 0, fmt.Errorf("tag realtime overflow")
	}
	rt := absoluteEpoch * intervalUsec
	if rt > maxUint64-intervalUsec {
		return 0, 0, fmt.Errorf("tag realtime overflow")
	}
	return rt, rt + intervalUsec, nil
}

func hmacObject(hm hash.Hash, data []byte, offset uint64, typ uint8, size uint64, isCompact bool) {
	hm.Write(data[offset : offset+objectHeaderSize])

	switch typ {
	case objectTypeData:
		hm.Write(data[offset+16 : offset+24])
		payloadOffset := uint64(dataObjectHeaderSize)
		if isCompact {
			payloadOffset = uint64(compactDataObjectHeaderSize)
		}
		if size > payloadOffset {
			hm.Write(data[offset+payloadOffset : offset+size])
		}
	case objectTypeField:
		hm.Write(data[offset+16 : offset+24])
		if size > uint64(fieldObjectHeaderSize) {
			hm.Write(data[offset+uint64(fieldObjectHeaderSize) : offset+size])
		}
	case objectTypeEntry:
		if size > objectHeaderSize {
			hm.Write(data[offset+objectHeaderSize : offset+size])
		}
	case objectTypeDataHashTable, objectTypeFieldHashTable, objectTypeEntryArray:
		// nothing beyond header
	case objectTypeTag:
		hm.Write(data[offset+objectHeaderSize : offset+objectHeaderSize+16])
	}
}
