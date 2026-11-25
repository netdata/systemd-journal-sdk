//go:build cgo

package main

import (
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const unitNameChars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:-_.\\@"

type unitSelection struct{ system, user []string }

func rawUnitSpecs(flags *cliFlags) unitSelection {
	s := unitSelection{flags.unitFlag.Values(), flags.userUnitFlag.Values()}
	if *flags.userFlag && !*flags.systemFlag {
		for _, unit := range s.system {
			if !containsString(s.user, unit) {
				s.user = append(s.user, unit)
			}
		}
		s.system = nil
	}
	return s
}

func effectiveUnitSpecs(flags *cliFlags) ([]string, []string, error) {
	if !flags.unitsReady {
		flags.unitsReady = true
		raw := rawUnitSpecs(flags)
		for i, values := range [][]string{raw.system, raw.user} {
			var normalized []string
			for _, value := range values {
				name, escaped, err := normalizeUnitName(value)
				if err != nil {
					flags.unitsError = err
					break
				}
				if escaped != "" && !flags.effectiveQuiet() && flags.diagnostics != nil {
					fmt.Fprintf(flags.diagnostics, "Invalid unit name \"%s\" escaped as \"%s\" (maybe you should use systemd-escape?).\n", value, escaped)
				}
				normalized = append(normalized, name)
			}
			if i == 0 {
				flags.units.system = normalized
			} else {
				flags.units.user = normalized
			}
			if flags.unitsError != nil {
				break
			}
		}
	}
	return flags.units.system, flags.units.user, flags.unitsError
}

func unitHasSuffix(name string) bool {
	for _, suffix := range unitSuffixes {
		if strings.HasSuffix(name, suffix) {
			return true
		}
	}
	return false
}

func validUnitName(name string) bool {
	if len(name) == 0 || len(name) >= 256 || strings.LastIndexByte(name, '.') <= 0 || name[0] == '@' || !unitHasSuffix(name) {
		return false
	}
	for i := 0; i < len(name); i++ {
		if strings.IndexByte(unitNameChars, name[i]) < 0 {
			return false
		}
	}
	return true
}

func normalizeUnitName(value string) (name, escaped string, err error) {
	if value == "" || strings.IndexByte(value, 0) >= 0 {
		return "", "", fmt.Errorf("Failed to mangle unit name '%s': Invalid argument", value)
	}
	valid := func(chars string) bool {
		for i := 0; i < len(value); i++ {
			if !strings.ContainsRune(chars, rune(value[i])) {
				return false
			}
		}
		return true
	}
	if validUnitName(value) || isGlobPattern(value) && valid(unitNameChars+"[]!*?") {
		return value, "", nil
	}
	if strings.HasPrefix(value, "/") {
		var components []string
		for _, part := range strings.Split(value, "/") {
			if part == "" || part == "." || part == ".." && len(components) == 0 {
				continue
			}
			components = append(components, part)
		}
		path := strings.Join(components, "/")
		if !containsString(components, "..") && validUnitPathComponents(components) && len(path) < 4095 {
			suffix := ".mount"
			if strings.HasPrefix(path, "dev/") || strings.HasPrefix(path, "sys/") {
				suffix = ".device"
			}
			var b strings.Builder
			if path == "" {
				b.WriteByte('-')
			}
			for i := 0; i < len(path); i++ {
				c := path[i]
				switch {
				case c == '/':
					b.WriteByte('-')
				case c == '-' || c == '\\' || c == '.' && i == 0 || !strings.ContainsRune(strings.TrimSuffix(unitNameChars, "@"), rune(c)):
					fmt.Fprintf(&b, `\x%02x`, c)
				default:
					b.WriteByte(c)
				}
			}
			name = b.String() + suffix
			if len(name) >= 256 {
				var hash [8]byte
				binary.LittleEndian.PutUint64(hash[:], journal.SipHash24(longUnitNameHashKey, append([]byte(name), 0)))
				name = name[:256-1-len(suffix)-16-1] + "_" + hex.EncodeToString(hash[:]) + suffix
			}
			return name, "", nil
		}
	}
	var b strings.Builder
	for i := 0; i < len(value); i++ {
		c := value[i]
		switch {
		case c == '/':
			b.WriteByte('-')
		case !strings.ContainsRune(unitNameChars+"[]!*?", rune(c)):
			fmt.Fprintf(&b, `\x%02x`, c)
		default:
			b.WriteByte(c)
		}
	}
	name = b.String()
	if name != value {
		escaped = name
	}
	if !isGlobPattern(name) && !validUnitName(name) {
		name += ".service"
	}
	return name, escaped, nil
}

func containsString(values []string, value string) bool {
	for _, candidate := range values {
		if candidate == value {
			return true
		}
	}
	return false
}

// longUnitNameHashKey is systemd's LONG_UNIT_NAME_HASH_KEY (src/basic/unit-name.c).
var longUnitNameHashKey = [16]byte{0xec, 0xf2, 0x37, 0xfb, 0x58, 0x32, 0x4a, 0x32, 0x84, 0x9f, 0x06, 0x9b, 0x0d, 0x21, 0xeb, 0x9a}

func validUnitPathComponents(parts []string) bool {
	for _, part := range parts {
		if len(part) > 255 {
			return false
		}
	}
	return true
}
