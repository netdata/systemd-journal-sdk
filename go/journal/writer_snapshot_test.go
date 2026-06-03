package journal

import (
	"bytes"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

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

func fieldWithTotalPayloadLen(t *testing.T, name string, payloadLen int) Field {
	t.Helper()

	prefixLen := len(name) + 1
	if payloadLen < prefixLen {
		t.Fatalf("payload length %d is shorter than %q prefix", payloadLen, name+"=")
	}
	return Field{Name: name, Value: bytes.Repeat([]byte("A"), payloadLen-prefixLen)}
}

func snapshotHasDataObjectFlag(snapshot journalSnapshot, flag uint8) bool {
	for _, data := range snapshot.dataByPayload {
		if data.header.object.flag&flag != 0 {
			return true
		}
	}
	return false
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
	scanJournalSnapshotObjects(t, content, &snapshot, snapshotEndOffset(t, content, header))
	return snapshot
}

func snapshotEndOffset(t *testing.T, content []byte, header journalHeader) uint64 {
	t.Helper()
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
	return endOffset
}

func scanJournalSnapshotObjects(t *testing.T, content []byte, snapshot *journalSnapshot, endOffset uint64) {
	t.Helper()
	for offset := snapshot.header.headerSize; offset < endOffset; {
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

		recordJournalSnapshotObject(t, content, snapshot, offset, oh)
		offset = align8(offset + oh.size)
	}
}

func recordJournalSnapshotObject(t *testing.T, content []byte, snapshot *journalSnapshot, offset uint64, oh objectHeader) {
	t.Helper()
	switch oh.typ {
	case objectTypeData:
		recordSnapshotDataObject(t, content, snapshot, offset, oh)
	case objectTypeField:
		recordSnapshotFieldObject(t, content, snapshot, offset, oh)
	case objectTypeEntry:
		entry := parseEntryObject(t, offset, content[offset:offset+oh.size], snapshot.header.isCompact())
		snapshot.entries = append(snapshot.entries, entry)
	case objectTypeDataHashTable, objectTypeFieldHashTable, objectTypeEntryArray:
	default:
		t.Fatalf("unexpected object type %d at offset %d", oh.typ, offset)
	}
}

func recordSnapshotDataObject(t *testing.T, content []byte, snapshot *journalSnapshot, offset uint64, oh objectHeader) {
	t.Helper()
	header, err := parseDataHeader(content[offset : offset+dataObjectHeaderSize])
	if err != nil {
		t.Fatalf("parseDataHeader(%d) error = %v", offset, err)
	}
	payloadOffset := uint64(dataObjectHeaderSize)
	if snapshot.header.isCompact() {
		payloadOffset = compactDataObjectHeaderSize
	}
	payload := append([]byte(nil), content[offset+payloadOffset:offset+oh.size]...)
	snapshot.dataByPayload[string(payload)] = dataSnapshot{offset: offset, header: header, payload: payload}
}

func recordSnapshotFieldObject(t *testing.T, content []byte, snapshot *journalSnapshot, offset uint64, oh objectHeader) {
	t.Helper()
	header, err := parseFieldHeader(content[offset : offset+fieldObjectHeaderSize])
	if err != nil {
		t.Fatalf("parseFieldHeader(%d) error = %v", offset, err)
	}
	payload := append([]byte(nil), content[offset+fieldObjectHeaderSize:offset+oh.size]...)
	snapshot.fieldByPayload[string(payload)] = fieldSnapshot{offset: offset, header: header, payload: payload}
}

func parseEntryObject(t *testing.T, offset uint64, content []byte, compact bool) entrySnapshot {
	t.Helper()

	oh, err := parseObjectHeader(content[:objectHeaderSize])
	if err != nil {
		t.Fatalf("parseObjectHeader(entry) error = %v", err)
	}
	itemSize := uint64(regularEntryItemSize)
	if compact {
		itemSize = compactEntryItemSize
	}
	if oh.size < entryObjectHeaderSize || (oh.size-entryObjectHeaderSize)%itemSize != 0 {
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
	for i := entryObjectHeaderSize; i < len(content); i += int(itemSize) {
		if compact {
			entry.itemOffsets = append(entry.itemOffsets, uint64(binary.LittleEndian.Uint32(content[i:i+compactEntryItemSize])))
		} else {
			entry.itemOffsets = append(entry.itemOffsets, le64(content[i:i+8]))
		}
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
	requireJournalctl(t)

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
	requireJournalctl(t)

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
	requireJournalctl(t)

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
	requireJournalctl(t)

	verify := exec.Command("journalctl", "--verify", "--file", path)
	if output, err := verify.CombinedOutput(); err != nil {
		t.Fatalf("journalctl --verify failed: %v\n%s", err, output)
	}
}

func verifyJournalctlFails(t *testing.T, path string, want string) {
	t.Helper()
	requireJournalctl(t)

	verify := exec.Command("journalctl", "--verify", "--file", path)
	output, err := verify.CombinedOutput()
	if err == nil {
		t.Fatalf("journalctl --verify unexpectedly passed for %s\n%s", path, output)
	}
	if want != "" && !strings.Contains(strings.ToLower(string(output)), strings.ToLower(want)) {
		t.Fatalf("journalctl --verify output = %q, want substring %q", output, want)
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
