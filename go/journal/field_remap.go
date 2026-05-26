package journal

import (
	"crypto/md5"
	"encoding/binary"
	"encoding/hex"
	"math/bits"
	"strings"
)

const (
	remappingMarker = "ND_REMAPPING"
	remappedPrefix  = "ND_"
)

type remappedFieldMapping struct {
	original string
	mapped   string
}

type rdpCharType uint8

const (
	rdpLowercase rdpCharType = iota
	rdpUppercase
	rdpDot
	rdpUnderscore
	rdpHyphen
)

type rdpTokenType uint8

const (
	rdpTokenLowercase rdpTokenType = iota
	rdpTokenUppercase
	rdpTokenCapitalized
)

type rdpSeparator uint8

const (
	rdpSepDot rdpSeparator = iota
	rdpSepHyphen
	rdpSepUnderscore
)

type rdpToken struct {
	word  bool
	kind  rdpTokenType
	start int
	end   int
	sep   rdpSeparator
}

type rdpFieldType uint8

const (
	rdpFieldLowercase rdpFieldType = iota
	rdpFieldUppercase
	rdpFieldLowerCamel
	rdpFieldUpperCamel
	rdpFieldEmpty
)

type rdpNode struct {
	field bool
	ftype rdpFieldType
	sep   rdpSeparator
}

type rdpFieldBuilder struct {
	ftype    rdpFieldType
	start    int
	end      int
	extended bool
}

func isSystemdCompatibleLogFieldName(name string) bool {
	if len(name) == 0 || len(name) > 64 {
		return false
	}
	if name[0] < 'A' || name[0] > 'Z' {
		return false
	}
	for i := 0; i < len(name); i++ {
		c := name[i]
		if c == '_' || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			continue
		}
		return false
	}
	return true
}

func remapLogFields(fields []Field, registry map[string]string) ([]Field, []remappedFieldMapping) {
	out := make([]Field, 0, len(fields))
	newMappings := make([]remappedFieldMapping, 0)
	pending := make(map[string]string)

	for _, field := range fields {
		if isSystemdCompatibleLogFieldName(field.Name) {
			out = append(out, field)
			continue
		}

		mapped, ok := registry[field.Name]
		if !ok {
			mapped, ok = pending[field.Name]
		}
		if !ok {
			mapped = encodeRemappedFieldName([]byte(field.Name))
			pending[field.Name] = mapped
			newMappings = append(newMappings, remappedFieldMapping{
				original: field.Name,
				mapped:   mapped,
			})
		}
		out = append(out, Field{Name: mapped, Value: field.Value})
	}

	return out, newMappings
}

func encodeRemappedFieldName(fieldName []byte) string {
	encoded, ok := rdpEncode(fieldName)
	if !ok {
		return rdpMD5Fallback(fieldName)
	}

	var compressed string
	if rdpHasChecksum(encoded) {
		compressed = encoded[:2] + rdpCompressRuns(encoded[2:])
	} else {
		compressed = rdpCompressRuns(encoded)
	}

	normalized := strings.ToUpper(string(fieldName))
	normalized = strings.NewReplacer(".", "_", "-", "_").Replace(normalized)
	if suffix, ok := strings.CutPrefix(normalized, "RESOURCE_ATTRIBUTES_"); ok {
		normalized = "RA_" + suffix
	} else if suffix, ok := strings.CutPrefix(normalized, "LOG_ATTRIBUTES_"); ok {
		normalized = "LA_" + suffix
	} else if suffix, ok := strings.CutPrefix(normalized, "LOG_BODY_"); ok {
		normalized = "LB_" + suffix
	}

	result := "ND" + strings.ToUpper(compressed) + "_" + normalized
	if len(result) > 64 {
		return rdpMD5Fallback(fieldName)
	}
	return result
}

func rdpMD5Fallback(fieldName []byte) string {
	sum := md5.Sum(fieldName)
	return remappedPrefix + strings.ToUpper(hex.EncodeToString(sum[:]))
}

func rdpEncode(fieldName []byte) (string, bool) {
	tokens, ok := rdpTokenize(fieldName)
	if !ok {
		return "", false
	}
	nodes := rdpParse(tokens)
	return rdpEncodeNodes(string(fieldName), nodes), true
}

func rdpCharKind(c byte) (rdpCharType, bool) {
	switch {
	case c >= 'a' && c <= 'z':
		return rdpLowercase, true
	case (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9'):
		return rdpUppercase, true
	case c == '.':
		return rdpDot, true
	case c == '_':
		return rdpUnderscore, true
	case c == '-':
		return rdpHyphen, true
	default:
		return 0, false
	}
}

func rdpTokenize(fieldName []byte) ([]rdpToken, bool) {
	if len(fieldName) == 0 {
		return nil, true
	}

	tokens := make([]rdpToken, 0)
	start := 0
	var prev rdpCharType
	var first rdpCharType
	hasPrev := false
	hasLowercase := false
	hasUppercase := false

	for i := 0; i < len(fieldName); i++ {
		curr, ok := rdpCharKind(fieldName[i])
		if !ok {
			return nil, false
		}
		if !hasPrev {
			first = curr
			prev = curr
			hasPrev = true
			continue
		}

		shouldSplit := false
		switch {
		case prev == rdpDot || prev == rdpUnderscore || prev == rdpHyphen:
			shouldSplit = true
		case curr == rdpDot || curr == rdpUnderscore || curr == rdpHyphen:
			shouldSplit = true
		case prev == rdpUppercase && curr == rdpUppercase:
			if i+1 < len(fieldName) {
				next, ok := rdpCharKind(fieldName[i+1])
				if !ok {
					return nil, false
				}
				shouldSplit = next == rdpLowercase
			}
		case prev == rdpLowercase && curr == rdpLowercase:
			shouldSplit = false
		case prev == rdpUppercase && curr == rdpLowercase:
			shouldSplit = hasUppercase && hasLowercase
		default:
			shouldSplit = true
		}

		if shouldSplit {
			tokens = append(tokens, rdpCreateToken(first, hasLowercase, hasUppercase, start, i))
			start = i
			first = curr
			hasLowercase = false
			hasUppercase = false
		} else {
			if curr == rdpLowercase {
				hasLowercase = true
			} else if curr == rdpUppercase {
				hasUppercase = true
			}
		}
		prev = curr
	}

	if start < len(fieldName) {
		tokens = append(tokens, rdpCreateToken(first, hasLowercase, hasUppercase, start, len(fieldName)))
	}
	return tokens, true
}

func rdpCreateToken(first rdpCharType, hasLowercase, hasUppercase bool, start, end int) rdpToken {
	switch first {
	case rdpLowercase:
		return rdpToken{word: true, kind: rdpTokenLowercase, start: start, end: end}
	case rdpUppercase:
		if hasLowercase {
			return rdpToken{word: true, kind: rdpTokenCapitalized, start: start, end: end}
		}
		return rdpToken{word: true, kind: rdpTokenUppercase, start: start, end: end}
	case rdpDot:
		return rdpToken{sep: rdpSepDot}
	case rdpHyphen:
		return rdpToken{sep: rdpSepHyphen}
	default:
		return rdpToken{sep: rdpSepUnderscore}
	}
}

func rdpParse(tokens []rdpToken) []rdpNode {
	nodes := make([]rdpNode, 0)
	if len(tokens) > 0 && !tokens[0].word {
		nodes = append(nodes, rdpNode{field: true, ftype: rdpFieldEmpty})
	}

	var builder *rdpFieldBuilder
	for i, token := range tokens {
		if !token.word {
			if builder != nil {
				nodes = append(nodes, rdpNode{field: true, ftype: builder.ftype})
				builder = nil
			}
			nodes = append(nodes, rdpNode{sep: token.sep})
			if i+1 >= len(tokens) || !tokens[i+1].word {
				nodes = append(nodes, rdpNode{field: true, ftype: rdpFieldEmpty})
			}
			continue
		}

		if builder != nil {
			if builder.canAdd(token.kind) {
				builder.end = token.end
				builder.extended = true
				continue
			}
			if builder.ftype == rdpFieldLowercase && !builder.extended && token.kind == rdpTokenCapitalized {
				builder.ftype = rdpFieldLowerCamel
				builder.end = token.end
				builder.extended = true
				continue
			}
			nodes = append(nodes, rdpNode{field: true, ftype: builder.ftype})
		}
		builder = &rdpFieldBuilder{ftype: rdpFieldTypeForToken(token.kind), start: token.start, end: token.end}
	}

	if builder != nil {
		nodes = append(nodes, rdpNode{field: true, ftype: builder.ftype})
	}
	return nodes
}

func (b rdpFieldBuilder) canAdd(kind rdpTokenType) bool {
	return (b.ftype == rdpFieldLowercase && kind == rdpTokenLowercase) ||
		(b.ftype == rdpFieldUppercase && kind == rdpTokenUppercase) ||
		(b.ftype == rdpFieldLowerCamel && kind == rdpTokenCapitalized) ||
		(b.ftype == rdpFieldUpperCamel && kind == rdpTokenCapitalized)
}

func rdpFieldTypeForToken(kind rdpTokenType) rdpFieldType {
	switch kind {
	case rdpTokenLowercase:
		return rdpFieldLowercase
	case rdpTokenUppercase:
		return rdpFieldUppercase
	default:
		return rdpFieldUpperCamel
	}
}

func rdpEncodeNodes(source string, nodes []rdpNode) string {
	hasCamel := false
	for _, node := range nodes {
		if node.field && (node.ftype == rdpFieldLowerCamel || node.ftype == rdpFieldUpperCamel) {
			hasCamel = true
			break
		}
	}

	var out strings.Builder
	if hasCamel {
		out.WriteString(rdpChecksum(source))
	}

	for i := 0; i < len(nodes); {
		node := nodes[i]
		if !node.field {
			i++
			continue
		}
		nextIsSeparator := i+1 < len(nodes) && !nodes[i+1].field
		nextIsField := i+1 < len(nodes) && nodes[i+1].field
		var sep rdpSeparator
		if nextIsSeparator {
			sep = nodes[i+1].sep
		}
		out.WriteByte(rdpPairChar(node.ftype, nextIsSeparator, nextIsField, sep))
		if nextIsSeparator {
			i += 2
		} else {
			i++
		}
	}
	return out.String()
}

func rdpPairChar(ftype rdpFieldType, nextIsSeparator, nextIsField bool, sep rdpSeparator) byte {
	base := byte('a')
	switch ftype {
	case rdpFieldLowercase:
		base = 'a'
	case rdpFieldLowerCamel:
		base = 'f'
	case rdpFieldUpperCamel:
		base = 'k'
	case rdpFieldUppercase:
		base = 'p'
	case rdpFieldEmpty:
		base = 'u'
	}
	if nextIsSeparator {
		switch sep {
		case rdpSepDot:
			return base
		case rdpSepUnderscore:
			return base + 1
		default:
			return base + 2
		}
	}
	if nextIsField && ftype != rdpFieldEmpty {
		return base + 3
	}
	return base + 4
}

func rdpChecksum(source string) string {
	msg := append([]byte(source), 0xff)
	hash := sipHash13Zero(msg)
	first := int((hash / 36) % 36)
	second := int(hash % 36)
	return string([]byte{rdpChecksumChar(first), rdpChecksumChar(second)})
}

func rdpChecksumChar(idx int) byte {
	if idx < 26 {
		return byte('A' + idx)
	}
	return byte('0' + idx - 26)
}

func rdpHasChecksum(encoded string) bool {
	if encoded == "" {
		return false
	}
	c := encoded[0]
	return (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
}

func rdpCompressRuns(s string) string {
	if s == "" {
		return ""
	}
	var out strings.Builder
	for i := 0; i < len(s); {
		ch := s[i]
		count := 1
		for i+count < len(s) && s[i+count] == ch {
			count++
		}
		if count <= 2 {
			for j := 0; j < count; j++ {
				out.WriteByte(ch)
			}
		} else {
			remaining := count
			for remaining > 0 {
				if remaining > 9 {
					out.WriteByte('9')
					out.WriteByte(ch)
					remaining -= 9
				} else if remaining > 2 {
					out.WriteByte(byte('0' + remaining))
					out.WriteByte(ch)
					remaining = 0
				} else {
					for j := 0; j < remaining; j++ {
						out.WriteByte(ch)
					}
					remaining = 0
				}
			}
		}
		i += count
	}
	return out.String()
}

func sipHash13Zero(msg []byte) uint64 {
	var k0, k1 uint64
	v0 := uint64(0x736f6d6570736575) ^ k0
	v1 := uint64(0x646f72616e646f6d) ^ k1
	v2 := uint64(0x6c7967656e657261) ^ k0
	v3 := uint64(0x7465646279746573) ^ k1

	remaining := msg
	for len(remaining) >= 8 {
		m := binary.LittleEndian.Uint64(remaining[:8])
		v3 ^= m
		v0, v1, v2, v3 = sipHash13Round(v0, v1, v2, v3)
		v0 ^= m
		remaining = remaining[8:]
	}

	b := uint64(len(msg)) << 56
	for i, c := range remaining {
		b |= uint64(c) << (8 * uint(i))
	}

	v3 ^= b
	v0, v1, v2, v3 = sipHash13Round(v0, v1, v2, v3)
	v0 ^= b
	v2 ^= 0xff
	for i := 0; i < 3; i++ {
		v0, v1, v2, v3 = sipHash13Round(v0, v1, v2, v3)
	}

	return v0 ^ v1 ^ v2 ^ v3
}

func sipHash13Round(v0, v1, v2, v3 uint64) (uint64, uint64, uint64, uint64) {
	v0 += v1
	v1 = bits.RotateLeft64(v1, 13)
	v1 ^= v0
	v0 = bits.RotateLeft64(v0, 32)
	v2 += v3
	v3 = bits.RotateLeft64(v3, 16)
	v3 ^= v2
	v0 += v3
	v3 = bits.RotateLeft64(v3, 21)
	v3 ^= v0
	v2 += v1
	v1 = bits.RotateLeft64(v1, 17)
	v1 ^= v2
	v2 = bits.RotateLeft64(v2, 32)
	return v0, v1, v2, v3
}
