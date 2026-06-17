package journal

import (
	"encoding/hex"
	"fmt"
	"os"
)

const defaultJournalFileMode os.FileMode = 0o640

// ErrMissingMachineID is returned by Create/NewLog when the caller does not
// provide a non-zero Options.MachineID. The strict writer contract requires an
// explicit machine ID anchor.
var ErrMissingMachineID = fmt.Errorf("journal: machine id is required")

// ErrMissingBootID is returned by Create/NewLog when the caller does not
// provide a non-zero Options.BootID. The strict writer contract requires an
// explicit default boot ID anchor.
var ErrMissingBootID = fmt.Errorf("journal: boot id is required")

// validateStrictIdentity returns an error if the supplied Options struct
// lacks a non-zero machine_id or boot_id. The strict writer contract enforces
// explicit caller-supplied anchors for both fields. SDK-generated random IDs
// are no longer accepted as a default fallback.
func validateStrictIdentity(opts Options) error {
	if isZeroUUID(opts.MachineID) {
		return ErrMissingMachineID
	}
	if isZeroUUID(opts.BootID) {
		return ErrMissingBootID
	}
	return nil
}

func normalizeOptions(opts Options) (Options, error) {
	if err := validateStrictIdentity(opts); err != nil {
		return opts, err
	}
	if isZeroUUID(opts.SeqnumID) {
		opts.SeqnumID = mustRandomUUID()
	}
	if isZeroUUID(opts.FileID) {
		opts.FileID = mustRandomUUID()
	}
	if opts.HeadSeqnum == 0 {
		opts.HeadSeqnum = 1
	}
	maxFileSize := normalizeJournalMaxFileSize(opts.MaxFileSize, opts.Compact)
	if opts.MaxFileSize == 0 {
		opts.MaxFileSize = maxFileSize
	}
	if opts.DataHashTableBuckets == 0 {
		opts.DataHashTableBuckets = dataHashBucketsForMaxFileSize(maxFileSize)
	}
	if opts.FieldHashTableBuckets == 0 {
		opts.FieldHashTableBuckets = defaultFieldHashBuckets
	}
	if opts.CompressThresholdBytes == 0 {
		opts.CompressThresholdBytes = defaultCompressThreshold
	} else if opts.CompressThresholdBytes < minCompressThreshold {
		opts.CompressThresholdBytes = minCompressThreshold
	}
	if opts.FileMode == nil {
		opts.FileMode = JournalFileMode(defaultJournalFileMode)
	}
	return opts, nil
}

func normalizeOpenOptions(opts Options) Options {
	if opts.LivePublishEveryEntries == nil {
		opts.LivePublishEveryEntries = PublishEveryEntries(1)
	}
	return opts
}

func livePublishEveryEntries(opts Options) uint64 {
	if opts.LivePublishEveryEntries == nil {
		return 1
	}
	return *opts.LivePublishEveryEntries
}

func validCompression(compression int) bool {
	switch compression {
	case CompressionNone, CompressionZSTD, CompressionXZ, CompressionLZ4:
		return true
	default:
		return false
	}
}

// String returns the canonical 32-character lowercase hexadecimal UUID form
// used by journal paths and headers.
func (id UUID) String() string {
	return hex.EncodeToString(id[:])
}

func mustRandomUUID() UUID {
	id, err := NewUUID()
	if err != nil {
		panic(err)
	}
	return id
}

func isZeroUUID(id UUID) bool {
	return id == UUID{}
}

func (w *Writer) initialize(opts Options) error {
	layout := initialWriterLayout(opts)

	fileSize, ok := roundUpToFileSizeIncrease(layout.appendOffset)
	if !ok {
		return fmt.Errorf("journal initial arena too large")
	}
	if opts.Compact && fileSize > journalCompactSizeMax {
		return fmt.Errorf("compact journal cannot exceed 4 GiB")
	}

	incFlags := initialIncompatibleFlags(opts)
	compatibleFlags, err := w.initialCompatibleFlags(opts)
	if err != nil {
		return err
	}

	w.header = newInitialHeader(opts, layout, fileSize, compatibleFlags, incFlags)
	w.appendOffset = layout.appendOffset
	w.nextSeqnum = opts.HeadSeqnum

	if err := w.mapArena(fileSize); err != nil {
		return err
	}
	arenaMapped := true
	defer func() {
		if arenaMapped {
			_ = w.closeArena()
		}
	}()
	if err := w.writeHeader(); err != nil {
		return err
	}
	if err := w.writeInitialHashTableObjects(layout); err != nil {
		return err
	}

	if w.seal != nil {
		if err := w.appendFirstTag(); err != nil {
			return err
		}
	}

	arenaMapped = false
	return nil
}

func initialIncompatibleFlags(opts Options) uint32 {
	flags := uint32(incompatibleKeyedHash)
	switch opts.Compression {
	case CompressionZSTD:
		flags |= incompatibleCompressedZSTD
	case CompressionXZ:
		flags |= incompatibleCompressedXZ
	case CompressionLZ4:
		flags |= incompatibleCompressedLZ4
	}
	if opts.Compact {
		flags |= incompatibleCompact
	}
	return flags
}

func (w *Writer) initialCompatibleFlags(opts Options) (uint32, error) {
	flags := uint32(compatibleTailEntryBootID)
	if opts.Seal == nil {
		return flags, nil
	}
	seal, err := newSealState(*opts.Seal)
	if err != nil {
		return 0, err
	}
	w.seal = seal
	return flags | compatibleSealed | compatibleSealedContinuous, nil
}

func (w *Writer) writeInitialHashTableObjects(layout initialLayout) error {
	if err := w.writeObjectHeader(layout.fieldObjectOffset, objectHeader{
		typ:  objectTypeFieldHashTable,
		size: objectHeaderSize + layout.fieldSize,
	}); err != nil {
		return err
	}
	return w.writeObjectHeader(layout.dataObjectOffset, objectHeader{
		typ:  objectTypeDataHashTable,
		size: objectHeaderSize + layout.dataSize,
	})
}

type initialLayout struct {
	fieldObjectOffset uint64
	dataSize          uint64
	fieldSize         uint64
	dataObjectOffset  uint64
	fieldOffset       uint64
	dataOffset        uint64
	appendOffset      uint64
}

func initialWriterLayout(opts Options) initialLayout {
	dataSize := uint64(opts.DataHashTableBuckets * hashItemSize)
	fieldSize := uint64(opts.FieldHashTableBuckets * hashItemSize)
	fieldObjectOffset := uint64(headerSize)
	dataObjectOffset := align8(fieldObjectOffset + objectHeaderSize + fieldSize)
	return initialLayout{
		fieldObjectOffset: fieldObjectOffset,
		dataSize:          dataSize,
		fieldSize:         fieldSize,
		dataObjectOffset:  dataObjectOffset,
		fieldOffset:       fieldObjectOffset + objectHeaderSize,
		dataOffset:        dataObjectOffset + objectHeaderSize,
		appendOffset:      align8(dataObjectOffset + objectHeaderSize + dataSize),
	}
}

func newInitialHeader(opts Options, layout initialLayout, fileSize uint64, compatibleFlags, incFlags uint32) journalHeader {
	return journalHeader{
		signature:            [8]byte{'L', 'P', 'K', 'S', 'H', 'H', 'R', 'H'},
		compatibleFlags:      compatibleFlags,
		incompatibleFlags:    incFlags,
		state:                stateOnline,
		fileID:               opts.FileID,
		machineID:            opts.MachineID,
		seqnumID:             opts.SeqnumID,
		headerSize:           headerSize,
		arenaSize:            fileSize - headerSize,
		dataHashTableOffset:  layout.dataOffset,
		dataHashTableSize:    layout.dataSize,
		fieldHashTableOffset: layout.fieldOffset,
		fieldHashTableSize:   layout.fieldSize,
		tailObjectOffset:     layout.dataObjectOffset,
		nObjects:             2,
	}
}

func (w *Writer) mapArena(size uint64) error {
	arena, err := newMappedArena(w.file, size)
	if err != nil {
		return err
	}
	w.arena = arena
	return nil
}

func (w *Writer) closeArena() error {
	if w.arena == nil {
		return nil
	}
	err := w.arena.close()
	w.arena = nil
	return err
}

func (w *Writer) syncArena() error {
	if w.arena != nil {
		return w.arena.sync()
	}
	return w.file.Sync()
}

func (w *Writer) postChange() error {
	w.postChangeFence.Add(1)
	size, ok := checkedAdd(w.header.headerSize, w.header.arenaSize)
	if !ok || size > uint64(int64(^uint64(0)>>1)) {
		return fmt.Errorf("%w: journal file too large", errInvalidJournal)
	}
	return w.file.Truncate(int64(size))
}

func (w *Writer) publishAfterEntry() error {
	switch w.livePublishEveryEntries {
	case 0:
		return nil
	case 1:
		return w.postChange()
	default:
		w.entriesSinceLivePublication++
		if w.entriesSinceLivePublication >= w.livePublishEveryEntries {
			w.entriesSinceLivePublication = 0
			return w.postChange()
		}
		return nil
	}
}

func (w *Writer) readAt(dst []byte, offset uint64) error {
	if w.arena != nil {
		return w.arena.readAt(dst, offset)
	}
	_, err := w.file.ReadAt(dst, int64(offset))
	return err
}

func (w *Writer) writeAt(offset uint64, src []byte) error {
	if w.arena != nil {
		return w.arena.writeAt(offset, src)
	}
	_, err := w.file.WriteAt(src, int64(offset))
	return err
}

func (w *Writer) writeHeader() error {
	buf := make([]byte, headerSize)
	putHeader(buf, w.header)
	return w.writeAt(0, buf)
}

func (w *Writer) publishObjectMetadata() error {
	if err := w.writeUint64At(96, w.header.arenaSize); err != nil {
		return err
	}
	if err := w.writeUint64At(136, w.header.tailObjectOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(144, w.header.nObjects); err != nil {
		return err
	}
	if err := w.writeUint64At(208, w.header.nData); err != nil {
		return err
	}
	if err := w.writeUint64At(216, w.header.nFields); err != nil {
		return err
	}
	if err := w.writeUint64At(232, w.header.nEntryArrays); err != nil {
		return err
	}
	if err := w.writeUint64At(240, w.header.dataHashChainDepth); err != nil {
		return err
	}
	return w.writeUint64At(248, w.header.fieldHashChainDepth)
}

func (w *Writer) publishEntryMetadata() error {
	if err := w.writeUUIDAt(56, w.header.tailEntryBootID); err != nil {
		return err
	}
	if err := w.writeUint64At(160, w.header.tailEntrySeqnum); err != nil {
		return err
	}
	if err := w.writeUint64At(168, w.header.headEntrySeqnum); err != nil {
		return err
	}
	if err := w.writeUint64At(176, w.header.entryArrayOffset); err != nil {
		return err
	}
	if err := w.writeUint64At(184, w.header.headEntryRealtime); err != nil {
		return err
	}
	if err := w.writeUint64At(192, w.header.tailEntryRealtime); err != nil {
		return err
	}
	if err := w.writeUint64At(200, w.header.tailEntryMonotonic); err != nil {
		return err
	}
	if err := w.writeUint32At(256, w.header.tailEntryArrayOffset); err != nil {
		return err
	}
	if err := w.writeUint32At(260, w.header.tailEntryArrayNEntries); err != nil {
		return err
	}
	if err := w.writeUint64At(264, w.header.tailEntryOffset); err != nil {
		return err
	}
	return w.writeUint64At(152, w.header.nEntries)
}

func (w *Writer) writeObjectHeader(offset uint64, header objectHeader) error {
	buf := make([]byte, objectHeaderSize)
	putObjectHeader(buf, header)
	return w.writeAt(offset, buf)
}

func (w *Writer) writeObject(offset uint64, buf []byte) error {
	end, ok := checkedAdd(offset, uint64(len(buf)))
	if !ok {
		return fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	if err := w.ensureArenaSize(end); err != nil {
		return err
	}
	return w.writeAt(offset, buf)
}

func (w *Writer) newObjectBuffer(offset, size uint64) ([]byte, bool, error) {
	alignedSize := align8(size)
	end, ok := checkedAdd(offset, alignedSize)
	if !ok {
		return nil, false, fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	if err := w.ensureArenaSize(end); err != nil {
		return nil, false, err
	}
	if w.arena != nil {
		if data, ok, err := w.arena.directBytesAt(offset, alignedSize); err != nil || ok {
			return data, ok, err
		}
	}
	if alignedSize > uint64(int(^uint(0)>>1)) {
		return nil, false, fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	return make([]byte, int(alignedSize)), false, nil
}

func (w *Writer) commitObjectBuffer(offset uint64, buf []byte, direct bool) error {
	if direct {
		return nil
	}
	return w.writeAt(offset, buf)
}

func readObjectHeaderAt(f *os.File, offset uint64) (objectHeader, error) {
	buf := make([]byte, objectHeaderSize)
	if _, err := f.ReadAt(buf, int64(offset)); err != nil {
		return objectHeader{}, err
	}
	return parseObjectHeader(buf)
}

func (w *Writer) hash(payload []byte) uint64 {
	return sipHash24(w.header.fileID, payload)
}

func (w *Writer) objectAdded(offset, size uint64) error {
	if offset > ^uint64(0)-size {
		return fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	w.header.tailObjectOffset = offset
	w.appendOffset = align8(offset + size)
	w.header.nObjects++
	return w.ensureArenaSize(w.appendOffset)
}

func (w *Writer) ensureArenaSize(requiredSize uint64) error {
	oldSize := headerSize + w.header.arenaSize
	if requiredSize <= oldSize {
		return nil
	}
	newSize, ok := roundUpToFileSizeIncrease(requiredSize)
	if !ok {
		return fmt.Errorf("%w: object exceeds file bounds", errInvalidJournal)
	}
	if w.compact && newSize > journalCompactSizeMax {
		return fmt.Errorf("%w: compact journal cannot exceed 4 GiB", errInvalidJournal)
	}
	if w.arena != nil {
		if err := w.arena.remap(newSize); err != nil {
			return err
		}
	} else if err := w.file.Truncate(int64(newSize)); err != nil {
		return err
	}
	w.header.arenaSize = newSize - headerSize
	return nil
}

func (w *Writer) entryAdded(entryOffset, entrySeqnum, realtime, monotonic uint64, bootID UUID) {
	w.header.nEntries++
	if w.header.headEntrySeqnum == 0 {
		w.header.headEntrySeqnum = entrySeqnum
	}
	if w.header.headEntryRealtime == 0 {
		w.header.headEntryRealtime = realtime
	}
	w.header.tailEntrySeqnum = entrySeqnum
	w.header.tailEntryRealtime = realtime
	w.header.tailEntryMonotonic = monotonic
	w.header.tailEntryBootID = bootID
	w.header.tailEntryOffset = entryOffset
	w.nextSeqnum = entrySeqnum + 1
}
