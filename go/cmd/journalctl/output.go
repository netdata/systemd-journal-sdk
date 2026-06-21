package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
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
	truncateNewline bool
}

func newOutputOptions(flags *cliFlags, outputFieldsSet bool) outputOptions {
	return outputOptions{
		mode:            *flags.output,
		outputFields:    parseOutputFields(*flags.outputFieldsFlag),
		outputFieldsSet: outputFieldsSet,
		utc:             *flags.utcFlag,
		truncateNewline: *flags.truncateNewlineFlag,
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
	realtime uint64
	bootID   journal.UUID
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
	default:
		return r.renderShort(entry, timestampShort)
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
	return fmt.Sprintf("%s %s: %s\n", timestamp, entryLabel(entry), displayMessage(entry, r.options.truncateNewline)), nil
}

func (r *outputRenderer) renderWithUnit(entry *journal.Entry) (string, error) {
	timestamp, err := r.formatTimestamp(entry, timestampShortFull)
	if err != nil {
		return "", err
	}
	label := unitLabel(entry)
	if label == "" {
		label = entryLabel(entry)
	}
	return fmt.Sprintf("%s %s: %s\n", timestamp, label, displayMessage(entry, r.options.truncateNewline)), nil
}

func (r *outputRenderer) renderShortDelta(entry *journal.Entry) (string, error) {
	monotonic := formatMonotonic(entry.Monotonic)
	delta := "                "
	if r.previousDelta != nil {
		diff := entry.Realtime - r.previousDelta.realtime
		if r.previousDelta.realtime > entry.Realtime {
			diff = r.previousDelta.realtime - entry.Realtime
		}
		marker := " "
		if r.previousDelta.bootID != entry.BootID {
			marker = "*"
		}
		delta = fmt.Sprintf(" <%s%s>", formatMonotonic(diff), marker)
	}
	r.previousDelta = &deltaState{realtime: entry.Realtime, bootID: entry.BootID}
	return fmt.Sprintf("[%s%s] %s: %s\n", monotonic, delta, entryLabel(entry), displayMessage(entry, r.options.truncateNewline)), nil
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
		fmt.Fprintf(&b, "    %s=%s\n", field.name, displayValue(field.value, false))
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
		encoded, err = json.MarshalIndent(object, "", "  ")
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
		return "[" + formatMonotonic(entry.Monotonic) + "]", nil
	}
	if mode == timestampShortUnix {
		return fmt.Sprintf("%d.%06d", entry.Realtime/1_000_000, entry.Realtime%1_000_000), nil
	}
	t := time.UnixMicro(int64(entry.Realtime))
	if r.options.utc {
		t = t.UTC()
	} else {
		t = t.Local()
	}
	switch mode {
	case timestampShort:
		return t.Format("Jan _2 15:04:05"), nil
	case timestampShortFull:
		return t.Format("Mon 2006-01-02 15:04:05 MST"), nil
	case timestampShortISO:
		return t.Format("2006-01-02T15:04:05Z07:00"), nil
	case timestampShortISOPrecise:
		return t.Format("2006-01-02T15:04:05.000000Z07:00"), nil
	case timestampShortPrecise:
		return t.Format("Jan _2 15:04:05.000000"), nil
	case timestampVerbose:
		return t.Format("Mon 2006-01-02 15:04:05.000000 MST"), nil
	default:
		return "", fmt.Errorf("unknown timestamp mode")
	}
}

func formatMonotonic(usec uint64) string {
	return fmt.Sprintf("%5d.%06d", usec/1_000_000, usec%1_000_000)
}

func entryLabel(entry *journal.Entry) string {
	for _, name := range []string{"SYSLOG_IDENTIFIER", "_COMM", "_EXE"} {
		if value := firstString(entry, name); value != "" {
			return value
		}
	}
	return "-"
}

func unitLabel(entry *journal.Entry) string {
	for _, name := range []string{"_SYSTEMD_UNIT", "_SYSTEMD_USER_UNIT", "UNIT", "USER_UNIT", "OBJECT_SYSTEMD_UNIT", "OBJECT_SYSTEMD_USER_UNIT"} {
		if value := firstString(entry, name); value != "" {
			return value
		}
	}
	return ""
}

func displayMessage(entry *journal.Entry, truncateNewline bool) string {
	values := entryValues(entry, "MESSAGE")
	if len(values) == 0 {
		return ""
	}
	return displayValue(values[0], truncateNewline)
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
		if idx := bytes.IndexByte(value, '\n'); idx >= 0 {
			value = value[:idx]
		}
	}
	return string(value)
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
	)
	if entry.Seqnum != 0 {
		fields = append(fields, outputField{"__SEQNUM", []byte(fmt.Sprintf("%d", entry.Seqnum))})
	}
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
		return selectedNamedFields(entry, options.outputFields)
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
		addJSONValue(object, field.name, field.value)
	}
	for _, field := range selectedOutputFields(entry, options) {
		addJSONValue(object, field.name, field.value)
	}
	return object
}

func addJSONValue(object map[string]any, name string, value []byte) {
	encoded := jsonValueForBytes(value)
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

func jsonValueForBytes(value []byte) any {
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
	for _, b := range value {
		if b != '\t' && (b < 0x20 || b >= 0x7f) {
			return false
		}
	}
	return true
}
