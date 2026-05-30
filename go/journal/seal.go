package journal

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"hash"
)

const (
	tagLength                  = 256 / 8
	objectTypeTag              = 7
	compatibleSealed           = 1 << 0
	compatibleSealedContinuous = 1 << 2
)

// SealOptions configures Forward Secure Sealing for a journal writer.
// Seed must be exactly 12 bytes.
type SealOptions struct {
	Seed         []byte
	IntervalUsec uint64
	// StartUsec is normalized to systemd's verification-key boundary:
	// floor(StartUsec / IntervalUsec) * IntervalUsec.
	StartUsec uint64
}

// sealState holds per-writer FSS+HMAC state.
type sealState struct {
	fsprgState  []byte
	msk         []byte
	seed        []byte
	interval    uint64
	start       uint64
	hmac        hash.Hash
	hmacRunning bool
}

func newSealState(opts SealOptions) (*sealState, error) {
	if len(opts.Seed) != fsprgRecommendedSeedlen {
		return nil, fmt.Errorf("seal seed must be %d bytes", fsprgRecommendedSeedlen)
	}
	if opts.IntervalUsec == 0 || opts.StartUsec < opts.IntervalUsec {
		return nil, fmt.Errorf("FSS start and interval must be set")
	}
	start := (opts.StartUsec / opts.IntervalUsec) * opts.IntervalUsec
	msk, mpk, err := fsprgGenMK(opts.Seed, fsprgRecommendedSecpar)
	if err != nil {
		return nil, err
	}
	state0 := fsprgGenState0(mpk, opts.Seed)
	return &sealState{
		fsprgState: state0,
		msk:        msk,
		seed:       append([]byte(nil), opts.Seed...),
		interval:   opts.IntervalUsec,
		start:      start,
	}, nil
}

func (s *sealState) getEpoch() uint64 {
	return fsprgGetEpoch(s.fsprgState)
}

func (s *sealState) getGoalEpoch(realtime uint64) (uint64, error) {
	if s.start == 0 || s.interval == 0 {
		return 0, fmt.Errorf("FSS start or interval not set")
	}
	if realtime < s.start {
		return 0, fmt.Errorf("realtime before FSS start")
	}
	return (realtime - s.start) / s.interval, nil
}

func (s *sealState) needEvolve(realtime uint64) (bool, error) {
	goal, err := s.getGoalEpoch(realtime)
	if err != nil {
		return false, err
	}
	epoch := s.getEpoch()
	if epoch > goal {
		return false, fmt.Errorf("FSS epoch %d > goal %d", epoch, goal)
	}
	return epoch != goal, nil
}

func (s *sealState) hmacStart() {
	if s.hmacRunning {
		return
	}
	key := fsprgGetKey(s.fsprgState, tagLength, 0)
	s.hmac = hmac.New(sha256.New, key)
	s.hmacRunning = true
}

func (s *sealState) hmacWrite(p []byte) {
	s.hmacStart()
	s.hmac.Write(p)
}

func (s *sealState) hmacReset() {
	s.hmacRunning = false
	s.hmac = nil
}

func (s *sealState) hmacSum() []byte {
	return s.hmac.Sum(nil)
}

// tagObjectSize returns the on-disk size of a TAG object.
func tagObjectSize() uint64 {
	return uint64(objectHeaderSize + 8 + 8 + tagLength)
}

// appendTag creates a TAG object at offset, HMACs its header/seqnum/epoch,
// stores the current HMAC digest as the tag value, and resets the HMAC cycle.
// The caller must have ensured the object fits.
func (w *Writer) appendTag() error {
	if w.seal == nil {
		return nil
	}
	w.seal.hmacStart()

	offset := w.appendOffset
	size := tagObjectSize()
	seqnum := w.header.nTags + 1
	epoch := w.seal.getEpoch()

	buf := make([]byte, align8(size))
	putObjectHeader(buf[:objectHeaderSize], objectHeader{typ: objectTypeTag, size: size})
	binary.LittleEndian.PutUint64(buf[objectHeaderSize:objectHeaderSize+8], seqnum)
	binary.LittleEndian.PutUint64(buf[objectHeaderSize+8:objectHeaderSize+16], epoch)

	// HMAC object header + seqnum + epoch (exclude tag value)
	w.seal.hmacWrite(buf[:objectHeaderSize+16])

	// Store digest
	copy(buf[objectHeaderSize+16:objectHeaderSize+16+tagLength], w.seal.hmacSum())

	if err := w.writeObject(offset, buf); err != nil {
		return err
	}
	w.objectAdded(offset, size)
	w.header.nTags = seqnum
	w.seal.hmacReset()
	return nil
}

// appendFirstTag is called after hash table setup to create the initial TAG.
func (w *Writer) appendFirstTag() error {
	if w.seal == nil {
		return nil
	}
	if err := w.hmacPutHeader(); err != nil {
		return err
	}
	if err := w.hmacPutHashTableObject(w.header.fieldHashTableOffset - uint64(objectHeaderSize)); err != nil {
		return err
	}
	if err := w.hmacPutHashTableObject(w.header.dataHashTableOffset - uint64(objectHeaderSize)); err != nil {
		return err
	}
	return w.appendTag()
}

// maybeAppendTag checks whether the entry realtime crosses a sealing interval.
// If so, it appends a tag (finalizing the old epoch) and evolves the FSPRG.
func (w *Writer) maybeAppendTag(realtime uint64) error {
	if w.seal == nil {
		return nil
	}
	need, err := w.seal.needEvolve(realtime)
	if err != nil {
		return err
	}
	if !need {
		return nil
	}
	if err := w.appendTag(); err != nil {
		return err
	}
	// Evolve across intervals, appending intermediate tags.
	for {
		goal, err := w.seal.getGoalEpoch(realtime)
		if err != nil {
			return err
		}
		epoch := w.seal.getEpoch()
		if epoch >= goal {
			break
		}
		w.seal.fsprgState = fsprgEvolve(w.seal.fsprgState)
		if w.seal.getEpoch() < goal {
			if err := w.appendTag(); err != nil {
				return err
			}
		}
	}
	return nil
}

// hmacPutHeader updates the HMAC with the immutable header byte ranges.
func (w *Writer) hmacPutHeader() error {
	if w.seal == nil {
		return nil
	}
	w.seal.hmacStart()

	// signature through just before state: bytes 0-15
	w.seal.hmacWrite(w.headerBytes(0, 16))

	// file_id through just before tail_entry_boot_id: bytes 24-55
	w.seal.hmacWrite(w.headerBytes(24, 32))

	// seqnum_id through just before arena_size: bytes 72-95 (seqnum_id[16] + header_size[8])
	w.seal.hmacWrite(w.headerBytes(72, 24))

	// data_hash_table_offset through just before tail_object_offset: bytes 104-135
	w.seal.hmacWrite(w.headerBytes(104, 32))

	return nil
}

// hmacPutHashTableObject updates the HMAC with a hash table object read from the file.
func (w *Writer) hmacPutHashTableObject(objectStart uint64) error {
	if w.seal == nil {
		return nil
	}
	w.seal.hmacStart()
	buf := make([]byte, objectHeaderSize)
	if err := w.readAt(buf, objectStart); err != nil {
		return err
	}
	// Hash table objects: only object header is immutable
	w.seal.hmacWrite(buf)
	return nil
}

// hmacPutObject updates the HMAC with the immutable bytes of an object.
// The object type determines which bytes are HMAC'd.
func (w *Writer) hmacPutObject(objectStart uint64, typ uint8) error {
	if w.seal == nil {
		return nil
	}
	w.seal.hmacStart()

	// Object header (up to payload) is always HMAC'd
	buf := make([]byte, objectHeaderSize)
	if err := w.readAt(buf, objectStart); err != nil {
		return err
	}
	w.seal.hmacWrite(buf)

	switch typ {
	case objectTypeData:
		// hash (8 bytes) + payload
		hashBuf := make([]byte, 8)
		if err := w.readAt(hashBuf, objectStart+16); err != nil {
			return err
		}
		w.seal.hmacWrite(hashBuf)
		payloadOffset := w.dataPayloadOffset()
		payloadSize := binary.LittleEndian.Uint64(buf[8:16]) - payloadOffset
		if payloadSize > 0 {
			payload := make([]byte, payloadSize)
			if err := w.readAt(payload, objectStart+payloadOffset); err != nil {
				return err
			}
			w.seal.hmacWrite(payload)
		}
	case objectTypeField:
		// hash (8 bytes) + payload
		hashBuf := make([]byte, 8)
		if err := w.readAt(hashBuf, objectStart+16); err != nil {
			return err
		}
		w.seal.hmacWrite(hashBuf)
		payloadSize := binary.LittleEndian.Uint64(buf[8:16]) - uint64(fieldObjectHeaderSize)
		if payloadSize > 0 {
			payload := make([]byte, payloadSize)
			if err := w.readAt(payload, objectStart+fieldObjectHeaderSize); err != nil {
				return err
			}
			w.seal.hmacWrite(payload)
		}
	case objectTypeEntry:
		// everything from seqnum onward
		entrySize := binary.LittleEndian.Uint64(buf[8:16])
		if entrySize > uint64(objectHeaderSize) {
			rest := make([]byte, entrySize-uint64(objectHeaderSize))
			if err := w.readAt(rest, objectStart+objectHeaderSize); err != nil {
				return err
			}
			w.seal.hmacWrite(rest)
		}
	case objectTypeDataHashTable, objectTypeFieldHashTable, objectTypeEntryArray:
		// nothing beyond object header
	case objectTypeTag:
		// seqnum + epoch
		tagMeta := make([]byte, 16)
		if err := w.readAt(tagMeta, objectStart+objectHeaderSize); err != nil {
			return err
		}
		w.seal.hmacWrite(tagMeta)
	}
	return nil
}

// headerBytes returns a slice of the in-memory header at the given offset/length.
func (w *Writer) headerBytes(off, length uint64) []byte {
	buf := make([]byte, headerSize)
	putHeader(buf, w.header)
	return buf[off : off+length]
}
