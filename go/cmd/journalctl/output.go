package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

type outputOptions struct {
	mode            string
	outputFields    []string
	outputFieldsSet bool
	utc             bool
	noHostname      bool
	fullWidth       bool
	showAll         bool
	truncateNewline bool
	merge           bool
}

const (
	printCharThreshold = 300
	jsonThreshold      = 4096
	defaultColumns     = 80
)

func newOutputOptions(flags *cliFlags, outputFieldsSet bool, fullWidth bool) outputOptions {
	return outputOptions{
		mode:            *flags.output,
		outputFields:    parseOutputFields(*flags.outputFieldsFlag),
		outputFieldsSet: outputFieldsSet,
		utc:             *flags.utcFlag,
		noHostname:      *flags.noHostnameFlag,
		fullWidth:       fullWidth,
		showAll:         *flags.allFlag,
		truncateNewline: *flags.truncateNewlineFlag,
		merge:           *flags.mergeFlag,
	}
}

func parseOutputFields(value string) []string {
	var fields []string
	for _, item := range strings.Split(value, ",") {
		item = strings.TrimSpace(item)
		if item != "" {
			fields = append(fields, item)
		}
	}
	return fields
}

type outputRenderer struct {
	options       outputOptions
	previousDelta *deltaState
}

type deltaState struct {
	realtime  uint64
	monotonic uint64
	bootID    journal.UUID
}

func newOutputRenderer(options outputOptions) *outputRenderer {
	return &outputRenderer{options: options}
}

func (r *outputRenderer) render(entry *journal.Entry) (string, error) {
	switch r.options.mode {
	case "export":
		return r.renderExport(entry), nil
	case "json":
		return r.renderJSON(entry, jsonFrameLine)
	case "json-pretty":
		return r.renderJSON(entry, jsonFramePretty)
	case "json-sse":
		return r.renderJSON(entry, jsonFrameSSE)
	case "json-seq":
		return r.renderJSON(entry, jsonFrameSeq)
	case "verbose":
		return r.renderVerbose(entry)
	case "cat":
		return r.renderCat(entry), nil
	case "with-unit":
		return r.renderWithUnit(entry)
	case "short-full":
		return r.renderShort(entry, timestampShortFull)
	case "short-iso":
		return r.renderShort(entry, timestampShortISO)
	case "short-iso-precise":
		return r.renderShort(entry, timestampShortISOPrecise)
	case "short-precise":
		return r.renderShort(entry, timestampShortPrecise)
	case "short-monotonic":
		return r.renderShort(entry, timestampShortMonotonic)
	case "short-delta":
		return r.renderShortDelta(entry)
	case "short-unix":
		return r.renderShort(entry, timestampShortUnix)
	case "short":
		return r.renderShort(entry, timestampShort)
	default:
		return "", fmt.Errorf("Unknown output format %q.", r.options.mode)
	}
}

type timestampMode int

const (
	timestampShort timestampMode = iota
	timestampShortFull
	timestampShortISO
	timestampShortISOPrecise
	timestampShortPrecise
	timestampShortMonotonic
	timestampShortUnix
	timestampVerbose
)

func (r *outputRenderer) renderShort(entry *journal.Entry, mode timestampMode) (string, error) {
	timestamp, err := r.formatTimestamp(entry, mode)
	if err != nil {
		return "", err
	}
	prefix := fmt.Sprintf("%s %s: ", timestamp, entryLabel(entry, r.options))
	return prefix + displayMessage(entry, r.options, len(prefix)) + "\n", nil
}

func (r *outputRenderer) renderWithUnit(entry *journal.Entry) (string, error) {
	timestamp, err := r.formatTimestamp(entry, timestampShortFull)
	if err != nil {
		return "", err
	}
	label := unitLabel(entry)
	if label == "" {
		label = baseEntryLabel(entry)
	}
	label = formatEntryLabel(entry, label, r.options)
	prefix := fmt.Sprintf("%s %s: ", timestamp, label)
	return prefix + displayMessage(entry, r.options, len(prefix)) + "\n", nil
}

func (r *outputRenderer) renderShortDelta(entry *journal.Entry) (string, error) {
	currentRealtime := displayRealtimeUsec(entry)
	currentMonotonic := displayMonotonicUsec(entry)
	monotonic := formatMonotonic(currentMonotonic)
	delta := "                "
	if r.previousDelta != nil {
		marker := " "
		if r.previousDelta.bootID != entry.BootID {
			marker = "*"
		}
		var diff uint64
		if marker == "*" {
			diff = absDiff(currentRealtime, r.previousDelta.realtime)
		} else {
			diff = absDiff(currentMonotonic, r.previousDelta.monotonic)
		}
		delta = fmt.Sprintf(" <%s%s>", formatMonotonic(diff), marker)
	}
	r.previousDelta = &deltaState{realtime: currentRealtime, monotonic: currentMonotonic, bootID: entry.BootID}
	prefix := fmt.Sprintf("[%s%s] %s: ", monotonic, delta, entryLabel(entry, r.options))
	return prefix + displayMessage(entry, r.options, len(prefix)) + "\n", nil
}

func absDiff(a, b uint64) uint64 {
	if a > b {
		return a - b
	}
	return b - a
}

func (r *outputRenderer) renderCat(entry *journal.Entry) string {
	var b strings.Builder
	if r.options.outputFieldsSet {
		for _, name := range r.options.outputFields {
			for _, value := range entryValues(entry, name) {
				b.WriteString(displayValue(value, r.options.truncateNewline))
				b.WriteByte('\n')
			}
		}
		return b.String()
	}
	for _, value := range entryValues(entry, "MESSAGE") {
		b.WriteString(displayValue(value, r.options.truncateNewline))
		b.WriteByte('\n')
	}
	return b.String()
}

func (r *outputRenderer) renderVerbose(entry *journal.Entry) (string, error) {
	timestamp, err := r.formatTimestamp(entry, timestampVerbose)
	if err != nil {
		return "", err
	}
	var b strings.Builder
	fmt.Fprintf(&b, "%s [%s]\n", timestamp, entry.Cursor)
	for _, field := range verboseFields(entry, r.options) {
		fmt.Fprintf(&b, "    %s=%s\n", field.name, displayVerboseValue(field.name, field.value, r.options))
	}
	return b.String(), nil
}

func (r *outputRenderer) renderExport(entry *journal.Entry) string {
	var buf bytes.Buffer
	for _, field := range metadataFields(entry) {
		writeExportField(&buf, []byte(field.name), field.value)
	}
	for _, field := range selectedOutputFields(entry, r.options) {
		writeExportField(&buf, []byte(field.name), field.value)
	}
	buf.WriteByte('\n')
	return buf.String()
}

type jsonFrame int

const (
	jsonFrameLine jsonFrame = iota
	jsonFramePretty
	jsonFrameSSE
	jsonFrameSeq
)

func (r *outputRenderer) renderJSON(entry *journal.Entry, frame jsonFrame) (string, error) {
	object := jsonObject(entry, r.options)
	var encoded []byte
	var err error
	if frame == jsonFramePretty {
		encoded, err = marshalSystemdPrettyJSON(object)
	} else {
		encoded, err = json.Marshal(object)
	}
	if err != nil {
		return "", err
	}
	switch frame {
	case jsonFrameSSE:
		return "data: " + string(encoded) + "\n\n", nil
	case jsonFrameSeq:
		return string([]byte{0x1e}) + string(encoded) + "\n", nil
	default:
		return string(encoded) + "\n", nil
	}
}

func (r *outputRenderer) formatTimestamp(entry *journal.Entry, mode timestampMode) (string, error) {
	if mode == timestampShortMonotonic {
		return "[" + formatMonotonic(displayMonotonicUsec(entry)) + "]", nil
	}
	realtime := displayRealtimeUsec(entry)
	if mode == timestampShortUnix {
		return fmt.Sprintf("%d.%06d", realtime/1_000_000, realtime%1_000_000), nil
	}
	t := time.UnixMicro(int64(realtime))
	if r.options.utc {
		t = t.UTC()
	} else {
		t = t.Local()
	}
	switch mode {
	case timestampShort:
		return t.Format("Jan 02 15:04:05"), nil
	case timestampShortFull:
		return t.Format("Mon 2006-01-02 15:04:05 MST"), nil
	case timestampShortISO:
		return t.Format("2006-01-02T15:04:05-07:00"), nil
	case timestampShortISOPrecise:
		return t.Format("2006-01-02T15:04:05.000000-07:00"), nil
	case timestampShortPrecise:
		return t.Format("Jan 02 15:04:05.000000"), nil
	case timestampVerbose:
		return t.Format("Mon 2006-01-02 15:04:05.000000 MST"), nil
	default:
		return "", fmt.Errorf("unknown timestamp mode")
	}
}

func outputSkipsMissingMessage(mode string) bool {
	switch mode {
	case "short", "short-full", "short-iso", "short-iso-precise", "short-precise",
		"short-monotonic", "short-delta", "short-unix", "with-unit":
		return true
	default:
		return false
	}
}

func displayRealtimeUsec(entry *journal.Entry) uint64 {
	if realtime, ok := sourceRealtimeUsec(entry); ok {
		return realtime
	}
	return entry.Realtime
}

func displayMonotonicUsec(entry *journal.Entry) uint64 {
	if realtime, ok := sourceRealtimeUsec(entry); ok {
		return mapClockUsec(entry.Monotonic, entry.Realtime, realtime)
	}
	return entry.Monotonic
}

func sourceRealtimeUsec(entry *journal.Entry) (uint64, bool) {
	values := entryValues(entry, "_SOURCE_REALTIME_TIMESTAMP")
	if len(values) == 0 {
		return 0, false
	}
	value, err := strconv.ParseUint(string(values[0]), 10, 64)
	if err != nil || value == 0 {
		return 0, false
	}
	return value, true
}

func mapClockUsec(value, from, to uint64) uint64 {
	if to >= from {
		delta := to - from
		if value > ^uint64(0)-delta {
			return ^uint64(0)
		}
		return value + delta
	}
	delta := from - to
	if value < delta {
		return 0
	}
	return value - delta
}

func formatMonotonic(usec uint64) string {
	return fmt.Sprintf("%5d.%06d", usec/1_000_000, usec%1_000_000)
}

func entryLabel(entry *journal.Entry, options outputOptions) string {
	return formatEntryLabel(entry, baseEntryLabel(entry), options)
}

func baseEntryLabel(entry *journal.Entry) string {
	for _, name := range []string{"SYSLOG_IDENTIFIER", "_COMM", "_EXE"} {
		if value := firstString(entry, name); value != "" {
			return value
		}
	}
	return "unknown"
}

func formatEntryLabel(entry *journal.Entry, label string, options outputOptions) string {
	var parts []string
	if !options.noHostname {
		if hostname := firstString(entry, "_HOSTNAME"); hostname != "" {
			parts = append(parts, hostname)
		}
	}
	if label == "" {
		label = "unknown"
	}
	if pid := firstString(entry, "_PID"); pid != "" {
		label += "[" + pid + "]"
	} else if pid := firstString(entry, "SYSLOG_PID"); pid != "" {
		label += "[" + pid + "]"
	}
	parts = append(parts, label)
	return strings.Join(parts, " ")
}

func unitLabel(entry *journal.Entry) string {
	for _, name := range []string{"_SYSTEMD_UNIT", "_SYSTEMD_USER_UNIT", "UNIT", "USER_UNIT", "OBJECT_SYSTEMD_UNIT", "OBJECT_SYSTEMD_USER_UNIT"} {
		if value := firstString(entry, name); value != "" {
			return value
		}
	}
	return ""
}

func displayMessage(entry *journal.Entry, options outputOptions, prefixColumns int) string {
	values := entryValues(entry, "MESSAGE")
	if len(values) == 0 {
		return ""
	}
	value := values[0]
	if options.truncateNewline {
		value = truncateAtNewline(value)
	}
	if !options.showAll && !journalTextPrintable(value) {
		return blobData(len(value))
	}
	out := indentContinuationLines(cStringBytes(value), prefixColumns)
	if !options.showAll && !options.fullWidth {
		out = ellipsizeLine(out, prefixColumns)
	}
	return string(out)
}

func indentContinuationLines(value []byte, prefixColumns int) []byte {
	if !bytes.Contains(value, []byte{'\n'}) {
		return value
	}
	indent := bytes.Repeat([]byte{' '}, prefixColumns)
	out := make([]byte, 0, len(value)+bytes.Count(value, []byte{'\n'})*prefixColumns)
	for idx, b := range value {
		out = append(out, b)
		if b == '\n' && idx+1 < len(value) {
			out = append(out, indent...)
		}
	}
	return out
}

func firstString(entry *journal.Entry, name string) string {
	values := entryValues(entry, name)
	if len(values) == 0 {
		return ""
	}
	return string(values[0])
}

func displayValue(value []byte, truncateNewline bool) string {
	if truncateNewline {
		value = truncateAtNewline(value)
	}
	return string(value)
}

func displayVerboseValue(name string, value []byte, options outputOptions) string {
	if options.showAll {
		return string(cStringBytes(value))
	}
	if !journalTextPrintable(value) || (!options.fullWidth && len(name)+1+len(value) >= printCharThreshold) {
		return blobData(len(value))
	}
	return string(value)
}

func truncateAtNewline(value []byte) []byte {
	if idx := bytes.IndexByte(value, '\n'); idx >= 0 {
		return value[:idx]
	}
	return value
}

func cStringBytes(value []byte) []byte {
	if idx := bytes.IndexByte(value, 0); idx >= 0 {
		return value[:idx]
	}
	return value
}

func ellipsizeLine(value []byte, prefixColumns int) []byte {
	limit := defaultColumns - prefixColumns - 1
	if limit < 0 {
		limit = 0
	}
	if len(value) <= limit {
		return value
	}
	for limit > 0 && !utf8.RuneStart(value[limit]) {
		limit--
	}
	out := make([]byte, 0, limit+len("…"))
	out = append(out, value[:limit]...)
	out = append(out, "…"...)
	return out
}

func journalTextPrintable(value []byte) bool {
	text := string(value)
	if !utf8.ValidString(text) {
		return false
	}
	for _, ch := range text {
		cp := uint32(ch)
		if (cp < 0x20 && ch != '\t' && ch != '\n') || (cp >= 0x7f && cp <= 0x9f) {
			return false
		}
	}
	return true
}

func blobData(size int) string {
	return fmt.Sprintf("[%s blob data]", formatJournalBytes(uint64(size)))
}

type outputField struct {
	name  string
	value []byte
}

func metadataFields(entry *journal.Entry) []outputField {
	fields := []outputField{}
	if entry.Cursor != "" {
		fields = append(fields, outputField{"__CURSOR", []byte(entry.Cursor)})
	}
	fields = append(fields,
		outputField{"__REALTIME_TIMESTAMP", []byte(fmt.Sprintf("%d", entry.Realtime))},
		outputField{"__MONOTONIC_TIMESTAMP", []byte(fmt.Sprintf("%d", entry.Monotonic))},
		outputField{"__SEQNUM", []byte(fmt.Sprintf("%d", entry.Seqnum))},
	)
	if seqnumID, _, _, _, err := journal.ParseCursor(entry.Cursor); err == nil && seqnumID != "" {
		fields = append(fields, outputField{"__SEQNUM_ID", []byte(seqnumID)})
	}
	fields = append(fields, outputField{"_BOOT_ID", []byte(entry.BootID.String())})
	return fields
}

func verboseFields(entry *journal.Entry, options outputOptions) []outputField {
	if options.outputFieldsSet {
		return selectedNamedFields(entry, options.outputFields)
	}
	fields := make([]outputField, 0, len(entry.RawFields))
	for _, field := range entry.RawFields {
		fields = append(fields, outputField{name: string(field.Name), value: field.Value})
	}
	return fields
}

func selectedOutputFields(entry *journal.Entry, options outputOptions) []outputField {
	if options.outputFieldsSet {
		fields := selectedNamedFields(entry, options.outputFields)
		filtered := fields[:0]
		for _, field := range fields {
			if !isMetadataField(field.name) {
				filtered = append(filtered, field)
			}
		}
		return filtered
	}
	fields := make([]outputField, 0, len(entry.RawFields))
	for _, field := range entry.RawFields {
		if string(field.Name) == "_BOOT_ID" {
			continue
		}
		fields = append(fields, outputField{name: string(field.Name), value: field.Value})
	}
	return fields
}

func isMetadataField(name string) bool {
	switch name {
	case "__CURSOR", "__REALTIME_TIMESTAMP", "__MONOTONIC_TIMESTAMP", "__SEQNUM", "__SEQNUM_ID", "_BOOT_ID":
		return true
	default:
		return false
	}
}

func selectedNamedFields(entry *journal.Entry, names []string) []outputField {
	var fields []outputField
	for _, name := range names {
		for _, value := range entryValues(entry, name) {
			fields = append(fields, outputField{name: name, value: value})
		}
	}
	return fields
}

func jsonObject(entry *journal.Entry, options outputOptions) map[string]any {
	object := make(map[string]any)
	for _, field := range metadataFields(entry) {
		addJSONValue(object, field.name, field.value, options.showAll)
	}
	for _, field := range selectedOutputFields(entry, options) {
		addJSONValue(object, field.name, field.value, options.showAll)
	}
	return object
}

func addJSONValue(object map[string]any, name string, value []byte, showAll bool) {
	encoded := jsonValueForBytes(name, value, showAll)
	if existing, ok := object[name]; ok {
		if values, ok := existing.([]any); ok {
			object[name] = append(values, encoded)
			return
		}
		object[name] = []any{existing, encoded}
		return
	}
	object[name] = encoded
}

func jsonValueForBytes(name string, value []byte, showAll bool) any {
	if !showAll && len(name)+1+len(value) >= jsonThreshold {
		return nil
	}
	if utf8.Valid(value) {
		text := string(value)
		printable := true
		for _, ch := range text {
			cp := uint32(ch)
			if (cp < 0x20 && ch != '\t' && ch != '\n') || (cp >= 0x7f && cp <= 0x9f) {
				printable = false
				break
			}
		}
		if printable {
			return text
		}
	}
	values := make([]any, 0, len(value))
	for _, b := range value {
		values = append(values, int(b))
	}
	return values
}

func marshalSystemdPrettyJSON(object map[string]any) ([]byte, error) {
	var buf bytes.Buffer
	if err := writeSystemdPrettyJSONValue(&buf, object, 0); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func writeSystemdPrettyJSONValue(buf *bytes.Buffer, value any, depth int) error {
	switch v := value.(type) {
	case map[string]any:
		buf.WriteByte('{')
		keys := make([]string, 0, len(v))
		for key := range v {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		if len(keys) > 0 {
			buf.WriteByte('\n')
		}
		for i, key := range keys {
			writeJSONTabs(buf, depth+1)
			keyBytes, err := json.Marshal(key)
			if err != nil {
				return err
			}
			buf.Write(keyBytes)
			buf.WriteString(" : ")
			if err := writeSystemdPrettyJSONValue(buf, v[key], depth+1); err != nil {
				return err
			}
			if i+1 != len(keys) {
				buf.WriteByte(',')
			}
			buf.WriteByte('\n')
		}
		if len(keys) > 0 {
			writeJSONTabs(buf, depth)
		}
		buf.WriteByte('}')
	case []any:
		buf.WriteByte('[')
		if len(v) > 0 {
			buf.WriteByte('\n')
		}
		for i, item := range v {
			writeJSONTabs(buf, depth+1)
			if err := writeSystemdPrettyJSONValue(buf, item, depth+1); err != nil {
				return err
			}
			if i+1 != len(v) {
				buf.WriteByte(',')
			}
			buf.WriteByte('\n')
		}
		if len(v) > 0 {
			writeJSONTabs(buf, depth)
		}
		buf.WriteByte(']')
	default:
		raw, err := json.Marshal(value)
		if err != nil {
			return err
		}
		buf.Write(raw)
	}
	return nil
}

func writeJSONTabs(buf *bytes.Buffer, depth int) {
	for i := 0; i < depth; i++ {
		buf.WriteByte('\t')
	}
}

func writeExportField(buf *bytes.Buffer, name []byte, value []byte) {
	text := make([]byte, 0, len(name)+1+len(value))
	text = append(text, name...)
	text = append(text, '=')
	text = append(text, value...)
	if journalBytesPrintable(text) {
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

func journalBytesPrintable(value []byte) bool {
	if !utf8.Valid(value) {
		return false
	}
	for _, ch := range string(value) {
		cp := uint32(ch)
		if (cp < 0x20 && ch != '\t') || (cp >= 0x7f && cp <= 0x9f) {
			return false
		}
	}
	return true
}
