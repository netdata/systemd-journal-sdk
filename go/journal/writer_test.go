package journal

import (
	"bytes"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

var (
	testMachineID = UUID{0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f}
	testBootID    = UUID{0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f}
	testSeqnumID  = UUID{0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f}
	testFileID    = UUID{0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f}
)

func TestCreateAppendAndReopenLayout(t *testing.T) {
	path := filepath.Join(t.TempDir(), "go-writer.journal")
	opts := testOptions()

	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}

	if err := w.Append([]Field{
		StringField("MESSAGE", "hello"),
		StringField("PRIORITY", "6"),
		StringField("SYSLOG_IDENTIFIER", "go-test"),
	}, EntryOptions{RealtimeUsec: 1_700_000_000_000_001, MonotonicUsec: 101}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	w, err = Open(path)
	if err != nil {
		t.Fatalf("Open() error = %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "hello"),
		StringField("PRIORITY", "5"),
		StringField("SYSLOG_IDENTIFIER", "go-test"),
	}, EntryOptions{RealtimeUsec: 1_700_000_000_000_002, MonotonicUsec: 102}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close(second) error = %v", err)
	}

	snapshot := readJournalSnapshot(t, path)
	if snapshot.header.state != stateOnline {
		t.Fatalf("state = %d, want online", snapshot.header.state)
	}
	if snapshot.header.nEntries != 2 {
		t.Fatalf("nEntries = %d, want 2", snapshot.header.nEntries)
	}
	if snapshot.header.headEntrySeqnum != 1 || snapshot.header.tailEntrySeqnum != 2 {
		t.Fatalf("seqnum range = %d..%d, want 1..2", snapshot.header.headEntrySeqnum, snapshot.header.tailEntrySeqnum)
	}
	if snapshot.header.entryArrayOffset == 0 {
		t.Fatal("entryArrayOffset is zero")
	}

	message := snapshot.dataByPayload["MESSAGE=hello"]
	if message.offset == 0 {
		t.Fatal("MESSAGE=hello data object not found")
	}
	if message.header.nEntries != 2 {
		t.Fatalf("MESSAGE=hello nEntries = %d, want 2", message.header.nEntries)
	}
	if message.header.entryOffset == 0 || message.header.entryArrayOffset == 0 {
		t.Fatalf("MESSAGE=hello entry links are incomplete: entry=%d array=%d", message.header.entryOffset, message.header.entryArrayOffset)
	}

	if got := len(snapshot.entries); got != 2 {
		t.Fatalf("entry object count = %d, want 2", got)
	}
	if got := len(snapshot.dataByPayload); got != 4 {
		t.Fatalf("unique data object count = %d, want 4", got)
	}
	if got := len(snapshot.fieldByPayload); got != 3 {
		t.Fatalf("unique field object count = %d, want 3", got)
	}
}

func TestWriterRejectsInvalidEntries(t *testing.T) {
	path := filepath.Join(t.TempDir(), "invalid.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	defer func() {
		if err := w.Close(); err != nil {
			t.Fatalf("Close() error = %v", err)
		}
	}()

	if err := w.Append(nil, EntryOptions{}); !errors.Is(err, errEntryEmpty) {
		t.Fatalf("Append(nil) error = %v, want errEntryEmpty", err)
	}
	if err := w.Append([]Field{StringField("lowercase", "bad")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("Append(lowercase) error = %v, want errFieldName", err)
	}
	if err := w.Append([]Field{StringField("BAD-NAME", "bad")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("Append(BAD-NAME) error = %v, want errFieldName", err)
	}
	if err := w.Append([]Field{StringField("1MESSAGE", "bad")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("Append(1MESSAGE) error = %v, want errFieldName", err)
	}
	if err := w.Append([]Field{StringField(strings.Repeat("A", 65), "bad")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("Append(long field name) error = %v, want errFieldName", err)
	}
}

func TestWriterLockRejectsSecondWriter(t *testing.T) {
	path := filepath.Join(t.TempDir(), "locked.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	defer func() {
		if err := w.Close(); err != nil {
			t.Fatalf("Close() error = %v", err)
		}
	}()

	if second, err := Open(path); err == nil {
		_ = second.Close()
		t.Fatal("Open() succeeded while first writer lock is held")
	}
}

func TestJournalctlReadsCreatedJournal(t *testing.T) {
	if _, err := exec.LookPath("journalctl"); err != nil {
		t.Skip("journalctl is not installed")
	}

	path := filepath.Join(t.TempDir(), "journalctl-readback.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "journalctl can read this"),
		StringField("PRIORITY", "6"),
		StringField("SYSLOG_IDENTIFIER", "go-writer-test"),
		StringField("_SYSTEMD_UNIT", "go-writer.service"),
	}, EntryOptions{RealtimeUsec: 1_700_000_000_000_011, MonotonicUsec: 201}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "filtered out"),
		StringField("PRIORITY", "5"),
		StringField("SYSLOG_IDENTIFIER", "go-writer-test"),
		StringField("_SYSTEMD_UNIT", "other.service"),
	}, EntryOptions{RealtimeUsec: 1_700_000_000_000_012, MonotonicUsec: 202}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	verify := exec.Command("journalctl", "--verify", "--file", path)
	if output, err := verify.CombinedOutput(); err != nil {
		t.Fatalf("journalctl --verify failed: %v\n%s", err, output)
	}

	rows := runJournalctlJSON(t, path, "_SYSTEMD_UNIT=go-writer.service")
	if len(rows) != 1 {
		t.Fatalf("filtered row count = %d, want 1; rows=%v", len(rows), rows)
	}
	assertJSONField(t, rows[0], "MESSAGE", "journalctl can read this")
	assertJSONField(t, rows[0], "PRIORITY", "6")
	assertJSONField(t, rows[0], "SYSLOG_IDENTIFIER", "go-writer-test")
	assertJSONField(t, rows[0], "_SYSTEMD_UNIT", "go-writer.service")
}

func TestOpenAppendDefaultMonotonicPreservesJournalctlVerify(t *testing.T) {
	if _, err := exec.LookPath("journalctl"); err != nil {
		t.Skip("journalctl is not installed")
	}

	path := filepath.Join(t.TempDir(), "reopen-monotonic.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	time.Sleep(20 * time.Millisecond)
	if err := w.Append([]Field{StringField("MESSAGE", "first")}, EntryOptions{}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	time.Sleep(20 * time.Millisecond)
	if err := w.Append([]Field{StringField("MESSAGE", "second")}, EntryOptions{}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close(first writer) error = %v", err)
	}

	w, err = Open(path)
	if err != nil {
		t.Fatalf("Open() error = %v", err)
	}
	time.Sleep(time.Millisecond)
	if err := w.Append([]Field{StringField("MESSAGE", "third")}, EntryOptions{}); err != nil {
		t.Fatalf("Append(third) error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close(second writer) error = %v", err)
	}

	verifyJournalctl(t, path)
	rows := runJournalctlJSON(t, path)
	if len(rows) != 3 {
		t.Fatalf("row count = %d, want 3; rows=%v", len(rows), rows)
	}
}

func TestEntryArrayGrowthAndJournalctlReadback(t *testing.T) {
	if _, err := exec.LookPath("journalctl"); err != nil {
		t.Skip("journalctl is not installed")
	}

	path := filepath.Join(t.TempDir(), "entry-array-growth.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	for i := 0; i <= initialEntryArrayCap; i++ {
		if err := w.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("growth-%04d", i)),
			StringField("PRIORITY", "6"),
		}, EntryOptions{RealtimeUsec: 1_700_000_010_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	verifyJournalctl(t, path)
	if got := runJournalctlLineCount(t, path, "PRIORITY=6"); got != initialEntryArrayCap+1 {
		t.Fatalf("filtered row count = %d, want %d", got, initialEntryArrayCap+1)
	}
}

func TestWriterBinaryFieldCompatibility(t *testing.T) {
	if _, err := exec.LookPath("journalctl"); err != nil {
		t.Skip("journalctl is not installed")
	}

	path := filepath.Join(t.TempDir(), "binary-fields.journal")
	binaryValue := []byte{0x00, 0x01, 0x02, 'A', '\n', 0x7f, 0x80, 0xff}
	matchableBinaryValue := []byte{'a', 'b', 'c', 0x07, 'd', 'e', 'f'}
	emptyBinaryValue := []byte{}

	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "binary compatibility"),
		StringField("TEST_ID", "binary-field"),
		{Name: "BINARY_PAYLOAD", Value: binaryValue},
		{Name: "BINARY_MATCH", Value: matchableBinaryValue},
		{Name: "BINARY_EMPTY", Value: emptyBinaryValue},
	}, EntryOptions{RealtimeUsec: 1_700_000_030_000_000, MonotonicUsec: 301}); err != nil {
		t.Fatalf("Append(binary) error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	payload := append([]byte("BINARY_PAYLOAD="), binaryValue...)
	matchablePayload := append([]byte("BINARY_MATCH="), matchableBinaryValue...)
	emptyPayload := append([]byte("BINARY_EMPTY="), emptyBinaryValue...)
	snapshot := readJournalSnapshot(t, path)
	if got := snapshot.dataByPayload[string(payload)].payload; !bytes.Equal(got, payload) {
		t.Fatalf("raw DATA payload = %x, want %x", got, payload)
	}
	if got := snapshot.dataByPayload[string(matchablePayload)].payload; !bytes.Equal(got, matchablePayload) {
		t.Fatalf("raw matchable DATA payload = %x, want %x", got, matchablePayload)
	}
	if got := snapshot.dataByPayload[string(emptyPayload)].payload; !bytes.Equal(got, emptyPayload) {
		t.Fatalf("raw empty DATA payload = %x, want %x", got, emptyPayload)
	}

	verifyJournalctl(t, path)

	rows := runJournalctlJSON(t, path, "TEST_ID=binary-field")
	if len(rows) != 1 {
		t.Fatalf("filtered row count = %d, want 1; rows=%v", len(rows), rows)
	}
	assertJSONByteArray(t, rows[0], "BINARY_PAYLOAD", binaryValue)
	assertJSONByteArray(t, rows[0], "BINARY_MATCH", matchableBinaryValue)
	assertJSONField(t, rows[0], "BINARY_EMPTY", "")

	rows = runJournalctlJSON(t, path, "BINARY_MATCH=abc\x07def")
	if len(rows) != 1 {
		t.Fatalf("binary match row count = %d, want 1; rows=%v", len(rows), rows)
	}

	exported := runJournalctlExport(t, path, "TEST_ID=binary-field")
	assertExportField(t, exported, "BINARY_PAYLOAD", binaryValue)
	assertExportField(t, exported, "BINARY_MATCH", matchableBinaryValue)
	assertExportField(t, exported, "BINARY_EMPTY", emptyBinaryValue)

	t.Run("libsystemd", func(t *testing.T) {
		runLibsystemdBinaryFieldReader(t, path, "BINARY_PAYLOAD", binaryValue, "TEST_ID=binary-field")
		runLibsystemdBinaryFieldReader(t, path, "BINARY_MATCH", matchableBinaryValue, "TEST_ID=binary-field")
		runLibsystemdBinaryFieldReader(t, path, "BINARY_EMPTY", emptyBinaryValue, "TEST_ID=binary-field")
	})
}

func TestHashCollisionChainsDeduplicate(t *testing.T) {
	path := filepath.Join(t.TempDir(), "hash-collisions.journal")
	opts := testOptions()
	opts.DataHashTableBuckets = 1
	opts.FieldHashTableBuckets = 1

	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	entries := [][]Field{
		{StringField("MESSAGE", "one"), StringField("PRIORITY", "6")},
		{StringField("MESSAGE", "two"), StringField("PRIORITY", "6")},
		{StringField("MESSAGE", "one"), StringField("PRIORITY", "6")},
	}
	for i, fields := range entries {
		if err := w.Append(fields, EntryOptions{RealtimeUsec: 1_700_000_020_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	snapshot := readJournalSnapshot(t, path)
	if got := len(snapshot.dataByPayload); got != 3 {
		t.Fatalf("unique data object count = %d, want 3", got)
	}
	if got := len(snapshot.fieldByPayload); got != 2 {
		t.Fatalf("unique field object count = %d, want 2", got)
	}
	if got := snapshot.dataByPayload["MESSAGE=one"].header.nEntries; got != 2 {
		t.Fatalf("MESSAGE=one nEntries = %d, want 2", got)
	}
}

func TestWriterSyncCloseAndClosedAppend(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sync-close.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.Append([]Field{StringField("MESSAGE", "sync")}, EntryOptions{}); err != nil {
		t.Fatalf("Append() error = %v", err)
	}
	if err := w.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("second Close() error = %v", err)
	}
	if err := w.Append([]Field{StringField("MESSAGE", "after close")}, EntryOptions{}); !errors.Is(err, errWriterClosed) {
		t.Fatalf("Append(after Close) error = %v, want errWriterClosed", err)
	}
	if err := w.Sync(); !errors.Is(err, errWriterClosed) {
		t.Fatalf("Sync(after Close) error = %v, want errWriterClosed", err)
	}
}

func TestAppendMapUsesDeterministicOrdering(t *testing.T) {
	path := filepath.Join(t.TempDir(), "map.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.AppendMap(map[string]string{
		"SYSLOG_IDENTIFIER": "go-test",
		"PRIORITY":          "6",
		"MESSAGE":           "ordered",
	}); err != nil {
		t.Fatalf("AppendMap() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	snapshot := readJournalSnapshot(t, path)
	if len(snapshot.entries) != 1 {
		t.Fatalf("entry count = %d, want 1", len(snapshot.entries))
	}

	gotOffsets := snapshot.entries[0].itemOffsets
	wantOffsets := append([]uint64(nil), gotOffsets...)
	for i := 1; i < len(wantOffsets); i++ {
		if wantOffsets[i-1] > wantOffsets[i] {
			t.Fatalf("entry data offsets are not sorted: %v", gotOffsets)
		}
	}
}

type journalSnapshot struct {
	header         journalHeader
	dataByPayload  map[string]dataSnapshot
	fieldByPayload map[string]fieldSnapshot
	entries        []entrySnapshot
}

type dataSnapshot struct {
	offset  uint64
	header  dataHeader
	payload []byte
}

type fieldSnapshot struct {
	offset  uint64
	header  fieldHeader
	payload []byte
}

type entrySnapshot struct {
	offset      uint64
	header      entryHeader
	itemOffsets []uint64
}

func testOptions() Options {
	return Options{
		MachineID:             testMachineID,
		BootID:                testBootID,
		SeqnumID:              testSeqnumID,
		FileID:                testFileID,
		DataHashTableBuckets:  64,
		FieldHashTableBuckets: 16,
	}
}

func readJournalSnapshot(t *testing.T, path string) journalSnapshot {
	t.Helper()

	content, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile() error = %v", err)
	}
	header, err := parseHeader(content[:headerSize])
	if err != nil {
		t.Fatalf("parseHeader() error = %v", err)
	}

	snapshot := journalSnapshot{
		header:         header,
		dataByPayload:  make(map[string]dataSnapshot),
		fieldByPayload: make(map[string]fieldSnapshot),
	}
	endOffset := uint64(len(content))
	if header.tailObjectOffset != 0 {
		if header.tailObjectOffset+objectHeaderSize > uint64(len(content)) {
			t.Fatalf("tail object offset %d exceeds file size %d", header.tailObjectOffset, len(content))
		}
		tail, err := parseObjectHeader(content[header.tailObjectOffset : header.tailObjectOffset+objectHeaderSize])
		if err != nil {
			t.Fatalf("parseObjectHeader(tail %d) error = %v", header.tailObjectOffset, err)
		}
		if tail.size < objectHeaderSize {
			t.Fatalf("tail object at offset %d has invalid size %d", header.tailObjectOffset, tail.size)
		}
		endOffset = align8(header.tailObjectOffset + tail.size)
	}
	for offset := header.headerSize; offset < endOffset; {
		if offset+objectHeaderSize > uint64(len(content)) {
			t.Fatalf("short object header at offset %d", offset)
		}
		oh, err := parseObjectHeader(content[offset : offset+objectHeaderSize])
		if err != nil {
			t.Fatalf("parseObjectHeader(%d) error = %v", offset, err)
		}
		if oh.size < objectHeaderSize {
			t.Fatalf("object at offset %d has invalid size %d", offset, oh.size)
		}
		if offset+oh.size > uint64(len(content)) {
			t.Fatalf("object at offset %d exceeds file size: size=%d file=%d", offset, oh.size, len(content))
		}

		switch oh.typ {
		case objectTypeData:
			header, err := parseDataHeader(content[offset : offset+dataObjectHeaderSize])
			if err != nil {
				t.Fatalf("parseDataHeader(%d) error = %v", offset, err)
			}
			payload := append([]byte(nil), content[offset+dataObjectHeaderSize:offset+oh.size]...)
			snapshot.dataByPayload[string(payload)] = dataSnapshot{offset: offset, header: header, payload: payload}
		case objectTypeField:
			header, err := parseFieldHeader(content[offset : offset+fieldObjectHeaderSize])
			if err != nil {
				t.Fatalf("parseFieldHeader(%d) error = %v", offset, err)
			}
			payload := append([]byte(nil), content[offset+fieldObjectHeaderSize:offset+oh.size]...)
			snapshot.fieldByPayload[string(payload)] = fieldSnapshot{offset: offset, header: header, payload: payload}
		case objectTypeEntry:
			entry := parseEntryObject(t, offset, content[offset:offset+oh.size])
			snapshot.entries = append(snapshot.entries, entry)
		case objectTypeDataHashTable, objectTypeFieldHashTable, objectTypeEntryArray:
		default:
			t.Fatalf("unexpected object type %d at offset %d", oh.typ, offset)
		}

		offset = align8(offset + oh.size)
	}
	return snapshot
}

func parseEntryObject(t *testing.T, offset uint64, content []byte) entrySnapshot {
	t.Helper()

	oh, err := parseObjectHeader(content[:objectHeaderSize])
	if err != nil {
		t.Fatalf("parseObjectHeader(entry) error = %v", err)
	}
	if oh.size < entryObjectHeaderSize || (oh.size-entryObjectHeaderSize)%regularEntryItemSize != 0 {
		t.Fatalf("entry at offset %d has invalid size %d", offset, oh.size)
	}

	header := entryHeader{
		object:    oh,
		seqnum:    le64(content[16:24]),
		realtime:  le64(content[24:32]),
		monotonic: le64(content[32:40]),
		xorHash:   le64(content[56:64]),
	}
	copy(header.bootID[:], content[40:56])

	entry := entrySnapshot{offset: offset, header: header}
	for i := entryObjectHeaderSize; i < len(content); i += regularEntryItemSize {
		entry.itemOffsets = append(entry.itemOffsets, le64(content[i:i+8]))
	}
	return entry
}

func le64(data []byte) uint64 {
	return uint64(data[0]) |
		uint64(data[1])<<8 |
		uint64(data[2])<<16 |
		uint64(data[3])<<24 |
		uint64(data[4])<<32 |
		uint64(data[5])<<40 |
		uint64(data[6])<<48 |
		uint64(data[7])<<56
}

func runJournalctlJSON(t *testing.T, path string, matches ...string) []map[string]any {
	t.Helper()

	args := append([]string{"--file", path, "--output=json", "--no-pager"}, matches...)
	cmd := exec.Command("journalctl", args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl %s failed: %v\n%s", strings.Join(args, " "), err, output)
	}

	lines := bytes.Split(bytes.TrimSpace(output), []byte{'\n'})
	rows := make([]map[string]any, 0, len(lines))
	for _, line := range lines {
		if len(line) == 0 {
			continue
		}
		var row map[string]any
		if err := json.Unmarshal(line, &row); err != nil {
			t.Fatalf("json.Unmarshal(%q) error = %v", line, err)
		}
		rows = append(rows, row)
	}
	return rows
}

func runJournalctlLineCount(t *testing.T, path string, matches ...string) int {
	t.Helper()

	args := append([]string{"--file", path, "--output=json", "--no-pager"}, matches...)
	cmd := exec.Command("journalctl", args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl %s failed: %v\n%s", strings.Join(args, " "), err, output)
	}
	output = bytes.TrimSpace(output)
	if len(output) == 0 {
		return 0
	}
	return bytes.Count(output, []byte{'\n'}) + 1
}

func runJournalctlExport(t *testing.T, path string, matches ...string) map[string][][]byte {
	t.Helper()

	args := append([]string{"--file", path, "--output=export", "--no-pager"}, matches...)
	cmd := exec.Command("journalctl", args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl %s failed: %v\n%s", strings.Join(args, " "), err, output)
	}

	entries := parseJournalExport(t, output)
	if len(entries) != 1 {
		t.Fatalf("export entry count = %d, want 1; output=%x", len(entries), output)
	}
	return entries[0]
}

func parseJournalExport(t *testing.T, data []byte) []map[string][][]byte {
	t.Helper()

	var entries []map[string][][]byte
	entry := make(map[string][][]byte)
	for pos := 0; pos < len(data); {
		lineEnd := bytes.IndexByte(data[pos:], '\n')
		if lineEnd < 0 {
			t.Fatalf("unterminated export field at offset %d", pos)
		}
		lineEnd += pos
		line := data[pos:lineEnd]
		pos = lineEnd + 1

		if len(line) == 0 {
			if len(entry) > 0 {
				entries = append(entries, entry)
				entry = make(map[string][][]byte)
			}
			continue
		}

		if eq := bytes.IndexByte(line, '='); eq >= 0 {
			name := string(line[:eq])
			value := append([]byte(nil), line[eq+1:]...)
			entry[name] = append(entry[name], value)
			continue
		}

		remaining := len(data) - pos
		if remaining < 8 {
			t.Fatalf("binary export field %q at offset %d lacks length", line, pos)
		}
		size := binary.LittleEndian.Uint64(data[pos : pos+8])
		pos += 8
		remaining = len(data) - pos
		if size > uint64(remaining) {
			t.Fatalf("binary export field %q size %d exceeds remaining %d", line, size, remaining)
		}
		value := append([]byte(nil), data[pos:pos+int(size)]...)
		pos += int(size)
		if pos >= len(data) || data[pos] != '\n' {
			t.Fatalf("binary export field %q is not newline terminated", line)
		}
		pos++
		name := string(line)
		entry[name] = append(entry[name], value)
	}
	if len(entry) > 0 {
		entries = append(entries, entry)
	}
	return entries
}

func verifyJournalctl(t *testing.T, path string) {
	t.Helper()

	verify := exec.Command("journalctl", "--verify", "--file", path)
	if output, err := verify.CombinedOutput(); err != nil {
		t.Fatalf("journalctl --verify failed: %v\n%s", err, output)
	}
}

func assertJSONByteArray(t *testing.T, row map[string]any, key string, want []byte) {
	t.Helper()

	got, ok := row[key]
	if !ok {
		t.Fatalf("field %s missing from row %v", key, row)
	}
	items, ok := got.([]any)
	if !ok {
		t.Fatalf("field %s = %T(%v), want JSON byte array", key, got, got)
	}
	if len(items) != len(want) {
		t.Fatalf("field %s byte count = %d, want %d; got=%v", key, len(items), len(want), got)
	}
	for i, item := range items {
		number, ok := item.(float64)
		if !ok {
			t.Fatalf("field %s byte %d = %T(%v), want number", key, i, item, item)
		}
		if number != float64(want[i]) {
			t.Fatalf("field %s byte %d = %v, want %d", key, i, number, want[i])
		}
	}
}

func assertExportField(t *testing.T, fields map[string][][]byte, key string, want []byte) {
	t.Helper()

	values, ok := fields[key]
	if !ok {
		t.Fatalf("export field %s missing from %v", key, fields)
	}
	if len(values) != 1 {
		t.Fatalf("export field %s value count = %d, want 1", key, len(values))
	}
	if !bytes.Equal(values[0], want) {
		t.Fatalf("export field %s = %x, want %x", key, values[0], want)
	}
}

func assertJSONField(t *testing.T, row map[string]any, key, want string) {
	t.Helper()

	got, ok := row[key]
	if !ok {
		t.Fatalf("field %s missing from row %v", key, row)
	}
	if fmt.Sprint(got) != want {
		t.Fatalf("field %s = %v, want %q", key, got, want)
	}
}

func runLibsystemdBinaryFieldReader(t *testing.T, path, field string, expected []byte, matches ...string) {
	t.Helper()

	cc, err := exec.LookPath("cc")
	if err != nil {
		t.Skip("cc is not installed")
	}
	if _, err := exec.LookPath("pkg-config"); err != nil {
		t.Skip("pkg-config is not installed")
	}
	pkg := exec.Command("pkg-config", "--cflags", "--libs", "libsystemd")
	pkgOutput, err := pkg.Output()
	if err != nil {
		t.Skipf("libsystemd development files are not available: %v", err)
	}

	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd() error = %v", err)
	}
	source := filepath.Clean(filepath.Join(wd, "..", "..", "tests", "conformance", "binary", "libsystemd_binary_field_reader.c"))
	if _, err := os.Stat(source); err != nil {
		t.Fatalf("libsystemd helper source missing: %v", err)
	}

	exe := filepath.Join(t.TempDir(), "libsystemd-binary-field-reader")
	args := []string{source, "-o", exe}
	args = append(args, strings.Fields(string(pkgOutput))...)
	build := exec.Command(cc, args...)
	if output, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build libsystemd helper failed: %v\n%s", err, output)
	}

	runArgs := []string{path, field, hex.EncodeToString(expected)}
	runArgs = append(runArgs, matches...)
	run := exec.Command(exe, runArgs...)
	if output, err := run.CombinedOutput(); err != nil {
		t.Fatalf("libsystemd binary field readback failed: %v\n%s", err, output)
	}
}
