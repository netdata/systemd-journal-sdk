//go:build cgo

package main

import (
	"encoding/binary"
	"io"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

type timestampZoneType struct {
	offset              int64
	dst                 int
	name                string
	standard, universal bool
}
type timestampTransition struct {
	at   int64
	kind int
}
type timestampLeap struct{ at, correction int64 }
type timestampTZif struct {
	types       []timestampZoneType
	transitions []timestampTransition
	leaps       []timestampLeap
	firstName   string
	footer      [2]string
	footerRule  string
}
type timestampZone struct {
	location        *time.Location
	data            *timestampTZif
	names           [2]string
	initialNames    [2]string
	fileInfo        os.FileInfo
	renderLocations []*time.Location
}

func timestampZoneDirs() []string {
	if dir := os.Getenv("TZDIR"); dir != "" {
		return []string{dir}
	}
	return []string{"/usr/share/zoneinfo", "/usr/share/lib/zoneinfo", "/usr/lib/locale/TZ"}
}

func readTimestampZone(path string) (*timestampZone, bool) {
	file, err := os.Open(path)
	if err != nil {
		return nil, false
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil || !info.Mode().IsRegular() {
		return nil, false
	}
	data, err := io.ReadAll(io.LimitReader(file, (10<<20)+1))
	if err != nil || len(data) > 10<<20 {
		return nil, false
	}
	location, err := time.LoadLocationFromTZData(path, data)
	if err != nil {
		return nil, false
	}
	meta := parseTimestampTZif(data)
	if meta == nil {
		return nil, false
	}
	z := &timestampZone{location: location, data: meta, fileInfo: info}
	for i := len(meta.transitions) - 1; i >= 0; i-- {
		kind := meta.types[meta.transitions[i].kind]
		if z.names[kind.dst] == "" {
			z.names[kind.dst] = kind.name
		}
	}
	if z.names[0] == "" {
		z.names[0] = meta.firstName
	}
	if z.names[1] == "" {
		z.names[1] = z.names[0]
	}
	z.initialNames = z.names
	return z, true
}

func localTimestampZone() *timestampZone {
	if value, set := os.LookupEnv("TZ"); set {
		if value == "" {
			return &timestampZone{location: time.FixedZone("Universal", 0)}
		}
		value = strings.TrimPrefix(value, ":")
		if value == "" {
			return &timestampZone{location: time.UTC, names: [2]string{"UTC", "UTC"}}
		}
		if filepath.IsAbs(value) {
			if zone, ok := readTimestampZone(value); ok {
				return zone
			}
		}
		if value != "" {
			for _, dir := range timestampZoneDirs() {
				if zone, ok := readTimestampZone(filepath.Join(dir, value)); ok {
					return zone
				}
			}
		}
		if zone, ok := namedTimestampZone(value); ok {
			return zone
		}
		if zone := defaultTimestampZone(value); zone != nil {
			return zone
		}
		if zone := posixTimestampZone(value); zone != nil {
			return zone
		}
		return &timestampZone{location: time.FixedZone(fallbackTimestampName(value), 0)}
	} else if zone, ok := readTimestampZone("/etc/localtime"); ok {
		return zone
	}
	return &timestampZone{location: time.Local}
}

func posixTimestampZone(rule string) *timestampZone {
	rule = normalizeTimestampRule(rule)
	// A TZif footer lets the standard library parse POSIX rules without private APIs.
	names := timestampFooterNames(rule)
	if len(names[0]) < 3 {
		return nil
	}
	header := make([]byte, 44)
	copy(header, "TZif2")
	binary.BigEndian.PutUint32(header[32:], 1)
	binary.BigEndian.PutUint32(header[36:], 1)
	binary.BigEndian.PutUint32(header[40:], 4)
	block := func(width int) []byte {
		var b []byte
		if width == 4 {
			b = binary.BigEndian.AppendUint32(b, 1<<31)
		} else {
			b = binary.BigEndian.AppendUint64(b, 1<<63)
		}
		return append(b, 0, 0, 0, 0, 0, 0, 0, 'U', 'T', 'C', 0)
	}
	data := append(append(append(append([]byte{}, header...), block(4)...), header...), block(8)...)
	data = append(data, '\n')
	data = append(data, rule...)
	data = append(data, '\n')
	location, err := time.LoadLocationFromTZData("local", data)
	if err != nil {
		return nil
	}
	name, _ := time.Now().In(location).Zone()
	if name != names[0] && name != names[1] {
		return nil
	}
	return &timestampZone{location: location, names: names, initialNames: names}
}

func namedTimestampZone(name string) (*timestampZone, bool) {
	if name == "" || len(name) >= 4096 || strings.HasPrefix(name, "/") || strings.HasSuffix(name, "/") || strings.Contains(name, "//") {
		return nil, false
	}
	for _, c := range []byte(name) {
		if !(c >= 'A' && c <= 'Z' || c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || strings.ContainsRune("-_+/", rune(c))) {
			return nil, false
		}
	}
	installed := false
	for _, dir := range timestampZoneDirs() {
		if info, err := os.Stat(dir); err == nil && info.IsDir() {
			installed = true
		}
		if zone, ok := readTimestampZone(filepath.Join(dir, name)); ok {
			return zone, true
		}
	}
	if installed {
		return nil, false
	}
	location, err := time.LoadLocation(name)
	if err != nil {
		return nil, false
	}
	return &timestampZone{location: location}, true
}

func (z *timestampZone) restoreAfterNamed(named *timestampZone) {
	if z.data == nil {
		return
	}
	if z.fileInfo != nil && named.fileInfo != nil && os.SameFile(z.fileInfo, named.fileInfo) && z.fileInfo.ModTime().Unix() == named.fileInfo.ModTime().Unix() {
		z.names = named.names
	} else {
		z.names = z.initialNames
	}
}

func (z *timestampZone) abbreviation(name string) (int, bool) {
	if z.names[0] != "" {
		for dst, n := range z.names {
			if n == name {
				return dst, true
			}
		}
		return 0, false
	}
	now := time.Now()
	for _, month := range []time.Month{time.January, time.July} {
		t := time.Date(now.Year(), month, 1, 12, 0, 0, 0, z.location)
		abbr, _ := t.Zone()
		if abbr == name {
			if t.IsDST() {
				return 1, true
			}
			return 0, true
		}
	}
	return 0, false
}

// Providers discard leap records and may use a footer before any transition.
// Keep those native calendar semantics in the CLI's cold timestamp path.
func (z *timestampZone) civil(seconds int64, utc bool) (time.Time, int, int64) {
	t := time.Unix(seconds, 0).In(z.location)
	_, offsetInt := t.Zone()
	offset := int64(offsetInt)
	dst := 0
	if t.IsDST() {
		dst = 1
	}
	if utc {
		offset = 0
		dst = 0
	}
	if z.data != nil {
		meta := z.data
		if !utc {
			i := sort.Search(len(meta.transitions), func(i int) bool { return meta.transitions[i].at > seconds })
			if i > 0 && i == len(meta.transitions) && meta.footer[0] != "" {
				z.names = meta.footer
			} else {
				z.names = [2]string{}
				kind := 0
				if i == 0 {
					for j, k := range meta.types {
						if k.dst == 0 {
							kind = j
							break
						}
					}
					z.names[0] = meta.types[kind].name
					for _, k := range meta.types {
						if k.dst == 1 {
							z.names[1] = k.name
							break
						}
					}
				} else {
					kind = meta.transitions[i-1].kind
					k := meta.types[kind]
					z.names[k.dst] = k.name
					for _, transition := range meta.transitions[i:] {
						k := meta.types[transition.kind]
						if z.names[k.dst] == "" {
							z.names[k.dst] = k.name
						}
					}
				}
				if z.names[0] == "" {
					z.names[0] = z.names[1]
				}
				if z.names[1] == "" {
					z.names[1] = z.names[0]
				}
				offset = meta.types[kind].offset
				dst = meta.types[kind].dst
			}
		}
		i := sort.Search(len(meta.leaps), func(i int) bool { return meta.leaps[i].at > seconds })
		if i > 0 {
			leap := meta.leaps[i-1]
			offset -= leap.correction
			if leap.at == seconds && (i == 1 && leap.correction > 0 || i > 1 && leap.correction > meta.leaps[i-2].correction) {
				offset++
				for j := i - 1; j > 0 && meta.leaps[j].at == meta.leaps[j-1].at+1 && meta.leaps[j].correction == meta.leaps[j-1].correction+1; j-- {
					offset++
				}
			}
		}
	}
	return time.Unix(seconds+offset, 0).UTC(), dst, offset
}

func (z *timestampZone) leapHit(seconds int64) bool {
	if z.data == nil {
		return false
	}
	leaps := z.data.leaps
	i := sort.Search(len(leaps), func(i int) bool { return leaps[i].at >= seconds })
	return i < len(leaps) && leaps[i].at == seconds && (i == 0 && leaps[i].correction > 0 || i > 0 && leaps[i].correction > leaps[i-1].correction)
}

func parseTimestampTZif(data []byte) *timestampTZif {
	width := 4
	header := func(b []byte) ([6]uint64, bool) {
		var counts [6]uint64
		if len(b) < 44 || string(b[:4]) != "TZif" {
			return counts, false
		}
		for i := range counts {
			counts[i] = uint64(binary.BigEndian.Uint32(b[20+i*4:]))
		}
		return counts, true
	}
	size := func(n [6]uint64, w int) uint64 {
		return n[3]*uint64(w+1) + n[4]*6 + n[5] + n[2]*uint64(w+4) + n[0] + n[1]
	}
	n, ok := header(data)
	if !ok {
		return nil
	}
	if data[4] != 0 {
		skip := 44 + size(n, 4)
		if skip > uint64(len(data)) {
			return nil
		}
		data = data[skip:]
		n, ok = header(data)
		if !ok {
			return nil
		}
		width = 8
	}
	blockSize := size(n, width)
	if blockSize > uint64(len(data)-44) || n[4] == 0 || n[4] > 256 || n[0] > n[4] || n[1] > n[4] {
		return nil
	}
	block := data[44 : 44+blockSize]
	at := 0
	signed := func(b []byte) int64 {
		if width == 4 {
			return int64(int32(binary.BigEndian.Uint32(b)))
		}
		return int64(binary.BigEndian.Uint64(b))
	}
	meta := &timestampTZif{}
	for i := uint64(0); i < n[3]; i++ {
		epoch := signed(block[at:])
		at += width
		if i > 0 && epoch <= meta.transitions[i-1].at {
			return nil
		}
		meta.transitions = append(meta.transitions, timestampTransition{at: epoch})
	}
	for i := range meta.transitions {
		kind := int(block[at])
		at++
		if uint64(kind) >= n[4] {
			return nil
		}
		meta.transitions[i].kind = kind
	}
	types := block[at : at+int(n[4])*6]
	at += len(types)
	names := block[at : at+int(n[5])]
	at += len(names)
	nameAt := func(i int) (string, bool) {
		if i >= len(names) {
			return "", false
		}
		s := string(names[i:])
		end := strings.IndexByte(s, 0)
		if end < 0 {
			return "", false
		}
		return s[:end], true
	}
	meta.firstName, ok = nameAt(0)
	if !ok {
		return nil
	}
	for i := 0; i < len(types); i += 6 {
		name, valid := nameAt(int(types[i+5]))
		if !valid || types[i+4] > 1 {
			return nil
		}
		meta.types = append(meta.types, timestampZoneType{offset: int64(int32(binary.BigEndian.Uint32(types[i:]))), dst: int(types[i+4]), name: name})
	}
	for i := uint64(0); i < n[2]; i++ {
		epoch := signed(block[at:])
		at += width
		correction := int64(int32(binary.BigEndian.Uint32(block[at:])))
		at += 4
		if i > 0 && epoch <= meta.leaps[i-1].at {
			return nil
		}
		meta.leaps = append(meta.leaps, timestampLeap{epoch, correction})
	}
	for i := 0; i < int(n[1]); i++ {
		meta.types[i].standard = block[at+i] != 0
	}
	at += int(n[1])
	for i := 0; i < int(n[0]); i++ {
		meta.types[i].universal = block[at+i] != 0
	}
	if width == 8 {
		footer := strings.Trim(string(data[44+blockSize:]), "\n")
		meta.footerRule = footer
		meta.footer = timestampFooterNames(footer)
	}
	return meta
}

func timestampFooterNames(text string) [2]string {
	name := func() string {
		if strings.HasPrefix(text, "<") {
			if i := strings.IndexByte(text, '>'); i > 1 {
				n := text[1:i]
				text = text[i+1:]
				return n
			}
			return ""
		}
		i := 0
		for i < len(text) && ((text[i] >= 'a' && text[i] <= 'z') || (text[i] >= 'A' && text[i] <= 'Z')) {
			i++
		}
		n := text[:i]
		text = text[i:]
		return n
	}
	standard := name()
	i := 0
	for i < len(text) && strings.ContainsRune("+-0123456789:", rune(text[i])) {
		i++
	}
	text = text[i:]
	daylight := name()
	if daylight == "" {
		daylight = standard
	}
	return [2]string{standard, daylight}
}

// Rendering has no parser abbreviation state and reads zone data only once.
var renderingTimestampZone = sync.OnceValue(func() *timestampZone {
	z := localTimestampZone()
	if z.data != nil {
		for _, kind := range z.data.types {
			z.renderLocations = append(z.renderLocations, time.FixedZone(kind.name, int(kind.offset)))
		}
	}
	return z
})

func (z *timestampZone) displayLocation(seconds int64, utc bool) *time.Location {
	if utc {
		return time.UTC
	}
	if meta := z.data; meta != nil {
		i := sort.Search(len(meta.transitions), func(i int) bool { return meta.transitions[i].at > seconds })
		if i == 0 {
			for kind, value := range meta.types {
				if value.dst == 0 {
					return z.renderLocations[kind]
				}
			}
			return z.renderLocations[0]
		}
		if i < len(meta.transitions) || meta.footer[0] == "" {
			return z.renderLocations[meta.transitions[i-1].kind]
		}
	}
	return z.location
}

func (z *timestampZone) display(seconds int64, utc bool) (time.Time, string, int64, bool) {
	location := z.displayLocation(seconds, utc)
	t := time.Unix(seconds, 0).In(location)
	name, nativeOffset := t.Zone()

	offset := int64(nativeOffset)
	correction, leap := int64(0), false
	if z.data != nil {
		leaps := z.data.leaps
		i := sort.Search(len(leaps), func(i int) bool { return leaps[i].at > seconds })
		if i > 0 {
			correction = leaps[i-1].correction
			leap = leaps[i-1].at == seconds && (i == 1 && correction > 0 || i > 1 && correction > leaps[i-2].correction)
		}
	}
	return time.Unix(seconds+offset-correction, 0).UTC(), name, offset, leap
}

func formatCalendarTimestamp(usec uint64, utc bool, layout string, named, numeric bool) string {
	zone := renderingTimestampZone()
	if zone.data == nil || len(zone.data.leaps) == 0 {
		seconds := int64(usec / 1_000_000)
		location := zone.displayLocation(seconds, utc)
		t := time.Unix(seconds, int64(usec%1_000_000)*1000).In(location)
		if named && location.String() == "" {
			layout = strings.TrimSuffix(layout, " MST")
		}
		return t.Format(layout)
	}
	if named {
		layout = strings.TrimSuffix(layout, " MST")
	}
	if numeric {
		layout = strings.TrimSuffix(layout, "-07:00")
	}
	t, name, offset, leap := zone.display(int64(usec/1_000_000), utc)
	t = t.Add(time.Duration(usec%1_000_000) * time.Microsecond)
	var buffer [128]byte
	text := buffer[:0]
	if leap {
		// Go time.Time cannot represent second 60; retain the TZif leap label.
		before, after, _ := strings.Cut(layout, "05")
		text = t.AppendFormat(text, before)
		text = strconv.AppendInt(text, int64(t.Second()+1), 10)
		text = t.AppendFormat(text, after)
	} else {
		text = t.AppendFormat(text, layout)
	}
	if named && name != "" {
		text = append(text, ' ')
		text = append(text, name...)
	}
	if numeric {
		sign := byte('+')
		if offset < 0 {
			sign, offset = '-', -offset
		}
		text = append(text, sign)
		if offset/3600 < 10 {
			text = append(text, '0')
		}
		text = strconv.AppendInt(text, offset/3600, 10)
		text = append(text, ':', byte('0'+offset%3600/600), byte('0'+offset%600/60))
	}
	return string(text)
}

// libc retains a valid standard name even when the remaining TZ rule is invalid.
func fallbackTimestampName(value string) string {
	valid := func(b byte) bool { return b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' }
	if strings.HasPrefix(value, "<") {
		if end := strings.IndexByte(value, '>'); end >= 4 {
			for _, b := range []byte(value[1:end]) {
				if !valid(b) && !(b >= '0' && b <= '9') && b != '+' && b != '-' {
					return ""
				}
			}
			return value[1:end]
		}
		return ""
	}
	end := 0
	for end < len(value) && valid(value[end]) {
		end++
	}
	if end < 3 {
		return ""
	}
	return value[:end]
}

// Native TZ rules clamp offset components; providers otherwise accept wider hours.
func normalizeTimestampRule(rule string) string {
	var result strings.Builder
	for count := 0; count < 2; count++ {
		name := fallbackTimestampName(rule)
		if name == "" {
			break
		}
		end := len(name)
		if strings.HasPrefix(rule, "<") {
			end += 2
		}
		result.WriteString(rule[:end])
		rule = rule[end:]
		if len(rule) > 0 && (rule[0] == '+' || rule[0] == '-') {
			result.WriteByte(rule[0])
			rule = rule[1:]
		}
		for part := 0; part < 3; part++ {
			end = 0
			for end < len(rule) && rule[end] >= '0' && rule[end] <= '9' {
				end++
			}
			if end == 0 {
				break
			}
			value, err := strconv.ParseUint(rule[:end], 10, 64)
			if err != nil {
				value = ^uint64(0)
			}
			// Native parsing wraps through unsigned short before clamping.
			value &= 0xffff
			limit := uint64(59)
			if part == 0 {
				limit = 24
			}
			if value > limit {
				value = limit
			}
			result.WriteString(strconv.FormatUint(value, 10))
			rule = rule[end:]
			if part == 2 || !strings.HasPrefix(rule, ":") {
				break
			}
			result.WriteByte(':')
			rule = rule[1:]
		}
	}
	result.WriteString(rule)
	return result.String()
}

// Omitted DST dates use the native default rule file before built-in US dates.
func defaultTimestampZone(rule string) *timestampZone {
	rule = strings.TrimSuffix(normalizeTimestampRule(rule), ",")
	if strings.Contains(rule, ",") {
		return nil
	}
	names := timestampFooterNames(rule)
	standard := fallbackTimestampName(rule)
	if standard == "" {
		return nil
	}
	end := len(standard)
	if strings.HasPrefix(rule, "<") {
		end += 2
	}
	rest := strings.TrimLeft(rule[end:], "+-0123456789:")
	if fallbackTimestampName(rest) == "" {
		return nil
	}
	base := posixTimestampZone(rule + ",M3.2.0,M11.1.0")
	if base == nil {
		return nil
	}
	var offsets [2]int64
	for i, seconds := range []int64{1705276800, 1721001600} {
		_, offset := time.Unix(seconds, 0).In(base.location).Zone()
		offsets[i] = int64(offset)
	}
	for _, dir := range timestampZoneDirs() {
		z, ok := readTimestampZone(filepath.Join(dir, "posixrules"))
		if !ok || len(z.data.types) < 2 {
			continue
		}
		meta := z.data
		// Native default-file conversion starts with zero remembered DST offset.
		oldOffsets := [2]int64{meta.types[0].offset, 0}
		for i := len(meta.transitions) - 1; i >= 0; i-- {
			kind := meta.types[meta.transitions[i].kind]
			if kind.dst == 0 {
				oldOffsets[0] = kind.offset
				break
			}
		}
		previousDST := 0
		for i := range meta.transitions {
			transition := &meta.transitions[i]
			kind := meta.types[transition.kind]
			shiftKind := 0
			if previousDST != 0 && !kind.standard {
				shiftKind = 1
			}
			if !kind.universal {
				shift := offsets[shiftKind] - oldOffsets[shiftKind]
				if shift > 0 && transition.at > math.MaxInt64-shift || shift < 0 && transition.at < math.MinInt64-shift {
					return base
				}
				transition.at += shift
			}
			transition.kind, previousDST = kind.dst, kind.dst
		}
		meta.types = []timestampZoneType{{offset: offsets[0], name: names[0]}, {offset: offsets[1], dst: 1, name: names[1]}}
		meta.firstName = names[0]
		if meta.footer[0] != "" {
			meta.footer = names
			if footer := posixTimestampZone(meta.footerRule); footer != nil {
				z.location = footer.location
			}
		}
		z.names, z.initialNames = names, names
		z.fileInfo = nil
		return z
	}
	return base
}
