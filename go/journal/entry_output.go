package journal

import (
	"bytes"
	"encoding/binary"
	"slices"
	"sort"
	"strconv"
	"unicode/utf8"
)

// jsonNullThreshold is systemd's JSON_THRESHOLD: without ShowAll, JSON values
// whose "NAME=value" length reaches it are printed as null.
const jsonNullThreshold = 4096

var nullSeqnumID = []byte("00000000000000000000000000000000")

// EntryOutputOptions selects what export and JSON output include, as the
// journalctl --output-fields and --all options do.
type EntryOutputOptions struct {
	// Fields limits DATA fields to these names. Nil prints every field. The
	// metadata fields (__CURSOR, __REALTIME_TIMESTAMP, __MONOTONIC_TIMESTAMP,
	// __SEQNUM, __SEQNUM_ID, _BOOT_ID) are always printed.
	Fields []string
	// ShowAll keeps JSON values of 4096 bytes or more instead of printing null.
	ShowAll bool
}

// EntryEncoder renders entries in systemd's export and JSON formats
// (systemd logs-show.c output_export and output_json). It appends to a caller
// buffer and reuses internal scratch space, so one encoder serves a whole
// stream; it is not safe for concurrent use.
type EntryEncoder struct {
	fields  map[string]struct{}
	showAll bool
	// lossless keeps invalid and non-UTF-8 field names in export instead of
	// stopping there, the ExportEntry contract since SOW-0043.
	lossless  bool
	meta      []byte
	metaItems [6]jsonItem
	items     []jsonItem
}

type jsonItem struct {
	name  []byte
	value []byte
}

// NewEntryEncoder returns an encoder for the given options.
func NewEntryEncoder(options EntryOutputOptions) *EntryEncoder {
	e := &EntryEncoder{showAll: options.ShowAll}
	if options.Fields != nil {
		e.fields = make(map[string]struct{}, len(options.Fields))
		for _, name := range options.Fields {
			e.fields[name] = struct{}{}
		}
	}
	return e
}

func (e *EntryEncoder) selected(name []byte) bool {
	if e.fields == nil {
		return true
	}
	_, ok := e.fields[string(name)]
	return ok
}

// AppendExport appends the entry in journal export format. Metadata comes
// first, then DATA fields in on-disk order without the _BOOT_ID field (the
// header boot ID is already printed). A field without '=' or with an invalid
// name ends the entry, as systemd treats it as partially written.
func (e *EntryEncoder) AppendExport(dst []byte, entry *Entry) []byte {
	metadata := e.metadata(entry)
	for i := range metadata {
		// Lossless output must stay parseable for any caller-built cursor.
		if e.lossless {
			dst = appendExportField(dst, metadata[i].name, metadata[i].value)
			continue
		}
		dst = append(dst, metadata[i].name...)
		dst = append(dst, '=')
		dst = append(dst, metadata[i].value...)
		dst = append(dst, '\n')
	}
	forEachEntryData(entry, func(name, value []byte, separated bool) bool {
		if separated && string(name) == "_BOOT_ID" {
			return true
		}
		if !separated || !journaldFieldNameValid(name, true) {
			if !e.lossless {
				return false
			}
			if !separated {
				return true
			}
		}
		if !e.selected(name) {
			return true
		}
		dst = appendExportField(dst, name, value)
		return true
	})
	return append(dst, '\n')
}

func appendExportField(dst, name, value []byte) []byte {
	// "NAME=value" is text exactly when both halves are: '=' is ASCII.
	dst = append(dst, name...)
	if IsPrintableText(name, false) && IsPrintableText(value, false) {
		dst = append(dst, '=')
		dst = append(dst, value...)
		return append(dst, '\n')
	}
	dst = append(dst, '\n')
	dst = binary.LittleEndian.AppendUint64(dst, uint64(len(value)))
	dst = append(dst, value...)
	return append(dst, '\n')
}

// AppendJSON appends the entry as one line of compact JSON followed by a
// newline. Repeated fields become arrays; non-text values become byte arrays.
// Keys are sorted (systemd's order is unspecified).
func (e *EntryEncoder) AppendJSON(dst []byte, entry *Entry) []byte {
	return append(e.appendJSON(dst, entry, false), '\n')
}

// AppendJSONPretty appends the entry as systemd's tab-indented pretty JSON
// followed by a newline.
func (e *EntryEncoder) AppendJSONPretty(dst []byte, entry *Entry) []byte {
	return append(e.appendJSON(dst, entry, true), '\n')
}

func (e *EntryEncoder) appendJSON(dst []byte, entry *Entry, pretty bool) []byte {
	items := e.jsonItems(entry)
	if len(items) == 0 {
		return append(dst, "{}"...)
	}
	dst = append(dst, '{')
	for start := 0; start < len(items); {
		end := start + 1
		for end < len(items) && bytes.Equal(items[end].name, items[start].name) {
			end++
		}
		if start > 0 {
			dst = append(dst, ',')
		}
		if pretty {
			dst = append(dst, "\n\t"...)
		}
		dst = appendJSONString(dst, items[start].name)
		if pretty {
			dst = append(dst, " : "...)
		} else {
			dst = append(dst, ':')
		}
		if end-start == 1 {
			dst = e.appendJSONValue(dst, items[start], pretty, 1)
		} else {
			dst = append(dst, '[')
			for i := start; i < end; i++ {
				if i > start {
					dst = append(dst, ',')
				}
				if pretty {
					dst = append(dst, "\n\t\t"...)
				}
				dst = e.appendJSONValue(dst, items[i], pretty, 2)
			}
			if pretty {
				dst = append(dst, "\n\t"...)
			}
			dst = append(dst, ']')
		}
		start = end
	}
	if pretty {
		dst = append(dst, '\n')
	}
	return append(dst, '}')
}

func (e *EntryEncoder) appendJSONValue(dst []byte, item jsonItem, pretty bool, depth int) []byte {
	if !e.showAll && len(item.name)+1+len(item.value) >= jsonNullThreshold {
		return append(dst, "null"...)
	}
	if IsPrintableText(item.value, true) {
		return appendJSONString(dst, item.value)
	}
	if len(item.value) == 0 {
		return append(dst, "[]"...)
	}
	dst = append(dst, '[')
	for i, b := range item.value {
		if i > 0 {
			dst = append(dst, ',')
		}
		if pretty {
			dst = append(dst, '\n')
			for j := 0; j <= depth; j++ {
				dst = append(dst, '\t')
			}
		}
		dst = strconv.AppendUint(dst, uint64(b), 10)
	}
	if pretty {
		dst = append(dst, '\n')
		for j := 0; j < depth; j++ {
			dst = append(dst, '\t')
		}
	}
	return append(dst, ']')
}

// appendJSONString escapes as systemd's sd-json does: quote, backslash and
// C0 controls only, so text bytes are emitted unchanged.
func appendJSONString(dst, value []byte) []byte {
	const hex = "0123456789abcdef"
	dst = append(dst, '"')
	for _, c := range value {
		switch c {
		case '"':
			dst = append(dst, '\\', '"')
		case '\\':
			dst = append(dst, '\\', '\\')
		case '\b':
			dst = append(dst, '\\', 'b')
		case '\f':
			dst = append(dst, '\\', 'f')
		case '\n':
			dst = append(dst, '\\', 'n')
		case '\r':
			dst = append(dst, '\\', 'r')
		case '\t':
			dst = append(dst, '\\', 't')
		default:
			if c < ' ' {
				dst = append(dst, '\\', 'u', '0', '0', hex[c>>4], hex[c&0xf])
			} else {
				dst = append(dst, c)
			}
		}
	}
	return append(dst, '"')
}

// jsonItems returns the metadata and selected DATA fields sorted by name,
// keeping metadata first and on-disk order among values of one field.
// Unlike export, JSON skips an invalid field and continues (systemd
// update_json_data_split).
func (e *EntryEncoder) jsonItems(entry *Entry) []jsonItem {
	items := append(e.items[:0], e.metadata(entry)...)
	forEachEntryData(entry, func(name, value []byte, separated bool) bool {
		if !separated || string(name) == "_BOOT_ID" || !journaldFieldNameValid(name, true) || !e.selected(name) {
			return true
		}
		items = append(items, jsonItem{name: name, value: value})
		return true
	})
	slices.SortStableFunc(items, func(a, b jsonItem) int { return bytes.Compare(a.name, b.name) })
	e.items = items
	return items
}

// metadata returns the six fields systemd prints for every entry. The values
// alias the encoder's scratch buffer until the next call.
func (e *EntryEncoder) metadata(entry *Entry) []jsonItem {
	buf := e.meta[:0]
	buf = append(buf, entry.Cursor...)
	cursorEnd := len(buf)
	buf = strconv.AppendUint(buf, entry.Realtime, 10)
	realtimeEnd := len(buf)
	buf = strconv.AppendUint(buf, entry.Monotonic, 10)
	monotonicEnd := len(buf)
	buf = strconv.AppendUint(buf, entry.Seqnum, 10)
	seqnumEnd := len(buf)
	if seqnumID, _, _, _, err := ParseCursor(entry.Cursor); err == nil && seqnumID != "" {
		buf = append(buf, seqnumID...)
	} else {
		buf = append(buf, nullSeqnumID...)
	}
	seqnumIDEnd := len(buf)
	buf = append(buf, entry.BootID.String()...)
	e.meta = buf
	out := &e.metaItems
	out[0] = jsonItem{[]byte("__CURSOR"), buf[:cursorEnd]}
	out[1] = jsonItem{[]byte("__REALTIME_TIMESTAMP"), buf[cursorEnd:realtimeEnd]}
	out[2] = jsonItem{[]byte("__MONOTONIC_TIMESTAMP"), buf[realtimeEnd:monotonicEnd]}
	out[3] = jsonItem{[]byte("__SEQNUM"), buf[monotonicEnd:seqnumEnd]}
	out[4] = jsonItem{[]byte("__SEQNUM_ID"), buf[seqnumEnd:seqnumIDEnd]}
	out[5] = jsonItem{[]byte("_BOOT_ID"), buf[seqnumIDEnd:]}
	return out[:]
}

// forEachEntryData visits the entry's DATA fields in on-disk order until
// visit returns false. Entries built by hand without Payloads or RawFields
// fall back to the field maps in name order. separated is false for a
// payload without '='.
func forEachEntryData(entry *Entry, visit func(name, value []byte, separated bool) bool) {
	switch {
	case len(entry.Payloads) > 0:
		for _, payload := range entry.Payloads {
			name, value, ok := splitRawPayload(payload)
			if !ok {
				name, value = payload, nil
			}
			if !visit(name, value, ok) {
				return
			}
		}
	case len(entry.RawFields) > 0:
		for _, field := range entry.RawFields {
			if !visit(field.Name, field.Value, true) {
				return
			}
		}
	default:
		names := make([]string, 0, len(entry.FieldValues)+len(entry.Fields))
		for name := range entry.FieldValues {
			names = append(names, name)
		}
		for name := range entry.Fields {
			if _, ok := entry.FieldValues[name]; !ok {
				names = append(names, name)
			}
		}
		sort.Strings(names)
		for _, name := range names {
			values := entry.FieldValues[name]
			if len(values) == 0 {
				value, ok := entry.Fields[name]
				if !ok {
					continue
				}
				values = [][]byte{value}
			}
			for _, value := range values {
				if !visit([]byte(name), value, true) {
					return
				}
			}
		}
	}
}

// IsPrintableText reports whether value is text that systemd prints as is
// (utf8_is_printable_newline): valid UTF-8 without C0/C1 controls other than
// tab (and newline when allowNewline), surrogates or Unicode noncharacters.
func IsPrintableText(value []byte, allowNewline bool) bool {
	for i := 0; i < len(value); {
		c := value[i]
		if c < utf8.RuneSelf {
			if (c < ' ' && c != '\t' && (c != '\n' || !allowNewline)) || c == 0x7f {
				return false
			}
			i++
			continue
		}
		r, size := utf8.DecodeRune(value[i:])
		if r == utf8.RuneError && size == 1 {
			return false
		}
		if r <= 0x9f || (r >= 0xfdd0 && r <= 0xfdef) || r&0xfffe == 0xfffe {
			return false
		}
		i += size
	}
	return true
}

// ExportEntry returns the entry in journal export format with every field.
// Unlike EntryEncoder, which stops where systemd does, it keeps fields with
// invalid or non-UTF-8 names so no data is lost.
func ExportEntry(entry *Entry) string {
	e := EntryEncoder{lossless: true}
	return string(e.AppendExport(nil, entry))
}

// JSONEntry returns the entry as a JSON-ready map: text values as strings,
// other values as byte slices of ints, repeated fields as arrays. Values are
// never replaced by null (journalctl --all), so no data is lost.
func JSONEntry(entry *Entry) (map[string]interface{}, error) {
	e := EntryEncoder{showAll: true}
	items := e.jsonItems(entry)
	result := make(map[string]interface{}, len(items))
	for start := 0; start < len(items); {
		end := start + 1
		for end < len(items) && bytes.Equal(items[end].name, items[start].name) {
			end++
		}
		name := string(items[start].name)
		if end-start == 1 {
			result[name] = jsonMapValue(items[start].value)
		} else {
			values := make([]interface{}, 0, end-start)
			for i := start; i < end; i++ {
				values = append(values, jsonMapValue(items[i].value))
			}
			result[name] = values
		}
		start = end
	}
	return result, nil
}

func jsonMapValue(value []byte) interface{} {
	if IsPrintableText(value, true) {
		return string(value)
	}
	values := make([]int, len(value))
	for i, b := range value {
		values[i] = int(b)
	}
	return values
}
