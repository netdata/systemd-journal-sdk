package journal

import (
	"bytes"
	"encoding/binary"
	"path/filepath"
	"strings"
	"testing"
)

func TestCreateAndOpenCompressedDataAlgorithms(t *testing.T) {
	tests := []struct {
		name             string
		compression      int
		incompatibleFlag uint32
		objectFlag       uint8
	}{
		{"zstd", CompressionZSTD, incompatibleCompressedZSTD, objectCompressedZSTD},
		{"xz", CompressionXZ, incompatibleCompressedXZ, objectCompressedXZ},
		{"lz4", CompressionLZ4, incompatibleCompressedLZ4, objectCompressedLZ4},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			runCompressedDataAlgorithmCase(t, tc.name, tc.compression, tc.incompatibleFlag, tc.objectFlag)
		})
	}
}

func runCompressedDataAlgorithmCase(t *testing.T, name string, compression int, incompatibleFlag uint32, objectFlag uint8) {
	t.Helper()
	path := filepath.Join(t.TempDir(), name+".journal")
	createCompressedDataJournal(t, path, compression)
	assertCompressedDataSnapshot(t, readJournalSnapshot(t, path), incompatibleFlag, objectFlag)
	assertCompressedDataReadback(t, path)
}

func createCompressedDataJournal(t *testing.T, path string, compression int) {
	t.Helper()
	opts := testOptions()
	opts.Compression = compression
	opts.CompressThresholdBytes = 16

	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	appendWriterFields(t, w, "first", compressedPayloadFields("first", "A"), EntryOptions{RealtimeUsec: 1_700_000_000_000_001, MonotonicUsec: 101})
	closeWriterForTest(t, w, "first")

	w, err = Open(path)
	if err != nil {
		t.Fatalf("Open() error = %v", err)
	}
	appendWriterFields(t, w, "second", compressedPayloadFields("second", "B"), EntryOptions{RealtimeUsec: 1_700_000_000_000_002, MonotonicUsec: 102})
	closeWriterForTest(t, w, "second")
}

func compressedPayloadFields(message string, fill string) []Field {
	return []Field{
		StringField("MESSAGE", message),
		{Name: "COMPRESSED_PAYLOAD", Value: bytes.Repeat([]byte(fill), 512)},
	}
}

func assertCompressedDataSnapshot(t *testing.T, snapshot journalSnapshot, incompatibleFlag uint32, objectFlag uint8) {
	t.Helper()
	if snapshot.header.incompatibleFlags&incompatibleFlag == 0 {
		t.Fatalf("incompatible flags %#x missing %#x", snapshot.header.incompatibleFlags, incompatibleFlag)
	}
	compressedObjects := 0
	for _, data := range snapshot.dataByPayload {
		if data.header.object.flag&objectFlag != 0 {
			compressedObjects++
		}
	}
	if compressedObjects < 2 {
		t.Fatalf("compressed DATA objects = %d, want at least 2", compressedObjects)
	}
}

func assertCompressedDataReadback(t *testing.T, path string) {
	t.Helper()
	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile() error = %v", err)
	}
	defer r.Close()
	assertNextCompressedPayload(t, r, "first", "A")
	assertNextCompressedPayload(t, r, "second", "B")
}

func assertNextCompressedPayload(t *testing.T, r *Reader, label string, fill string) {
	t.Helper()
	if err := r.Next(); err != nil {
		t.Fatalf("Next(%s) error = %v", label, err)
	}
	entry, err := r.GetEntry()
	if err != nil {
		t.Fatalf("GetEntry(%s) error = %v", label, err)
	}
	if got := string(entry.Fields["COMPRESSED_PAYLOAD"]); got != strings.Repeat(fill, 512) {
		t.Fatalf("%s payload mismatch: %q", label, got)
	}
}

func TestZstdFrameWithContentSizeAddsDecodableFrameSize(t *testing.T) {
	payload := bytes.Repeat([]byte("0123456789abcdef"), 12000)
	frame, err := zstdCompress(payload)
	if err != nil {
		t.Fatalf("zstdCompress() error = %v", err)
	}
	if len(frame) < 9 {
		t.Fatalf("compressed frame too short: %d", len(frame))
	}
	if !bytes.Equal(frame[:4], []byte{0x28, 0xb5, 0x2f, 0xfd}) {
		t.Fatalf("unexpected zstd magic: %x", frame[:4])
	}
	if frame[4]>>6 != 2 {
		t.Fatalf("frame content-size flag = %d, want 2 for %d-byte payload", frame[4]>>6, len(payload))
	}
	if frame[4]&(1<<5) == 0 {
		t.Fatal("single-segment flag was not set")
	}
	gotSize := binary.LittleEndian.Uint32(frame[5:9])
	if gotSize != uint32(len(payload)) {
		t.Fatalf("frame content size = %d, want %d", gotSize, len(payload))
	}
	decoded, err := zstdDecompress(frame)
	if err != nil {
		t.Fatalf("zstdDecompress() error = %v", err)
	}
	if !bytes.Equal(decoded, payload) {
		t.Fatal("decoded zstd payload does not match original")
	}
}

func TestZstdFrameWithContentSizeLeavesUnsupportedFramesUnchanged(t *testing.T) {
	invalid := []byte{0, 1, 2, 3, 4, 5}
	if got := zstdFrameWithContentSize(invalid, 16); !bytes.Equal(got, invalid) {
		t.Fatal("invalid frame was changed")
	}

	payload := bytes.Repeat([]byte("zstd"), 128)
	patched, err := zstdCompress(payload)
	if err != nil {
		t.Fatalf("zstdCompress() error = %v", err)
	}
	if got := zstdFrameWithContentSize(patched, len(payload)); !bytes.Equal(got, patched) {
		t.Fatal("already patched frame was changed")
	}

	dictionaryFrame := append([]byte(nil), patched...)
	dictionaryFrame[4] &^= 0xc0
	dictionaryFrame[4] &^= 1 << 5
	dictionaryFrame[4] |= 1
	if got := zstdFrameWithContentSize(dictionaryFrame, len(payload)); !bytes.Equal(got, dictionaryFrame) {
		t.Fatal("dictionary-id frame was changed")
	}
}

func TestCompressionThresholdSystemdPolicy(t *testing.T) {
	tests := []struct {
		name              string
		configured        int
		payloadLen        int
		wantThreshold     int
		wantCompressedObj bool
	}{
		{
			name:              "default leaves byte below systemd threshold uncompressed",
			payloadLen:        defaultCompressThreshold - 1,
			wantThreshold:     defaultCompressThreshold,
			wantCompressedObj: false,
		},
		{
			name:              "default compresses at exact systemd threshold",
			payloadLen:        defaultCompressThreshold,
			wantThreshold:     defaultCompressThreshold,
			wantCompressedObj: true,
		},
		{
			name:              "positive configured threshold below systemd minimum clamps",
			configured:        1,
			payloadLen:        minCompressThreshold - 1,
			wantThreshold:     minCompressThreshold,
			wantCompressedObj: false,
		},
		{
			name:              "clamped systemd minimum still compresses eligible payload",
			configured:        1,
			payloadLen:        defaultCompressThreshold,
			wantThreshold:     minCompressThreshold,
			wantCompressedObj: true,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "threshold.journal")
			opts := testOptions()
			opts.Compression = CompressionZSTD
			opts.CompressThresholdBytes = tc.configured

			w, err := Create(path, opts)
			if err != nil {
				t.Fatalf("Create() error = %v", err)
			}
			if w.compressThreshold != tc.wantThreshold {
				t.Fatalf("compressThreshold = %d, want %d", w.compressThreshold, tc.wantThreshold)
			}
			if err := w.Append([]Field{fieldWithTotalPayloadLen(t, "F", tc.payloadLen)}, EntryOptions{
				RealtimeUsec:  1_700_000_000_000_010,
				MonotonicUsec: 110,
			}); err != nil {
				t.Fatalf("Append() error = %v", err)
			}
			if err := w.Close(); err != nil {
				t.Fatalf("Close() error = %v", err)
			}

			snapshot := readJournalSnapshot(t, path)
			gotCompressedObj := snapshotHasDataObjectFlag(snapshot, objectCompressedZSTD)
			if gotCompressedObj != tc.wantCompressedObj {
				t.Fatalf("zstd-compressed DATA object presence = %v, want %v", gotCompressedObj, tc.wantCompressedObj)
			}
			if journalctlAvailable() {
				verifyJournalctl(t, path)
			}
		})
	}
}

func TestCreateRejectsUnsupportedCompression(t *testing.T) {
	opts := testOptions()
	opts.Compression = 99
	if w, err := Create(filepath.Join(t.TempDir(), "invalid-compression.journal"), opts); err == nil {
		_ = w.Close()
		t.Fatal("Create() with unsupported compression succeeded")
	}
}
