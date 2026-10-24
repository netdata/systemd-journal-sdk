package journal

import (
	"context"
	"errors"
	"fmt"
)

var (
	// ErrSnapshotClosed indicates that a snapshot no longer owns its file.
	ErrSnapshotClosed = errors.New("journal: indexed snapshot is closed")
	// ErrSnapshotBusy indicates nested snapshot operations or payload callbacks.
	ErrSnapshotBusy = errors.New("journal: indexed snapshot callback is active")
	// ErrSnapshotUndeclared indicates a FIELD or value not declared at capture.
	ErrSnapshotUndeclared = errors.New("journal: indexed snapshot capture was not declared")
	// ErrSnapshotEntryExpired indicates use outside an entry callback.
	ErrSnapshotEntryExpired = errors.New("journal: indexed snapshot entry view expired")
)

// IndexedSnapshotOptions declares metadata to freeze while writes are excluded.
// A zero Reader uses DefaultReaderOptions. Nonzero Reader options retain their
// access mode; the bounds are always a snapshot. Declarations are copied.
type IndexedSnapshotOptions struct {
	Reader        ReaderOptions
	CaptureFields [][]byte
	CaptureValues []Field
}

// CapturedValue distinguishes an absent DATA value from an undeclared lookup.
type CapturedValue struct {
	Present    bool
	EntryCount uint64
}

type snapshotValueKey struct{ name, value string }

// IndexedSnapshot provides bounded native-index traversal over one committed
// population. Open it while the caller excludes writes/rotation/truncation.
// Subsequent append-only writes are supported. Files must have valid native
// index graphs; use VerifyIndex before reusing files after uncertain writes.
// Capture does not certify the whole graph. No lock is acquired by the SDK.
// A snapshot is single-consumer and is not safe for concurrent or nested use.
type IndexedSnapshot struct {
	reader                         *Reader
	maxObject, objectEnd, maxEntry uint64
	fields                         map[string]uint64
	values                         map[snapshotValueKey]CapturedValue
	busy, closed                   bool
	entry                          SnapshotEntry
	postingBuf                     [4096]byte
	payloadItems                   [4096]byte
}

// SnapshotEntry is a borrowed view valid only during its entry callback.
// Metadata are copied. Payload bytes are borrowed only for their own callback;
// neither view nor payload bytes may be retained. Copy needed values instead.
type SnapshotEntry struct {
	Seqnum, Realtime, Monotonic uint64
	BootID                      UUID
	snapshot                    *IndexedSnapshot
	offset                      uint64
	size                        uint64
	ctx                         context.Context
	active, inPayload           bool
}

// OpenIndexedSnapshot captures header, declared FIELD heads and exact counts.
// It does not materialize all entry offsets. Ordinary files use bounded reader
// windows; whole-file .journal.zst inputs incur the existing full staging cost.
func OpenIndexedSnapshot(ctx context.Context, path string, opts IndexedSnapshotOptions) (_ *IndexedSnapshot, err error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	ro := opts.Reader
	if ro == (ReaderOptions{}) {
		ro = DefaultReaderOptions()
	}
	r, err := openFileWithPolicy(path, ro.WithSnapshot(true), false, false)
	if err != nil {
		return nil, err
	}
	s := &IndexedSnapshot{reader: r, maxObject: r.header.tailObjectOffset, fields: make(map[string]uint64), values: make(map[snapshotValueKey]CapturedValue)}
	s.entry.snapshot = s
	defer func() {
		if err != nil {
			err = errors.Join(err, r.Close())
		}
	}()
	if err = s.captureBoundary(ctx); err != nil {
		return nil, err
	}
	for _, name := range opts.CaptureFields {
		if err = ctx.Err(); err != nil {
			return nil, err
		}
		var head uint64
		head, err = s.findField(ctx, name)
		if err != nil {
			return nil, err
		}
		s.fields[string(name)] = head
	}
	for _, field := range opts.CaptureValues {
		if err = ctx.Err(); err != nil {
			return nil, err
		}
		var off uint64
		var d dataHeader
		off, d, err = s.findData(ctx, []byte(field.Name), field.Value, true)
		if err != nil {
			return nil, err
		}
		cv := CapturedValue{Present: off != 0}
		if off != 0 {
			if err = s.validateCapturedPostings(ctx, off, d); err != nil {
				return nil, err
			}
			cv.EntryCount = d.nEntries
		}
		s.values[snapshotValueKey{field.Name, string(field.Value)}] = cv
	}
	if err = ctx.Err(); err != nil {
		return nil, err
	}
	return s, nil
}

// EntryCount is the frozen number of committed entries.
func (s *IndexedSnapshot) EntryCount() uint64 { return s.reader.header.nEntries }

// IsArchived reports whether the captured header was archived. The result stays
// fixed if the file is archived later; it does not certify index integrity.
func (s *IndexedSnapshot) IsArchived() bool { return s.reader.header.state == stateArchived }

// CapturedValue returns an immutable count, or ErrSnapshotUndeclared.
func (s *IndexedSnapshot) CapturedValue(name, value []byte) (CapturedValue, error) {
	if s.closed {
		return CapturedValue{}, ErrSnapshotClosed
	}
	cv, ok := s.values[snapshotValueKey{string(name), string(value)}]
	if !ok {
		return CapturedValue{}, ErrSnapshotUndeclared
	}
	return cv, nil
}

// Close releases all file windows/handles. It is idempotent, including after a
// traversal error. Close from a callback is rejected without closing the file.
func (s *IndexedSnapshot) Close() error {
	if s.busy {
		return ErrSnapshotBusy
	}
	if s.closed {
		return nil
	}
	s.closed = true
	return s.reader.Close()
}

func (s *IndexedSnapshot) begin(ctx context.Context) error {
	if s.closed {
		return ErrSnapshotClosed
	}
	if s.busy {
		return ErrSnapshotBusy
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	s.busy = true
	return nil
}

func snapshotCorrupt(what string) error {
	return fmt.Errorf("%w: indexed snapshot %s", errInvalidJournal, what)
}

func (s *IndexedSnapshot) checkOffset(off uint64) error {
	if off < s.reader.header.headerSize || off%8 != 0 || off > s.maxObject {
		return snapshotCorrupt("object offset outside committed bounds")
	}
	return nil
}

func (s *IndexedSnapshot) checkObject(off, size uint64) error {
	if err := s.checkOffset(off); err != nil {
		return err
	}
	if size < objectHeaderSize || off > s.objectEnd || size > s.objectEnd-off {
		return snapshotCorrupt("object extends past committed bounds")
	}
	return nil
}

func (s *IndexedSnapshot) readData(off uint64) (dataHeader, error) {
	if err := s.checkOffset(off); err != nil {
		return dataHeader{}, err
	}
	d, err := s.reader.readDataHeaderAt(off)
	if err != nil {
		return d, err
	}
	return d, s.checkObject(off, d.object.size)
}

// readPayloadHeader avoids parsing mutable hash/posting metadata when only the
// payload's type, compression flags and committed extent are needed.
func (s *IndexedSnapshot) readPayloadHeader(off uint64) (objectHeader, error) {
	if err := s.checkOffset(off); err != nil {
		return objectHeader{}, err
	}
	buf, err := s.reader.readSlice(off, objectHeaderSize)
	if err != nil {
		return objectHeader{}, err
	}
	header, err := parseObjectHeader(buf)
	if err != nil {
		return header, err
	}
	return header, s.checkObject(off, header.size)
}

func (s *IndexedSnapshot) arrayHeader(off uint64) (offsetArrayHeader, uint64, error) {
	if err := s.checkOffset(off); err != nil {
		return offsetArrayHeader{}, 0, err
	}
	a, capacity, err := s.reader.readOffsetArrayHeader(off)
	if err != nil {
		return a, 0, err
	}
	if err := s.checkObject(off, a.object.size); err != nil {
		return a, 0, err
	}
	if capacity == 0 || (a.nextArrayOffset != 0 && a.nextArrayOffset <= off) {
		return a, 0, snapshotCorrupt("invalid entry-array progress")
	}
	return a, capacity, nil
}
