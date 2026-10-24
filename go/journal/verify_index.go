package journal

import (
	"bytes"
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
)

// VerifyIndex verifies an immutable journal's complete native index graph before
// reuse. The caller must exclude writes and truncation for the entire call.
// Unlike VerifyFile, it requires every ENTRY/DATA link (including the final
// ENTRY), complete hash and FIELD-chain coverage, and consistent array tails.
// Unreferenced DATA and empty FIELD objects are rejected as incomplete producer
// state, even when a compatibility structural verification accepts them.
// It checks payload hashes and decodes each DATA only once, but does not verify
// sealed TAG authentication. Work and auxiliary memory scale with graph size;
// this is an offline integrity check, not an indexed-query opening operation.
// Cancellation is cooperative; opening a whole-file .zst and an individual
// decompression or file operation cannot be interrupted by the context.
func VerifyIndex(ctx context.Context, path string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	r, err := openFileWithPolicy(path, DefaultReaderOptions().WithSnapshot(true), false, false)
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal index verification failed: %v", err)}
	}
	defer r.Close()
	source := &indexVerifySource{reader: r, ctx: ctx}
	strict := &indexGraphState{ctx: ctx, names: make(map[string]string), arrays: make(map[uint64]struct{}), payloads: make(map[[32]byte]indexPayloadIdentity)}
	err = verifyObjectGraphMode(source, strict)
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return err
	}
	if err != nil {
		return &VerificationError{Reason: fmt.Sprintf("journal index verification failed: %v", err)}
	}
	return ctx.Err()
}

type indexPayloadIdentity struct {
	offset     uint64
	collisions []uint64
}

type indexGraphState struct {
	ctx      context.Context
	names    map[string]string
	payloads map[[32]byte]indexPayloadIdentity
	arrays   map[uint64]struct{}
}

func (s *indexGraphState) internName(name []byte) string {
	if value, ok := s.names[string(name)]; ok {
		return value
	}
	value := string(name)
	s.names[value] = value
	return value
}

// A cursor advances once per posting. No reconstructed posting list or reverse
// edge set is needed: global ENTRY order is also every DATA posting's order.
type indexArrayCursor struct {
	array uint64
	index uint64
	used  uint64
	total uint64
	last  uint64
}

func (v *graphVerifier) indexArrayNext(c *indexArrayCursor) (uint64, error) {
	if err := v.strict.ctx.Err(); err != nil {
		return 0, err
	}
	if c.used >= c.total {
		return 0, fmt.Errorf("more postings than declared")
	}
	a, ok := v.entryArrays[c.array]
	if !ok {
		return 0, fmt.Errorf("missing ENTRY_ARRAY %d", c.array)
	}
	if c.index == uint64(len(a.items)) {
		if a.next <= c.array {
			return 0, fmt.Errorf("ENTRY_ARRAY chain ended early or points backwards")
		}
		c.array, c.index = a.next, 0
		a, ok = v.entryArrays[c.array]
		if !ok {
			return 0, fmt.Errorf("missing ENTRY_ARRAY %d", c.array)
		}
	}
	if c.index == 0 {
		if _, exists := v.strict.arrays[c.array]; exists {
			return 0, fmt.Errorf("ENTRY_ARRAY %d belongs to multiple chains", c.array)
		}
		v.strict.arrays[c.array] = struct{}{}
	}
	item := a.items[c.index]
	if item == 0 || item <= c.last {
		return 0, fmt.Errorf("ENTRY_ARRAY postings are zero or not strictly increasing")
	}
	if _, ok := v.entryObjects[item]; !ok {
		return 0, fmt.Errorf("ENTRY_ARRAY references missing ENTRY %d", item)
	}
	c.index++
	c.used++
	c.last = item
	return item, nil
}

func (v *graphVerifier) indexArrayFinish(c indexArrayCursor, tailOffset, tailUsed uint64, checkTail bool) error {
	if c.used != c.total {
		return fmt.Errorf("ENTRY_ARRAY count mismatch")
	}
	if c.total == 0 {
		if c.array != 0 || (checkTail && (tailOffset != 0 || tailUsed != 0)) {
			return fmt.Errorf("empty ENTRY_ARRAY has nonempty metadata")
		}
		return nil
	}
	a := v.entryArrays[c.array]
	if a.next != 0 {
		return fmt.Errorf("ENTRY_ARRAY continues beyond declared count")
	}
	for _, item := range a.items[c.index:] {
		if err := v.strict.ctx.Err(); err != nil {
			return err
		}
		if item != 0 {
			return fmt.Errorf("ENTRY_ARRAY has unpublished posting")
		}
	}
	if checkTail && (tailOffset != c.array || tailUsed != c.index) {
		return fmt.Errorf("ENTRY_ARRAY tail metadata mismatch")
	}
	return nil
}

func (v *graphVerifier) validateIndex() error {
	if err := v.validateIndexHashTables(); err != nil {
		return err
	}
	if err := v.validateIndexFieldChains(); err != nil {
		return err
	}
	cursors := make(map[uint64]indexArrayCursor, len(v.dataObjects))
	for off, data := range v.dataObjects {
		if err := v.strict.ctx.Err(); err != nil {
			return err
		}
		if data.nEntries == 0 {
			return fmt.Errorf("DATA %d has no ENTRY references", off)
		}
		cursors[off] = indexArrayCursor{array: data.entryArrayOffset, total: data.nEntries, last: data.entryOffset}
	}
	global := indexArrayCursor{array: v.header.entryArrayOffset, total: v.header.nEntries}
	if err := v.validateIndexReversePostings(&global, cursors); err != nil {
		return err
	}
	// Header counts, strict ordering, and membership together prove the global
	// array contains every ENTRY object exactly once.
	if err := v.indexArrayFinish(global, uint64(v.header.tailEntryArrayOffset), uint64(v.header.tailEntryArrayNEntries), v.header.headerSize >= 264); err != nil {
		return fmt.Errorf("global index: %w", err)
	}
	for off, data := range v.dataObjects {
		if err := v.strict.ctx.Err(); err != nil {
			return err
		}
		cursor := cursors[off]
		if cursor.used != data.nEntries {
			return fmt.Errorf("DATA %d has posting without matching ENTRY data", off)
		}
		// DATA's first entry is inline, rather than part of its array.
		if cursor.total > 0 {
			cursor.total--
			cursor.used--
		}
		if err := v.indexArrayFinish(cursor, uint64(data.tailEntryArrayOffset), uint64(data.tailEntryArrayNEntries), v.compacted); err != nil {
			return fmt.Errorf("DATA %d: %w", off, err)
		}
	}
	if len(v.strict.arrays) != len(v.entryArrays) {
		return fmt.Errorf("unreferenced ENTRY_ARRAY object")
	}
	return nil
}

func (v *graphVerifier) validateIndexReversePostings(global *indexArrayCursor, cursors map[uint64]indexArrayCursor) error {
	for global.used < global.total {
		off, err := v.indexArrayNext(global)
		if err != nil {
			return fmt.Errorf("global index: %w", err)
		}
		entry := v.entryObjects[off]
		for _, dataOffset := range entry.items {
			if err := v.strict.ctx.Err(); err != nil {
				return err
			}
			data, ok := v.dataObjects[dataOffset]
			if !ok {
				return fmt.Errorf("ENTRY %d references missing DATA", off)
			}
			cursor := cursors[dataOffset]
			// Reverse postings represent distinct DATA-to-ENTRY membership, not
			// the number of times an ENTRY repeats the same DATA reference.
			if cursor.used > 0 && cursor.last == off {
				continue
			}
			if cursor.used >= cursor.total {
				return fmt.Errorf("ENTRY %d has missing reverse DATA posting", off)
			}
			var posted uint64
			if cursor.used == 0 {
				posted = data.entryOffset
				cursor.used++
			} else {
				posted, err = v.indexArrayNext(&cursor)
				if err != nil {
					return fmt.Errorf("DATA %d: %w", dataOffset, err)
				}
			}
			if posted != off {
				return fmt.Errorf("DATA %d posting does not match ENTRY %d", dataOffset, off)
			}
			cursors[dataOffset] = cursor
		}
	}
	return nil
}

func (v *graphVerifier) validateIndexHashTables() error {
	for _, table := range []struct {
		offset, size uint64
		fields       bool
	}{
		{v.header.dataHashTableOffset, v.header.dataHashTableSize, false},
		{v.header.fieldHashTableOffset, v.header.fieldHashTableSize, true},
	} {
		seen := make(map[uint64]struct{})
		expected := len(v.dataObjects)
		tableType := uint8(objectTypeDataHashTable)
		if table.fields {
			expected = len(v.fieldObjects)
			tableType = objectTypeFieldHashTable
		}
		if table.offset != 0 && table.size != 0 && v.counts[tableType] != 1 {
			return fmt.Errorf("missing or duplicate native hash table object")
		}
		if table.offset == 0 || table.size == 0 {
			if expected != 0 {
				return fmt.Errorf("missing native hash table")
			}
			continue
		}
		if table.size%hashItemSize != 0 {
			return fmt.Errorf("invalid hash table size")
		}
		buckets := table.size / hashItemSize
		for bucket := uint64(0); bucket < buckets; bucket++ {
			if err := v.strict.ctx.Err(); err != nil {
				return err
			}
			if err := v.validateIndexHashBucket(table.offset, bucket, buckets, table.fields, seen); err != nil {
				return err
			}
		}
		if len(seen) != expected {
			return fmt.Errorf("objects missing from native hash table")
		}
	}
	return nil
}

func (v *graphVerifier) validateIndexHashBucket(offset, bucket, buckets uint64, fields bool, seen map[uint64]struct{}) error {
	current, err := verifySourceU64(v.source, offset+bucket*hashItemSize)
	if err != nil {
		return err
	}
	tail, err := verifySourceU64(v.source, offset+bucket*hashItemSize+8)
	if err != nil {
		return err
	}
	var last uint64
	for current != 0 {
		if err := v.strict.ctx.Err(); err != nil {
			return err
		}
		if current <= last {
			return fmt.Errorf("hash chain is not strictly increasing")
		}
		if _, ok := seen[current]; ok {
			return fmt.Errorf("object belongs to multiple hash chains")
		}
		seen[current] = struct{}{}
		var hash, next uint64
		if fields {
			field, ok := v.fieldObjects[current]
			if !ok {
				return fmt.Errorf("FIELD hash chain references missing FIELD")
			}
			hash, next = field.hash, field.nextHashOffset
		} else {
			data, ok := v.dataObjects[current]
			if !ok {
				return fmt.Errorf("DATA hash chain references missing DATA")
			}
			hash, next = data.hash, data.nextHashOffset
		}
		if hash%buckets != bucket {
			return fmt.Errorf("hash bucket mismatch")
		}
		last, current = current, next
	}
	if last != tail {
		return fmt.Errorf("hash bucket tail mismatch")
	}
	return nil
}

func (v *graphVerifier) validateIndexFieldChains() error {
	seen := make(map[uint64]struct{}, len(v.dataObjects))
	names := make(map[string]struct{}, len(v.fieldObjects))
	for _, field := range v.fieldObjects {
		if err := v.strict.ctx.Err(); err != nil {
			return err
		}
		if field.headDataOffset == 0 {
			return fmt.Errorf("FIELD has no DATA")
		}
		if _, ok := names[field.name]; ok {
			return fmt.Errorf("duplicate FIELD name")
		}
		names[field.name] = struct{}{}
		for current := field.headDataOffset; current != 0; {
			if err := v.strict.ctx.Err(); err != nil {
				return err
			}
			if _, ok := seen[current]; ok {
				return fmt.Errorf("FIELD DATA chain cycle or duplicate membership")
			}
			seen[current] = struct{}{}
			data, ok := v.dataObjects[current]
			if !ok {
				return fmt.Errorf("FIELD chain references missing DATA")
			}
			if data.nextFieldOffset != 0 && data.nextFieldOffset >= current {
				return fmt.Errorf("FIELD DATA chain does not decrease")
			}
			if data.fieldName != field.name {
				return fmt.Errorf("FIELD chain DATA name mismatch")
			}
			current = data.nextFieldOffset
		}
	}
	if len(seen) != len(v.dataObjects) {
		return fmt.Errorf("DATA objects missing from FIELD chains")
	}
	return nil
}

// Keep fixed-size payload identities, not a second copy of journal data. A
// digest match is resolved by byte comparison, so digest collisions cannot
// reject distinct values or conceal duplicate DATA objects.
func (v *graphVerifier) indexUniqueData(offset uint64, payload []byte) error {
	digest := sha256.Sum256(payload)
	previous, exists := v.strict.payloads[digest]
	if !exists {
		v.strict.payloads[digest] = indexPayloadIdentity{offset: offset}
		return nil
	}
	compare := func(previousOffset uint64) error {
		header, _, err := v.readGraphObject(previousOffset, v.header.tailObjectOffset)
		if err != nil {
			return err
		}
		encoded, err := v.source.Slice(previousOffset+v.dataObjectPayloadOffset(), header.size-v.dataObjectPayloadOffset())
		if err != nil {
			return err
		}
		decoded, err := v.dataHashPayload(previousOffset, header.flag, encoded)
		if err != nil {
			return err
		}
		if bytes.Equal(payload, decoded) {
			return fmt.Errorf("duplicate DATA payload at %d and %d", previousOffset, offset)
		}
		return nil
	}
	if err := compare(previous.offset); err != nil {
		return err
	}
	for _, other := range previous.collisions {
		if err := compare(other); err != nil {
			return err
		}
	}
	previous.collisions = append(previous.collisions, offset)
	v.strict.payloads[digest] = previous
	return nil
}
