//go:build cgo

package main

import (
	"regexp"
	"unicode/utf8"
)

type byteUnitMatcher struct {
	patterns []*regexp.Regexp
	buffer   []byte
}

func newByteUnitMatcher(patterns []string) (*byteUnitMatcher, error) {
	m := &byteUnitMatcher{}
	for _, pattern := range patterns {
		re, err := regexp.Compile(globPatternToRegex(pattern))
		if err != nil {
			continue
		} // An invalid bracket expression cannot match.
		m.patterns = append(m.patterns, re)
	}
	return m, nil
}

func (m *byteUnitMatcher) Matches(value []byte) bool {
	for _, c := range value {
		if c < 128 {
			continue
		}
		// Unit patterns are mangled to ASCII. Map each payload byte to one
		// rune so Go's UTF-8 regexp preserves native C-locale byte matching.
		m.buffer = m.buffer[:0]
		for _, b := range value {
			m.buffer = utf8.AppendRune(m.buffer, rune(b))
		}
		value = m.buffer
		break
	}
	for _, pattern := range m.patterns {
		if pattern.Match(value) {
			return true
		}
	}
	return false
}

func (*byteUnitMatcher) Close() {}
