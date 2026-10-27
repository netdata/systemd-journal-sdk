//go:build cgo && (linux || darwin || freebsd)

package main

/*
#define _GNU_SOURCE
#include <fnmatch.h>
#include <locale.h>
#include <stdlib.h>
#include "unit_pattern_compat.h"

static struct journal_unit_pattern *unit_pattern_prepare(locale_t selected, const char *pattern) {
    locale_t previous = uselocale(selected);
    if (!previous) return NULL;
    struct journal_unit_pattern *p = journal_unit_pattern_new(pattern);
    uselocale(previous);
    return p;
}
static int unit_pattern_compat_matches(locale_t selected, char **patterns, struct journal_unit_pattern **compat, size_t count, const char *value) {
    locale_t previous = uselocale(selected);
    if (!previous) return 0;
    int matched = 0;
    for (size_t i = 0; i < count; i++)
        if (compat[i] ? journal_unit_pattern_match(compat[i], value) : fnmatch(patterns[i], value, FNM_NOESCAPE) == 0) {
            matched = 1;
            break;
        }
    uselocale(previous);
    return matched;
}

static locale_t unit_pattern_locale(void) {
    locale_t selected = newlocale(LC_ALL_MASK, "", (locale_t) 0);
    return selected ? selected : newlocale(LC_ALL_MASK, "C", (locale_t) 0);
}
static int unit_pattern_matches(locale_t selected, char **patterns, size_t count, const char *value) {
    locale_t previous = uselocale(selected);
    if (!previous) return 0;
    int matched = 0;
    for (size_t i = 0; i < count; i++)
        if (fnmatch(patterns[i], value, FNM_NOESCAPE) == 0) {
            matched = 1;
            break;
        }
    uselocale(previous);
    return matched;
}
*/
import "C"

import (
	"fmt"
	"runtime"
	"strings"
	"unsafe"
)

type unitPatternMatcher struct {
	locale   C.locale_t
	patterns **C.char
	count    int
	buffer   []byte
	compat   **C.struct_journal_unit_pattern
}

func newUnitPatternMatcher(patterns []string) (*unitPatternMatcher, error) {
	return newUnitPatternMatcherWithCompatibility(patterns, runtime.GOOS == "freebsd")
}

func newUnitPatternMatcherWithCompatibility(patterns []string, compatibility bool) (*unitPatternMatcher, error) {
	m := &unitPatternMatcher{locale: C.unit_pattern_locale(), count: len(patterns)}
	if m.locale == nil {
		return nil, fmt.Errorf("failed to initialize unit pattern locale")
	}
	m.patterns = (**C.char)(C.calloc(C.size_t(len(patterns)), C.size_t(unsafe.Sizeof((*C.char)(nil)))))
	if m.patterns == nil {
		m.Close()
		return nil, fmt.Errorf("failed to allocate unit patterns")
	}
	if compatibility {
		compatibility = false
		for _, pattern := range patterns {
			if strings.Contains(pattern, "[:") || strings.Contains(pattern, "[.") || strings.Contains(pattern, "[=") {
				compatibility = true
				break
			}
		}
	}
	if compatibility {
		m.compat = (**C.struct_journal_unit_pattern)(C.calloc(C.size_t(len(patterns)), C.size_t(unsafe.Sizeof((*C.struct_journal_unit_pattern)(nil)))))
		if m.compat == nil {
			m.Close()
			return nil, fmt.Errorf("failed to allocate unit pattern compatibility state")
		}
	}
	for i, pattern := range patterns {
		p := C.CString(pattern)
		if p == nil {
			m.Close()
			return nil, fmt.Errorf("failed to allocate unit pattern")
		}
		unsafe.Slice(m.patterns, m.count)[i] = p
		if compatibility && C.journal_unit_pattern_needed(p) != 0 {
			c := C.unit_pattern_prepare(m.locale, p)
			if c == nil {
				m.Close()
				return nil, fmt.Errorf("failed to prepare unit pattern")
			}
			unsafe.Slice(m.compat, m.count)[i] = c
		}
	}
	return m, nil
}

func (m *unitPatternMatcher) Matches(value []byte) bool {
	m.buffer = append(m.buffer[:0], value...)
	m.buffer = append(m.buffer, 0)
	if m.compat != nil {
		return C.unit_pattern_compat_matches(m.locale, m.patterns, m.compat, C.size_t(m.count), (*C.char)(unsafe.Pointer(&m.buffer[0]))) != 0
	}
	return C.unit_pattern_matches(m.locale, m.patterns, C.size_t(m.count), (*C.char)(unsafe.Pointer(&m.buffer[0]))) != 0
}

func (m *unitPatternMatcher) Close() {
	if m.compat != nil {
		for _, p := range unsafe.Slice(m.compat, m.count) {
			C.journal_unit_pattern_free(p)
		}
		C.free(unsafe.Pointer(m.compat))
		m.compat = nil
	}
	if m.patterns != nil {
		for _, pattern := range unsafe.Slice(m.patterns, m.count) {
			C.free(unsafe.Pointer(pattern))
		}
		C.free(unsafe.Pointer(m.patterns))
		m.patterns = nil
	}
	if m.locale != nil {
		C.freelocale(m.locale)
		m.locale = nil
	}
}
