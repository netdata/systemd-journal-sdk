package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"unicode/utf8"
)

func ExportEntry(entry *Entry) string {
	var buf bytes.Buffer

	writeExportMetadata(&buf, entry)
	written := writePreferredExportFields(&buf, entry)
	writeRemainingExportFields(&buf, entry, written)
	writeNonUTF8RawExportFields(&buf, entry)

	buf.WriteByte('\n')
	return buf.String()
}

func writeExportMetadata(buf *bytes.Buffer, entry *Entry) {
	if entry.Cursor != "" {
		writeExportField(buf, "__CURSOR", []byte(entry.Cursor))
	}

	if entry.Realtime != 0 {
		writeExportField(buf, "__REALTIME_TIMESTAMP", []byte(strconv.FormatUint(entry.Realtime, 10)))
	}

	if entry.Monotonic != 0 {
		writeExportField(buf, "__MONOTONIC_TIMESTAMP", []byte(strconv.FormatUint(entry.Monotonic, 10)))
	}

	if entry.Seqnum != 0 {
		writeExportField(buf, "__SEQNUM", []byte(strconv.FormatUint(entry.Seqnum, 10)))
	}

	if seqnumID, _, _, _, err := ParseCursor(entry.Cursor); err == nil && seqnumID != "" {
		writeExportField(buf, "__SEQNUM_ID", []byte(seqnumID))
	}

	writeExportField(buf, "_BOOT_ID", []byte(entry.BootID.String()))
}

func writePreferredExportFields(buf *bytes.Buffer, entry *Entry) map[string]struct{} {
	preferred := []string{"_MACHINE_ID", "_HOSTNAME", "PRIORITY", "_TRANSPORT"}
	written := map[string]struct{}{"_BOOT_ID": {}}
	for _, name := range preferred {
		for _, value := range entryValues(entry, name) {
			writeExportField(buf, name, value)
		}
		written[name] = struct{}{}
	}
	return written
}

func writeRemainingExportFields(buf *bytes.Buffer, entry *Entry, written map[string]struct{}) {
	var keys []string
	for _, k := range entryFieldNames(entry) {
		if _, ok := written[k]; ok {
			continue
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)

	for _, k := range keys {
		for _, value := range entryValues(entry, k) {
			writeExportField(buf, k, value)
		}
	}
}

func writeNonUTF8RawExportFields(buf *bytes.Buffer, entry *Entry) {
	for _, field := range entry.RawFields {
		if utf8.Valid(field.Name) {
			continue
		}
		writeExportRawField(buf, field.Name, field.Value)
	}
}

func JSONEntry(entry *Entry) (map[string]interface{}, error) {
	result := make(map[string]interface{})
	written := make(map[string]struct{})

	if entry.Cursor != "" {
		addJSONValue(result, "__CURSOR", []byte(entry.Cursor))
		written["__CURSOR"] = struct{}{}
	}
	if entry.Realtime != 0 {
		addJSONValue(result, "__REALTIME_TIMESTAMP", []byte(strconv.FormatUint(entry.Realtime, 10)))
		written["__REALTIME_TIMESTAMP"] = struct{}{}
	}
	if entry.Monotonic != 0 {
		addJSONValue(result, "__MONOTONIC_TIMESTAMP", []byte(strconv.FormatUint(entry.Monotonic, 10)))
		written["__MONOTONIC_TIMESTAMP"] = struct{}{}
	}
	if entry.Seqnum != 0 {
		addJSONValue(result, "__SEQNUM", []byte(strconv.FormatUint(entry.Seqnum, 10)))
		written["__SEQNUM"] = struct{}{}
	}
	if seqnumID, _, _, _, err := ParseCursor(entry.Cursor); err == nil && seqnumID != "" {
		addJSONValue(result, "__SEQNUM_ID", []byte(seqnumID))
		written["__SEQNUM_ID"] = struct{}{}
	}
	addJSONValue(result, "_BOOT_ID", []byte(entry.BootID.String()))
	written["_BOOT_ID"] = struct{}{}

	names := entryFieldNames(entry)
	sort.Strings(names)
	for _, name := range names {
		if _, ok := written[name]; ok {
			continue
		}
		for _, value := range entryValues(entry, name) {
			addJSONValue(result, name, value)
		}
	}

	return result, nil
}

func entryFieldNames(entry *Entry) []string {
	seen := make(map[string]struct{}, len(entry.Fields)+len(entry.FieldValues))
	for name := range entry.Fields {
		seen[name] = struct{}{}
	}
	for name := range entry.FieldValues {
		seen[name] = struct{}{}
	}
	names := make([]string, 0, len(seen))
	for name := range seen {
		names = append(names, name)
	}
	return names
}

func entryValues(entry *Entry, name string) [][]byte {
	if values := entry.FieldValues[name]; len(values) > 0 {
		return values
	}
	if value, ok := entry.Fields[name]; ok {
		return [][]byte{value}
	}
	return nil
}

func writeExportField(buf *bytes.Buffer, name string, value []byte) {
	writeExportRawField(buf, []byte(name), value)
}

func writeExportRawField(buf *bytes.Buffer, name []byte, value []byte) {
	text := make([]byte, 0, len(name)+1+len(value))
	text = append(text, name...)
	text = append(text, '=')
	text = append(text, value...)

	if journalBytesPrintable(text, false) {
		buf.Write(text)
		buf.WriteByte('\n')
		return
	}

	buf.Write(name)
	buf.WriteByte('\n')
	var size [8]byte
	binary.LittleEndian.PutUint64(size[:], uint64(len(value)))
	buf.Write(size[:])
	buf.Write(value)
	buf.WriteByte('\n')
}

func addJSONValue(result map[string]interface{}, name string, value []byte) {
	encoded := jsonFieldValue(value)
	if existing, ok := result[name]; ok {
		if values, ok := existing.([]interface{}); ok {
			result[name] = append(values, encoded)
			return
		}
		result[name] = []interface{}{existing, encoded}
		return
	}
	result[name] = encoded
}

func jsonFieldValue(value []byte) interface{} {
	if journalBytesPrintable(value, true) {
		return string(value)
	}

	values := make([]int, len(value))
	for i, b := range value {
		values[i] = int(b)
	}
	return values
}

func journalBytesPrintable(value []byte, allowNewline bool) bool {
	for len(value) > 0 {
		r, size := utf8.DecodeRune(value)
		if r == utf8.RuneError && size == 1 {
			return false
		}
		if r < ' ' {
			if r == '\t' || (allowNewline && r == '\n') {
				value = value[size:]
				continue
			}
			return false
		}
		if r >= 0x7f && r <= 0x9f {
			return false
		}
		value = value[size:]
	}
	return true
}

func ParseMatchString(s string) ([]byte, error) {
	field, err := parseMatchField(s)
	if err != nil {
		return nil, err
	}
	if err := validateMatchFieldName(field); err != nil {
		return nil, err
	}
	return []byte(s), nil
}

func parseMatchField(s string) (string, error) {
	switch {
	case s == "":
		return "", errors.New("empty match string")
	case s == "=":
		return "", errors.New("invalid match: missing field name")
	case strings.HasPrefix(s, "="):
		return "", errors.New("invalid match: field name cannot start with =")
	}
	eq := strings.IndexByte(s, '=')
	if eq < 0 {
		return "", errors.New("invalid match: missing '=' separator")
	}
	return s[:eq], nil
}

func validateMatchFieldName(field string) error {
	if field == "" {
		return errors.New("invalid match: empty field name")
	}

	if len(field) > 64 {
		return errors.New("invalid match: field name too long")
	}

	if field[0] >= '0' && field[0] <= '9' {
		return fmt.Errorf("invalid field name %q", field)
	}
	for _, c := range field {
		if c == '_' || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			continue
		}
		return fmt.Errorf("invalid field name %q", field)
	}

	return nil
}

type parsedCursorLocation struct {
	seqnumID     string
	seqnum       uint64
	seqnumSet    bool
	bootID       string
	monotonic    uint64
	monotonicSet bool
	realtime     uint64
	realtimeSet  bool
	xorHash      uint64
	xorHashSet   bool
}

func ParseCursor(cursor string) (seqnumID string, bootID string, realtime uint64, seqnum uint64, err error) {
	location, err := parseCursorLocation(cursor, true)
	if err != nil {
		return "", "", 0, 0, err
	}
	return location.seqnumID, location.bootID, location.realtime, location.seqnum, nil
}

func parseCursorLocation(cursor string, requireSeekComponent bool) (parsedCursorLocation, error) {
	parts := strings.Split(cursor, ";")
	values := make(map[string]string, len(parts))
	for _, part := range parts {
		key, value, ok := strings.Cut(part, "=")
		if !ok || key == "" || value == "" {
			return parsedCursorLocation{}, errors.New("invalid cursor format")
		}
		values[key] = value
	}

	var location parsedCursorLocation
	location.seqnumID = normalizeCursorID(values["s"])

	if values["j"] != "" || values["c"] != "" || values["n"] != "" {
		location.bootID = normalizeCursorID(values["j"])
		if location.seqnumID == "" || location.bootID == "" || values["c"] == "" || values["n"] == "" {
			return parsedCursorLocation{}, errors.New("invalid cursor format")
		}
		realtime, err := strconv.ParseUint(values["c"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad realtime")
		}
		seqnum, err := strconv.ParseUint(values["n"], 10, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad seqnum")
		}
		location.realtime = realtime
		location.realtimeSet = true
		location.seqnum = seqnum
		location.seqnumSet = true
		return location, nil
	}

	location.bootID = normalizeCursorID(values["b"])
	if values["t"] != "" {
		realtime, err := strconv.ParseUint(values["t"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad realtime")
		}
		location.realtime = realtime
		location.realtimeSet = true
	}
	if values["i"] != "" {
		seqnum, err := strconv.ParseUint(values["i"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad seqnum")
		}
		location.seqnum = seqnum
		location.seqnumSet = true
	}
	if values["m"] != "" {
		monotonic, err := strconv.ParseUint(values["m"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad monotonic")
		}
		location.monotonic = monotonic
		location.monotonicSet = true
	}
	if values["x"] != "" {
		xorHash, err := strconv.ParseUint(values["x"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad xor hash")
		}
		location.xorHash = xorHash
		location.xorHashSet = true
	}

	hasSeqnumCursor := location.seqnumID != "" && location.seqnumSet
	hasMonotonicCursor := location.bootID != "" && location.monotonicSet
	hasRealtimeCursor := location.realtimeSet
	if requireSeekComponent && !hasSeqnumCursor && !hasMonotonicCursor && !hasRealtimeCursor {
		return parsedCursorLocation{}, errors.New("invalid cursor format")
	}
	if !requireSeekComponent &&
		location.seqnumID == "" &&
		!location.seqnumSet &&
		location.bootID == "" &&
		!location.monotonicSet &&
		!location.realtimeSet &&
		!location.xorHashSet {
		return parsedCursorLocation{}, errors.New("invalid cursor format")
	}

	return location, nil
}

func normalizeCursorID(value string) string {
	return strings.ToLower(strings.ReplaceAll(value, "-", ""))
}

func cursorLocationMatches(got, want parsedCursorLocation) bool {
	matched := false
	if want.seqnumID != "" {
		if got.seqnumID != want.seqnumID {
			return false
		}
		matched = true
	}
	if want.seqnumSet {
		if !got.seqnumSet || got.seqnum != want.seqnum {
			return false
		}
		matched = true
	}
	if want.bootID != "" {
		if got.bootID != want.bootID {
			return false
		}
		matched = true
	}
	if want.monotonicSet {
		if !got.monotonicSet || got.monotonic != want.monotonic {
			return false
		}
		matched = true
	}
	if want.realtimeSet {
		if !got.realtimeSet || got.realtime != want.realtime {
			return false
		}
		matched = true
	}
	if want.xorHashSet {
		if !got.xorHashSet || got.xorHash != want.xorHash {
			return false
		}
		matched = true
	}
	return matched
}

func cursorLocationAtOrAfter(got, want parsedCursorLocation) bool {
	if want.seqnumID != "" && want.seqnumSet && got.seqnumID == want.seqnumID {
		if got.seqnum != want.seqnum {
			return got.seqnum > want.seqnum
		}
	}
	if want.bootID != "" && want.monotonicSet && got.bootID == want.bootID {
		if got.monotonic != want.monotonic {
			return got.monotonic > want.monotonic
		}
	}
	if want.realtimeSet {
		if got.realtime != want.realtime {
			return got.realtime > want.realtime
		}
	}
	if want.xorHashSet {
		if got.xorHash != want.xorHash {
			return got.xorHash > want.xorHash
		}
	}
	return true
}
