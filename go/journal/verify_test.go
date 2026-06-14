package journal

import (
	"bytes"
	"encoding/binary"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/klauspost/compress/zstd"
)

func TestVerifyFileDetectsCorruption(t *testing.T) {
	path := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "corrupted", "zstd-truncated-frame.zst")
	err := VerifyFile(path)
	if err == nil {
		t.Fatal("expected verification error for truncated zstd frame, got nil")
	}
	if !strings.Contains(err.Error(), "corrupt") {
		t.Fatalf("expected error to contain 'corrupt', got: %v", err)
	}
}

func TestVerifyFilePassesOnValidFixture(t *testing.T) {
	path := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "no-rtc", "system.journal.zst")
	err := VerifyFile(path)
	if err != nil {
		t.Fatalf("expected verification to pass for valid fixture, got: %v", err)
	}
}

func TestVerifyFileAndKeyWorkWithTinyReaderWindows(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", strings.Repeat("bounded verifier ", 128)),
		StringField("PRIORITY", "6"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", strings.Repeat("second bounded verifier ", 128)),
	}, EntryOptions{RealtimeUsec: 2500000}); err != nil {
		t.Fatalf("append second entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat journal: %v", err)
	}

	for _, mode := range []ReaderAccessMode{ReaderAccessReadAt, ReaderAccessMmap} {
		t.Run(accessModeName(mode), func(t *testing.T) {
			readerOpts := DefaultReaderOptions().
				WithSnapshot(true).
				WithAccessMode(mode).
				WithWindowSize(4096).
				WithMaxWindows(1)
			if uint64(info.Size()) <= readerOpts.WindowSize {
				t.Fatalf("test journal size %d must exceed forced window size %d", info.Size(), readerOpts.WindowSize)
			}
			if err := verifyFileWithOptions(path, readerOpts); err != nil {
				t.Fatalf("bounded VerifyFile failed: %v", err)
			}
			if err := verifyFileWithKeyOptions(path, testVerificationKey(opts.Seal), readerOpts); err != nil {
				t.Fatalf("bounded VerifyFileWithKey failed: %v", err)
			}
		})
	}
}

func TestVerifyFileWithKeySealedBasic(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "hello sealed world"),
		StringField("PRIORITY", "6"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	if err := VerifyFileWithKey(path, key); err != nil {
		t.Fatalf("VerifyFileWithKey failed: %v", err)
	}

	zstPath := path + ".zst"
	writeZstdFile(t, path, zstPath)
	if err := VerifyFileWithKey(zstPath, key); err != nil {
		t.Fatalf("VerifyFileWithKey failed for zstd-compressed sealed file: %v", err)
	}
}

func TestVerifyFileWithKeySealedWrongKeyFails(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "hello"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	wrongKey := "000000000000000000000001/1-f4240"
	err = VerifyFileWithKey(path, wrongKey)
	if err == nil {
		t.Fatal("expected VerifyFileWithKey to fail with wrong key")
	}
	if !strings.Contains(err.Error(), "tag failed verification") {
		t.Fatalf("expected 'tag failed verification', got: %v", err)
	}
}

func TestVerifyFileWithKeySealedTamperedDataFails(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "sealed-covered"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "later-entry"),
	}, EntryOptions{RealtimeUsec: 2500000}); err != nil {
		t.Fatalf("append later entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	tamperDataPayload(t, path, []byte("MESSAGE=sealed-covered"))

	key := testVerificationKey(opts.Seal)
	err = VerifyFileWithKey(path, key)
	if err == nil {
		t.Fatal("expected VerifyFileWithKey to fail with tampered data")
	}
	if !strings.Contains(err.Error(), "DATA hash mismatch") {
		t.Fatalf("expected DATA hash verification failure, got: %v", err)
	}
}

func TestVerifyFileWithKeyCompactSealed(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts(), Compact: true}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create compact sealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "compact sealed"),
		StringField("PRIORITY", "6"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	if err := VerifyFileWithKey(path, key); err != nil {
		t.Fatalf("VerifyFileWithKey compact+sealed failed: %v", err)
	}
}

func TestVerifyFileWithKeyEmptySealedFile(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	if err := VerifyFileWithKey(path, key); err != nil {
		t.Fatalf("VerifyFileWithKey empty sealed file failed: %v", err)
	}
}

func TestVerifyFileWithKeyUnsealedFile(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("create unsealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "unsealed"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(testSealOpts())
	if err := VerifyFileWithKey(path, key); err != nil {
		t.Fatalf("VerifyFileWithKey unsealed file failed: %v", err)
	}
}

func TestVerifyFileWithKeyMalformedTagEpoch(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "hello"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}

	header, _ := parseHeader(data[:headerSize])
	offset := uint64(headerSize)
	for offset < header.tailObjectOffset {
		size := binary.LittleEndian.Uint64(data[offset+8 : offset+16])
		alignedSize := align8(size)
		if data[offset] == objectTypeTag {
			// Corrupt epoch to an impossible value
			binary.LittleEndian.PutUint64(data[offset+24:offset+32], 0xdeadbeef)
			break
		}
		offset += alignedSize
	}
	if err := os.WriteFile(path, data, 0o640); err != nil {
		t.Fatalf("write tampered file: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	err = VerifyFileWithKey(path, key)
	if err == nil {
		t.Fatal("expected VerifyFileWithKey to fail with malformed tag epoch")
	}
	if !strings.Contains(err.Error(), "tag failed verification") {
		t.Fatalf("expected 'tag failed verification', got: %v", err)
	}
}

func TestVerifyFileWithKeyRejectsAlignedSizeOverflow(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "alignment-overflow"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	header, err := parseHeader(data[:headerSize])
	if err != nil {
		t.Fatalf("parse header: %v", err)
	}
	binary.LittleEndian.PutUint64(data[header.headerSize+8:header.headerSize+16], ^uint64(0))
	if err := os.WriteFile(path, data, 0o640); err != nil {
		t.Fatalf("write malformed file: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	err = VerifyFileWithKey(path, key)
	if err == nil {
		t.Fatal("expected VerifyFileWithKey to fail with aligned-size overflow")
	}
	if !strings.Contains(err.Error(), "overflows alignment") {
		t.Fatalf("expected alignment overflow error, got: %v", err)
	}
}

func tamperDataPayload(t *testing.T, path string, payload []byte) {
	t.Helper()

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	header, err := parseHeader(data[:headerSize])
	if err != nil {
		t.Fatalf("parse header: %v", err)
	}
	target := findTamperDataTarget(t, data, header, payload)
	assertTamperTargetCovered(t, target, payload)
	data[target.payloadOffset] ^= 0x01
	if err := os.WriteFile(path, data, 0o640); err != nil {
		t.Fatalf("write tampered file: %v", err)
	}
}

type tamperDataTarget struct {
	payloadOffset uint64
	objectOffset  uint64
	secondTag     uint64
}

func findTamperDataTarget(t *testing.T, data []byte, header journalHeader, payload []byte) tamperDataTarget {
	t.Helper()
	target := tamperDataTarget{}
	tagCount := 0

	for offset := header.headerSize; ; {
		size, alignedSize := tamperObjectSize(t, data, offset)
		recordTamperObject(data, header, payload, offset, size, &tagCount, &target)
		if offset == header.tailObjectOffset {
			return target
		}
		offset += alignedSize
	}
}

func tamperObjectSize(t *testing.T, data []byte, offset uint64) (uint64, uint64) {
	t.Helper()
	if offset+objectHeaderSize > uint64(len(data)) {
		t.Fatalf("object header at %d exceeds file", offset)
	}
	size := binary.LittleEndian.Uint64(data[offset+8 : offset+16])
	if size < objectHeaderSize {
		t.Fatalf("invalid object size %d at %d", size, offset)
	}
	alignedSize := align8(size)
	if offset+alignedSize > uint64(len(data)) {
		t.Fatalf("object at %d exceeds file", offset)
	}
	return size, alignedSize
}

func recordTamperObject(data []byte, header journalHeader, payload []byte, offset uint64, size uint64, tagCount *int, target *tamperDataTarget) {
	switch data[offset] {
	case objectTypeTag:
		*tagCount = *tagCount + 1
		if *tagCount == 2 {
			target.secondTag = offset
		}
	case objectTypeData:
		recordTamperDataObject(data, header, payload, offset, size, target)
	}
}

func recordTamperDataObject(data []byte, header journalHeader, payload []byte, offset uint64, size uint64, target *tamperDataTarget) {
	payloadOffset := uint64(dataObjectHeaderSize)
	if header.incompatibleFlags&incompatibleCompact != 0 {
		payloadOffset = uint64(compactDataObjectHeaderSize)
	}
	if size <= payloadOffset {
		return
	}
	start := offset + payloadOffset
	end := offset + size
	if bytes.Equal(data[start:end], payload) {
		target.payloadOffset = start
		target.objectOffset = offset
	}
}

func assertTamperTargetCovered(t *testing.T, target tamperDataTarget, payload []byte) {
	t.Helper()
	if target.payloadOffset == 0 {
		t.Fatalf("payload %q not found", payload)
	}
	if target.secondTag == 0 {
		t.Fatalf("second TAG not found; DATA payload would not be authenticated yet")
	}
	if target.objectOffset >= target.secondTag {
		t.Fatalf("DATA object at %d is not covered by second TAG at %d", target.objectOffset, target.secondTag)
	}
}

func writeZstdFile(t *testing.T, srcPath, dstPath string) {
	t.Helper()
	src, err := os.ReadFile(srcPath)
	if err != nil {
		t.Fatalf("read source journal: %v", err)
	}
	enc, err := zstd.NewWriter(nil, zstd.WithEncoderLevel(zstd.SpeedFastest))
	if err != nil {
		t.Fatalf("create zstd encoder: %v", err)
	}
	compressed := enc.EncodeAll(src, nil)
	enc.Close()
	if err := os.WriteFile(dstPath, compressed, 0o640); err != nil {
		t.Fatalf("write zstd journal: %v", err)
	}
}
