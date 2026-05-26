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
	f, cleanupPath, err := openJournalFile(path)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: corrupt or unreadable file: %v", err)}
	}
	defer closeJournalFile(f, cleanupPath)

	data, err := io.ReadAll(f)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal verification failed: read error: %v", err)}
	}

	if len(data) < headerMinSize {
		return &VerificationError{Reason: "journal verification failed: file too small"}
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

// parseVerificationKey parses a systemd-style verification key.
// Format: 12 seed bytes as hex pairs (optional dash separators), then /START_HEX-INTERVAL_HEX.
func parseVerificationKey(key string) (seed [12]byte, start, interval uint64, err error) {
	var seedBytes []byte
	i := 0
	for c := 0; c < 12; c++ {
		for i < len(key) && key[i] == '-' {
			i++
		}
		if i+2 > len(key) {
			return seed, 0, 0, fmt.Errorf("invalid verification key: seed too short")
		}
		b, err := strconv.ParseUint(key[i:i+2], 16, 8)
		if err != nil {
			return seed, 0, 0, fmt.Errorf("invalid verification key: bad seed hex")
		}
		seedBytes = append(seedBytes, byte(b))
		i += 2
	}
	if len(seedBytes) != 12 {
		return seed, 0, 0, fmt.Errorf("invalid verification key: seed length mismatch")
	}
	copy(seed[:], seedBytes)

	if i >= len(key) || key[i] != '/' {
		return seed, 0, 0, fmt.Errorf("invalid verification key: missing / separator")
	}
	i++

	next, ok := consumeVerificationHex(key, i)
	if !ok || next >= len(key) || key[next] != '-' {
		return seed, 0, 0, fmt.Errorf("invalid verification key: bad start hex")
	}
	start, err = strconv.ParseUint(key[i:next], 16, 64)
	if err != nil {
		return seed, 0, 0, fmt.Errorf("invalid verification key: bad start hex")
	}

	i = next + 1
	next, ok = consumeVerificationHex(key, i)
	if !ok {
		return seed, 0, 0, fmt.Errorf("invalid verification key: bad interval hex")
	}
	interval, err = strconv.ParseUint(key[i:next], 16, 64)
	if err != nil {
		return seed, 0, 0, fmt.Errorf("invalid verification key: bad interval hex")
	}
	if next != len(key) {
		return seed, 0, 0, fmt.Errorf("invalid verification key: trailing data")
	}
	if interval == 0 {
		return seed, 0, 0, fmt.Errorf("invalid verification key: zero interval")
	}

	return seed, start, interval, nil
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

func verifySealed(data []byte, header journalHeader, seed [12]byte, startEpoch, intervalUsec uint64) error {
	isCompact := header.incompatibleFlags&incompatibleCompact != 0

	msk, mpk, err := fsprgGenMK(seed[:], fsprgRecommendedSecpar)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("FSS setup failed: %v", err)}
	}
	state0 := fsprgGenState0(mpk, seed[:])

	headerSize := header.headerSize
	tailObjectOffset := header.tailObjectOffset
	fileSize := uint64(len(data))
	if headerSize < headerMinSize || headerSize > fileSize {
		return &VerificationError{Reason: fmt.Sprintf("invalid header_size %d", headerSize)}
	}

	var (
		nObjects          uint64
		nEntries          uint64
		nTags             uint64
		nData             uint64
		nFields           uint64
		nEntryArrays      uint64
		nDataHashTables   uint64
		nFieldHashTables  uint64
		lastTagEnd        uint64
		lastEpoch         uint64
		lastTagRealtime   uint64
		entrySeqnum       uint64
		entrySeqnumSet    bool
		entryMonotonic    uint64
		entryMonotonicSet bool
		entryBootID       UUID
		entryRealtime     uint64
		entryRealtimeSet  bool
		maxEntryRealtime  uint64
		minEntryRealtime  uint64 = ^uint64(0)
	)

	p := headerSize
	for {
		if tailObjectOffset == 0 {
			break
		}
		if p > tailObjectOffset {
			return &VerificationError{Reason: fmt.Sprintf("object offset %d exceeds tail_object_offset %d", p, tailObjectOffset)}
		}
		if p > fileSize-objectHeaderSize {
			return &VerificationError{Reason: fmt.Sprintf("object header at offset %d exceeds file bounds", p)}
		}

		typ := data[p]
		flags := data[p+1]
		size := binary.LittleEndian.Uint64(data[p+8 : p+16])
		alignedSize := align8(size)

		if size < objectHeaderSize {
			return &VerificationError{Reason: fmt.Sprintf("object size %d too small at offset %d", size, p)}
		}
		if alignedSize < size || alignedSize == 0 {
			return &VerificationError{Reason: fmt.Sprintf("object size %d overflows alignment at offset %d", size, p)}
		}
		if alignedSize > fileSize-p {
			return &VerificationError{Reason: fmt.Sprintf("object at offset %d with aligned size %d exceeds file bounds", p, alignedSize)}
		}

		compressionFlags := 0
		if flags&objectCompressedXZ != 0 {
			compressionFlags++
		}
		if flags&objectCompressedLZ4 != 0 {
			compressionFlags++
		}
		if flags&objectCompressedZSTD != 0 {
			compressionFlags++
		}
		if compressionFlags > 1 {
			return &VerificationError{Reason: fmt.Sprintf("multiple compression flags at offset %d", p)}
		}
		if flags&objectCompressedXZ != 0 && header.incompatibleFlags&incompatibleCompressedXZ == 0 {
			return &VerificationError{Reason: fmt.Sprintf("XZ object in file without XZ support at offset %d", p)}
		}
		if flags&objectCompressedLZ4 != 0 && header.incompatibleFlags&incompatibleCompressedLZ4 == 0 {
			return &VerificationError{Reason: fmt.Sprintf("LZ4 object in file without LZ4 support at offset %d", p)}
		}
		if flags&objectCompressedZSTD != 0 && header.incompatibleFlags&incompatibleCompressedZSTD == 0 {
			return &VerificationError{Reason: fmt.Sprintf("ZSTD object in file without ZSTD support at offset %d", p)}
		}
		if flags&^(objectCompressedXZ|objectCompressedLZ4|objectCompressedZSTD) != 0 {
			return &VerificationError{Reason: fmt.Sprintf("unknown object flags 0x%x at offset %d", flags, p)}
		}

		nObjects++

		switch typ {
		case objectTypeData:
			nData++
		case objectTypeField:
			nFields++
		case objectTypeEntry:
			if nTags == 0 {
				return &VerificationError{Reason: fmt.Sprintf("first entry before first tag at offset %d", p)}
			}
			eSeqnum := binary.LittleEndian.Uint64(data[p+16 : p+24])
			eRealtime := binary.LittleEndian.Uint64(data[p+24 : p+32])
			eMonotonic := binary.LittleEndian.Uint64(data[p+32 : p+40])
			var eBootID UUID
			copy(eBootID[:], data[p+40:p+56])

			if entryRealtimeSet && eRealtime < lastTagRealtime {
				return &VerificationError{Reason: fmt.Sprintf("older entry after newer tag at offset %d (%d < %d)", p, eRealtime, lastTagRealtime)}
			}

			if !entrySeqnumSet {
				if eSeqnum != header.headEntrySeqnum {
					return &VerificationError{Reason: fmt.Sprintf("head entry seqnum mismatch at offset %d", p)}
				}
			} else {
				if entrySeqnum >= eSeqnum {
					return &VerificationError{Reason: fmt.Sprintf("entry seqnum out of sync at offset %d", p)}
				}
			}
			entrySeqnum = eSeqnum
			entrySeqnumSet = true

			if entryMonotonicSet && eBootID == entryBootID && entryMonotonic > eMonotonic {
				return &VerificationError{Reason: fmt.Sprintf("entry monotonic out of sync at offset %d", p)}
			}
			entryMonotonic = eMonotonic
			entryBootID = eBootID
			entryMonotonicSet = true

			if !entryRealtimeSet {
				if eRealtime != header.headEntryRealtime {
					return &VerificationError{Reason: fmt.Sprintf("head entry realtime mismatch at offset %d", p)}
				}
			}
			entryRealtime = eRealtime
			entryRealtimeSet = true

			if eRealtime > maxEntryRealtime {
				maxEntryRealtime = eRealtime
			}
			if eRealtime < minEntryRealtime {
				minEntryRealtime = eRealtime
			}

			nEntries++

		case objectTypeDataHashTable:
			nDataHashTables++
		case objectTypeFieldHashTable:
			nFieldHashTables++
		case objectTypeEntryArray:
			nEntryArrays++
		case objectTypeTag:
			if size != objectHeaderSize+8+8+tagLength {
				return &VerificationError{Reason: fmt.Sprintf("invalid tag object size %d at offset %d", size, p)}
			}
			seqnum := binary.LittleEndian.Uint64(data[p+16 : p+24])
			epoch := binary.LittleEndian.Uint64(data[p+24 : p+32])

			if seqnum != nTags+1 {
				return &VerificationError{Reason: fmt.Sprintf("tag seqnum mismatch: got %d, want %d at offset %d", seqnum, nTags+1, p)}
			}

			sealedContinuous := header.compatibleFlags&compatibleSealedContinuous != 0
			if sealedContinuous {
				ok := nTags == 0 || (nTags == 1 && epoch == lastEpoch) || epoch == lastEpoch+1
				if !ok {
					return &VerificationError{Reason: fmt.Sprintf("epoch not continuous: got %d, last %d at offset %d", epoch, lastEpoch, p)}
				}
			} else {
				if epoch < lastEpoch {
					return &VerificationError{Reason: fmt.Sprintf("epoch out of sync: got %d, last %d at offset %d", epoch, lastEpoch, p)}
				}
			}

			rt, rtEnd, err := tagRealtimeRange(startEpoch, epoch, intervalUsec)
			if err != nil {
				return &VerificationError{Reason: err.Error()}
			}

			if entryRealtimeSet && entryRealtime >= rtEnd {
				return &VerificationError{Reason: fmt.Sprintf("entry realtime %d too late for tag end %d at offset %d", entryRealtime, rtEnd, p)}
			}
			if maxEntryRealtime >= rtEnd {
				return &VerificationError{Reason: fmt.Sprintf("max entry realtime %d too late for tag end %d at offset %d", maxEntryRealtime, rtEnd, p)}
			}
			if minEntryRealtime < rt {
				return &VerificationError{Reason: fmt.Sprintf("entry realtime %d too early for tag start %d at offset %d", minEntryRealtime, rt, p)}
			}

			// Compute HMAC
			state := fsprgSeek(state0, epoch, msk, seed[:])
			key := fsprgGetKey(state, tagLength, 0)
			hm := hmac.New(sha256.New, key)

			if nTags == 0 {
				hm.Write(data[0:16])
				hm.Write(data[24:56])
				hm.Write(data[72:96])
				hm.Write(data[104:136])
			}

			q := lastTagEnd
			if nTags == 0 {
				q = headerSize
			}

			for q <= p {
				if q > fileSize-objectHeaderSize {
					return &VerificationError{Reason: fmt.Sprintf("HMAC object header at offset %d exceeds file bounds", q)}
				}
				qTyp := data[q]
				qSize := binary.LittleEndian.Uint64(data[q+8 : q+16])
				if qSize < objectHeaderSize {
					return &VerificationError{Reason: fmt.Sprintf("HMAC object size %d too small at offset %d", qSize, q)}
				}
				qAlignedSize := align8(qSize)
				if qAlignedSize < qSize || qAlignedSize == 0 {
					return &VerificationError{Reason: fmt.Sprintf("HMAC object size %d overflows alignment at offset %d", qSize, q)}
				}
				if qAlignedSize > fileSize-q {
					return &VerificationError{Reason: fmt.Sprintf("HMAC object at offset %d with aligned size %d exceeds file bounds", q, qAlignedSize)}
				}
				hmacObject(hm, data, q, qTyp, qSize, isCompact)
				q += qAlignedSize
			}

			computed := hm.Sum(nil)
			stored := data[p+32 : p+32+tagLength]
			if !hmac.Equal(computed, stored) {
				return &VerificationError{Reason: fmt.Sprintf("tag failed verification at offset %d", p)}
			}

			nTags++
			lastTagEnd = p + alignedSize
			lastEpoch = epoch
			lastTagRealtime = rt
			minEntryRealtime = ^uint64(0)
		default:
			return &VerificationError{Reason: fmt.Sprintf("unknown object type %d at offset %d", typ, p)}
		}

		if p == tailObjectOffset {
			break
		}
		p += alignedSize
	}

	if nObjects != header.nObjects {
		return &VerificationError{Reason: fmt.Sprintf("object count mismatch: got %d, want %d", nObjects, header.nObjects)}
	}
	if nEntries != header.nEntries {
		return &VerificationError{Reason: fmt.Sprintf("entry count mismatch: got %d, want %d", nEntries, header.nEntries)}
	}
	if nTags != header.nTags {
		return &VerificationError{Reason: fmt.Sprintf("tag count mismatch: got %d, want %d", nTags, header.nTags)}
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
