//go:build cgo

package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"sort"
	"strings"
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
	colors          outputColors
	urlify          bool
}

const (
	printCharThreshold = 300
	jsonThreshold      = 4096
	defaultColumns     = 80
)

func newOutputOptions(flags *cliFlags, outputFieldsSet bool, fullWidth bool) outputOptions {
	colors := detectOutputColors()
	return outputOptions{
		mode:            *flags.output,
		outputFields:    parseOutputFields(*flags.outputFieldsFlag),
		outputFieldsSet: outputFieldsSet && len(parseOutputFields(*flags.outputFieldsFlag)) > 0,
		utc:             *flags.utcFlag,
		noHostname:      *flags.noHostnameFlag,
		fullWidth:       fullWidth,
		showAll:         *flags.allFlag,
		truncateNewline: *flags.truncateNewlineFlag,
		merge:           *flags.mergeFlag,
		colors:          colors,
		urlify:          urlifyEnabled(colors),
	}
}

func parseOutputFields(value string) []string {
	var fields []string
	for _, item := range strings.Split(value, ",") {
		if item != "" {
			fields = append(fields, item)
		}
	}
	return fields
}

type outputRenderer struct {
	options            outputOptions
	previousDelta      *deltaState
	catFieldsValidated bool
	columns            int
	highlight          matchRange
}

type deltaState struct {
	realtime  uint64
	monotonic uint64
	bootID    journal.UUID
}

func newOutputRenderer(options outputOptions) *outputRenderer {
	columns := defaultColumns
	if !options.fullWidth && !options.showAll && outputSkipsMissingMessage(options.mode) {
		if n, err := parseNativeUint64(os.Getenv("COLUMNS")); err == nil && n > 0 && n <= 65535 {
			columns = int(n)
		} else if n, ok := stdoutColumns(); ok {
			columns = n
		}
	}
	return &outputRenderer{options: options, catFieldsValidated: !options.outputFieldsSet, columns: columns}
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
		return r.renderCat(entry)
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
	header, columns := shortHeader(entry, r.options, false)
	prefix := timestamp + header + ": "
	return prefix + displayMessage(entry, r.options, len(prefix)-len(header)+columns, r.columns, r.highlight) + "\n", nil
}

func (r *outputRenderer) renderWithUnit(entry *journal.Entry) (string, error) {
	timestamp, err := r.formatTimestamp(entry, timestampShortFull)
	if err != nil {
		return "", err
	}
	header, columns := shortHeader(entry, r.options, true)
	prefix := timestamp + header + ": "
	return prefix + displayMessage(entry, r.options, len(prefix)-len(header)+columns, r.columns, r.highlight) + "\n", nil
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
			diff = displayTimeDifference(currentRealtime, r.previousDelta.realtime)
		} else {
			diff = displayTimeDifference(currentMonotonic, r.previousDelta.monotonic)
		}
		delta = fmt.Sprintf(" <%s%s>", formatMonotonic(diff), marker)
	}
	r.previousDelta = &deltaState{realtime: currentRealtime, monotonic: currentMonotonic, bootID: entry.BootID}
	header, columns := shortHeader(entry, r.options, false)
	prefix := fmt.Sprintf("[%s%s]%s: ", monotonic, delta, header)
	return prefix + displayMessage(entry, r.options, len(prefix)-len(header)+columns, r.columns, r.highlight) + "\n", nil
}

func displayTimeDifference(current, previous uint64) uint64 {
	if current < previous {
		return 0
	}
	return current - previous
}

func (r *outputRenderer) renderCat(entry *journal.Entry) (string, error) {
	var b strings.Builder
	fields := r.options.outputFields
	if !r.options.outputFieldsSet {
		fields = []string{"MESSAGE"}
	}
	if !r.catFieldsValidated {
		for _, name := range fields {
			if !validFieldName(name) {
				return "", fmt.Errorf("Invalid field name: %s", name)
			}
		}
		r.catFieldsValidated = true
	}
	for _, name := range fields {
		values := entryValues(entry, name)
		if len(values) > 0 {
			if r.options.colors.level == 0 {
				b.Write(values[0])
			} else {
				on, off, highlight := r.options.colors.log(entryPriority(entry, true), false)
				b.WriteString(on)
				if name == "MESSAGE" {
					span := r.highlight
					b.Write(values[0][:span.start])
					b.WriteString(highlight)
					b.Write(values[0][span.start:span.end])
					b.WriteString(on)
					b.Write(values[0][span.end:])
				} else {
					b.Write(values[0])
				}
				b.WriteString(off)
			}
			b.WriteByte('\n')
		}
	}
	return b.String(), nil
}

func validFieldName(name string) bool {
	if name == "" || strings.HasPrefix(name, "__") {
		return false
	}
	for _, c := range []byte(name) {
		if !(c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_') {
			return false
		}
	}
	return true
}

func (r *outputRenderer) renderVerbose(entry *journal.Entry) (string, error) {
	timestamp, err := r.formatTimestamp(entry, timestampVerbose)
	if err != nil {
		return "", err
	}
	var b strings.Builder
	colors := r.options.colors
	if colors.level == 0 {
		fmt.Fprintf(&b, "%s [%s]\n", timestamp, entry.Cursor)
	} else {
		on := ""
		if colors.underline {
			on = "\x1b[0;4m"
		}
		fmt.Fprintf(&b, "%s%s%s %s[%s]%s\n", on, timestamp, ansiNormal, colors.grey(), entry.Cursor, colors.grey())
	}
	for _, field := range verboseFields(entry, r.options) {
		if colors.level == 0 {
			fmt.Fprintf(&b, "    %s=%s\n", field.name, displayVerboseValue(field.name, field.value, r.options))
			continue
		}
		on, off := "", ""
		if colors.level != 0 {
			if field.name == "MESSAGE" {
				on, off = ansiHighlight, ansiNormal
			} else if strings.HasPrefix(field.name, "_") {
				on, off = ansiGreen, ansiNormal
			}
		}
		value := displayVerboseValue(field.name, field.value, r.options)
		// Native resets after a printable multiline field's terminating newline.
		printable := r.options.showAll || (r.options.fullWidth || len(field.name)+1+len(field.value) < printCharThreshold) && journalTextPrintable(field.value)
		if field.name == "CONFIG_FILE" && printable {
			if linked := fileLink(field.value, r.options.urlify); linked != nil {
				value = string(fullMessageWithNUL(linked, 4+len(field.name)+1))
			}
		}
		if printable {
			fmt.Fprintf(&b, "    %s%s=%s\n%s", on, field.name, value, off)
		} else {
			fmt.Fprintf(&b, "    %s%s=%s%s\n", on, field.name, value, off)
		}
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
	if r.options.colors.level != 0 {
		encoded = colorJSON(encoded)
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
		return fmt.Sprintf("%10d.%06d", realtime/1_000_000, realtime%1_000_000), nil
	}
	switch mode {
	case timestampShort:
		return formatCalendarTimestamp(realtime, r.options.utc, "Jan 02 15:04:05", false, false), nil
	case timestampShortFull:
		return formatCalendarTimestamp(realtime, r.options.utc, "Mon 2006-01-02 15:04:05 MST", true, false), nil
	case timestampShortISO:
		return formatCalendarTimestamp(realtime, r.options.utc, "2006-01-02T15:04:05-07:00", false, true), nil
	case timestampShortISOPrecise:
		return formatCalendarTimestamp(realtime, r.options.utc, "2006-01-02T15:04:05.000000-07:00", false, true), nil
	case timestampShortPrecise:
		return formatCalendarTimestamp(realtime, r.options.utc, "Jan 02 15:04:05.000000", false, false), nil
	case timestampVerbose:
		return formatCalendarTimestamp(realtime, r.options.utc, "Mon 2006-01-02 15:04:05.000000 MST", true, false), nil
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
	if entry == nil {
		return 0, false
	}
	raw, ok := lastEntryValue(entry, "_SOURCE_REALTIME_TIMESTAMP")
	if !ok {
		return 0, false
	}
	value, err := parseNativeUint64(string(cStringBytes(raw)))
	if err != nil || value == 0 || value >= 1<<55 {
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

func lastEntryValue(entry *journal.Entry, name string) ([]byte, bool) {
	if values := entry.FieldValues[name]; len(values) > 0 {
		return values[len(values)-1], true
	}
	value, ok := entry.Fields[name]
	return value, ok
}

func shortField(entry *journal.Entry, name string, options outputOptions) ([]byte, bool) {
	value, ok := lastEntryValue(entry, name)
	return value, ok && (options.showAll || len(value) < printCharThreshold && journalTextPrintable(value))
}

func shortHeader(entry *journal.Entry, options outputOptions, withUnit bool) (string, int) {
	var out strings.Builder
	columns := 0
	if !options.noHostname {
		if value, ok := shortField(entry, "_HOSTNAME", options); ok {
			out.WriteByte(' ')
			out.Write(cStringBytes(value))
			columns += len(value) + 1
		}
	}
	unitShown := false
	if withUnit {
		_, unitOK := shortField(entry, "_SYSTEMD_UNIT", options)
		_, userOK := shortField(entry, "_SYSTEMD_USER_UNIT", options)
		if unitOK || userOK {
			unit, unitOK := lastEntryValue(entry, "_SYSTEMD_UNIT")
			userUnit, userOK := lastEntryValue(entry, "_SYSTEMD_USER_UNIT")
			if unitOK {
				out.WriteByte(' ')
				out.Write(cStringBytes(unit))
				columns += len(unit) + 1
			}
			if userOK {
				if unitOK {
					out.WriteByte('/')
				} else {
					out.WriteByte(' ')
				}
				out.Write(cStringBytes(userUnit))
				// v262 accounts the system-unit length here as well.
				columns += len(unit) + 1
			}
			unitShown = true
		}
	}
	if !unitShown {
		value, ok := shortField(entry, "SYSLOG_IDENTIFIER", options)
		if !ok {
			value, ok = shortField(entry, "_COMM", options)
		}
		if ok {
			out.WriteByte(' ')
			out.Write(cStringBytes(value))
			columns += len(value) + 1
		} else {
			out.WriteString(" unknown")
		}
	}
	pid, ok := shortField(entry, "_PID", options)
	if !ok {
		pid, ok = shortField(entry, "SYSLOG_PID", options)
	}
	if ok {
		out.WriteByte('[')
		out.Write(cStringBytes(pid))
		out.WriteByte(']')
		columns += len(pid) + 2
	}
	return out.String(), columns
}

func displayMessage(entry *journal.Entry, options outputOptions, prefixColumns, columns int, highlight matchRange) string {
	if options.colors.level != 0 || options.urlify {
		return displayColoredMessage(entry, options, prefixColumns, columns, highlight)
	}
	values := entryValues(entry, "MESSAGE")
	if len(values) == 0 {
		return ""
	}
	value := values[len(values)-1]
	if !options.showAll {
		var printable bool
		value, printable = prepareShortMessage(value, options.truncateNewline)
		if !printable {
			return blobData(len(value))
		}
	} else if options.truncateNewline {
		value = truncateAtNewline(value)
	}
	if options.showAll && bytes.IndexByte(value, 0) >= 0 {
		return string(fullMessageWithNUL(value, prefixColumns))
	}
	if !options.showAll && !options.fullWidth {
		return string(shortenedMessage(value, prefixColumns, columns))
	}
	out := indentContinuationLines(value, prefixColumns)
	if len(out) > 0 && out[len(out)-1] == '\n' {
		out = out[:len(out)-1]
	}
	return string(out)
}

func prepareShortMessage(value []byte, truncate bool) ([]byte, bool) {
	printable := true
	for i, ch := range string(value) {
		if ch == '\x1b' || ch == '\t' || ch == '\r' {
			value = stripShortControls(value, i)
			if truncate {
				value = truncateAtNewline(value)
			}
			return value, journalTextPrintable(value)
		}
		if truncate && (ch == '\n' || ch == 0) {
			return value[:i], printable
		}
		if ch < 0x20 && ch != '\n' || ch >= 0x7f && ch <= 0x9f {
			printable = false
		} else if ch == utf8.RuneError {
			_, size := utf8.DecodeRune(value[i:])
			printable = printable && size != 1
		}
	}
	return value, printable
}

func stripShortControls(value []byte, start int) []byte {
	return stripShortControlsWithHighlight(value, start, nil)
}

func stripShortControlsWithHighlight(value []byte, start int, highlight *matchRange) []byte {
	var original, shift matchRange
	if highlight != nil {
		original = *highlight
	}
	out := make([]byte, 0, len(value))
	out = append(out, value[:start]...)
	for i := start; i < len(value); {
		switch value[i] {
		case '\t':
			out = append(out, "        "...)
			advanceHighlight(original, &shift, i, 7)
			i++
		case '\r':
			end := i + 1
			for end < len(value) && value[end] == '\r' {
				end++
			}
			if end < len(value) && value[end] != '\n' {
				out = append(out, value[i:end]...)
			}
			i = end
		case '\x1b':
			if i+1 == len(value) {
				out = append(out, value[i])
				i++
				continue
			}
			end := i + 2
			complete := false
			invalidAt, size := i+1, 1
			switch value[i+1] {
			case '[':
				size = 2
				for end < len(value) {
					b := value[end]
					end++
					if b == 'm' {
						complete = true
						break
					}
					if b != 0 && b != ';' && b != ':' && (b < '0' || b > '9') {
						invalidAt = end - 1
						break
					}
					invalidAt = end
				}
			case ']':
				size = 2
				for end < len(value) {
					b := value[end]
					end++
					if b == '\x07' {
						complete = true
						break
					}
					if b == '\x1b' {
						invalidAt = end
						if end < len(value) && value[end] == '\\' {
							end++
							complete = true
						}
						break
					}
					if b < 32 || b >= 127 {
						invalidAt = end - 1
						break
					}
					invalidAt = end
				}
			}
			if complete {
				i = end
			} else {
				// Native preserves malformed sequences and processes their body again.
				advanceHighlight(original, &shift, invalidAt, size)
				out = append(out, value[i:i+2]...)
				i += 2
			}
		default:
			out = append(out, value[i])
			i++
		}
	}
	if highlight != nil {
		highlight.start += shift.start
		highlight.end += shift.end
	}
	return out
}

func fullMessageWithNUL(value []byte, prefixColumns int) []byte {
	var out []byte
	for start := 0; start < len(value); {
		end := bytes.IndexByte(value[start:], '\n')
		if end < 0 {
			end = len(value)
		} else {
			end += start
		}
		if start > 0 {
			out = append(out, '\n')
			out = append(out, bytes.Repeat([]byte{' '}, prefixColumns)...)
		}
		out = append(out, cStringBytes(value[start:end])...)
		start = end + 1
	}
	return out
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

func displayValue(value []byte, truncateNewline bool) string {
	if truncateNewline {
		value = truncateAtNewline(value)
	}
	return string(value)
}

func displayVerboseValue(name string, value []byte, options outputOptions) string {
	if !options.showAll && (!journalTextPrintable(value) || (!options.fullWidth && len(name)+1+len(value) >= printCharThreshold)) {
		return blobData(len(value))
	}
	if bytes.ContainsAny(value, "\n\x00") {
		return string(fullMessageWithNUL(value, 4+len(name)+1))
	}
	return string(value)
}

func truncateAtNewline(value []byte) []byte {
	if idx := bytes.IndexAny(value, "\x00\r\n"); idx >= 0 {
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

var ellipsisMarker = func() string {
	locale := os.Getenv("LC_ALL")
	if locale == "" {
		locale = os.Getenv("LC_CTYPE")
	}
	if locale == "" {
		locale = os.Getenv("LANG")
	}
	if strings.Contains(strings.ToLower(locale), "utf") {
		return "…"
	}
	return "..."
}()

func shortenedMessage(value []byte, prefixColumns, columns int) []byte {
	var out []byte
	for start, line := 0, 0; start < len(value); line++ {
		end := bytes.IndexByte(value[start:], '\n')
		if end < 0 {
			end = len(value)
		} else {
			end += start
		}
		part := value[start:end]
		tail := line == 2 || end+1 >= printCharThreshold
		shorten := prefixColumns+len(part)+1 >= columns || tail
		if line > 0 {
			out = append(out, '\n')
		}
		if shorten && columns-prefixColumns < 3 {
			out = append(out, "..."...)
		} else {
			if line > 0 {
				out = append(out, bytes.Repeat([]byte{' '}, prefixColumns)...)
			}
			if !shorten {
				out = append(out, part...)
			} else if columns-prefixColumns > len(part)+3 {
				out = append(out, part...)
				out = append(out, "..."...)
			} else {
				percent := 90
				if tail {
					percent = 100
				}
				out = append(out, ellipsizeLine(part, columns-prefixColumns, percent)...)
			}
		}
		if tail {
			break
		}
		start = end + 1
	}
	return out
}

func ellipsizeLine(value []byte, columns, percent int) []byte {
	if isASCII(value) {
		if len(value) <= columns {
			return value
		}
		markerWidth := len(ellipsisMarker)
		if ellipsisMarker == "…" {
			markerWidth = 1
		}
		left := ((columns-markerWidth)*percent + 50) / 100
		right := columns - markerWidth - left
		out := append([]byte{}, value[:left]...)
		out = append(out, ellipsisMarker...)
		return append(out, value[len(value)-right:]...)
	}
	left, right, used := 0, len(value), 0
	budget := (columns - 1) * percent / 100
	for left < len(value) {
		ch, size := utf8.DecodeRune(value[left:])
		width := journalRuneWidth(ch)
		if used+width > budget {
			break
		}
		used += width
		left += size
	}
	for right > left && used < columns {
		ch, size := utf8.DecodeLastRune(value[left:right])
		width := journalRuneWidth(ch)
		if used+width > columns {
			break
		}
		used += width
		right -= size
	}
	if left >= right {
		return value
	}
	if used >= columns {
		if right < len(value) {
			_, size := utf8.DecodeRune(value[right:])
			right += size
		} else if left > 0 {
			_, size := utf8.DecodeLastRune(value[:left])
			left -= size
		}
	}
	out := append([]byte{}, value[:left]...)
	out = append(out, "…"...)
	return append(out, value[right:]...)
}

func isASCII(value []byte) bool {
	for _, b := range value {
		if b >= utf8.RuneSelf {
			return false
		}
	}
	return true
}

// v262 fixes character width to these intervals.
var journalWideRunes = [][2]rune{
	{0x1100, 0x115F},
	{0x2329, 0x232A},
	{0x2E80, 0x2E99},
	{0x2E9B, 0x2EF3},
	{0x2F00, 0x2FD5},
	{0x2FF0, 0x2FFB},
	{0x3000, 0x303E},
	{0x3041, 0x3096},
	{0x3099, 0x30FF},
	{0x3105, 0x312D},
	{0x3131, 0x318E},
	{0x3190, 0x31BA},
	{0x31C0, 0x31E3},
	{0x31F0, 0x321E},
	{0x3220, 0x3247},
	{0x3250, 0x32FE},
	{0x3300, 0x4DBF},
	{0x4E00, 0xA48C},
	{0xA490, 0xA4C6},
	{0xA960, 0xA97C},
	{0xAC00, 0xD7A3},
	{0xF900, 0xFAFF},
	{0xFE10, 0xFE19},
	{0xFE30, 0xFE52},
	{0xFE54, 0xFE66},
	{0xFE68, 0xFE6B},
	{0xFF01, 0xFF60},
	{0xFFE0, 0xFFE6},
	{0x1B000, 0x1B001},
	{0x1F200, 0x1F202},
	{0x1F210, 0x1F23A},
	{0x1F240, 0x1F248},
	{0x1F250, 0x1F251},
	{0x1F300, 0x1F567},
	{0x20000, 0x2FFFD},
	{0x30000, 0x3FFFD},
}

func journalRuneWidth(ch rune) int {
	i := sort.Search(len(journalWideRunes), func(i int) bool { return journalWideRunes[i][1] >= ch })
	if i < len(journalWideRunes) && journalWideRunes[i][0] <= ch {
		return 2
	}
	return 1
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
		if values, ok := existing.([]any); ok && len(values) > 0 {
			// A scalar binary value is a number array, not repeated field values.
			if _, binary := values[0].(int); !binary {
				object[name] = append(values, encoded)
				return
			}
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
