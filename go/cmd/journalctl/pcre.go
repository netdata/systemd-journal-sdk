//go:build cgo

package main

/*
#cgo pkg-config: libpcre2-8
#define PCRE2_CODE_UNIT_WIDTH 8
#include <pcre2.h>
#if PCRE2_MAJOR < 10 || (PCRE2_MAJOR == 10 && PCRE2_MINOR < 42)
#error journalctl requires PCRE2 10.42 or later
#endif

// Match data must die before returning: PCRE2 retains a borrowed subject pointer.
static int journal_pattern_match(pcre2_code *code, const unsigned char *subject, size_t size, size_t *range) {
    static const unsigned char empty = 0;
    pcre2_match_data *md = pcre2_match_data_create(1, NULL);
    if (!md) return PCRE2_ERROR_NOMEMORY;
    int rc = pcre2_match(code, subject ? subject : &empty, size, 0, 0, md, NULL);
    if (rc >= 0 && range) {
        PCRE2_SIZE *offsets = pcre2_get_ovector_pointer(md);
        range[0] = offsets[0];
        range[1] = offsets[1];
    }
    pcre2_match_data_free(md);
    return rc;
}

static pcre2_code *journal_pattern_compile(const unsigned char *pattern, size_t size, uint32_t flags,
                                           int *error, size_t *offset) {
    static const unsigned char empty = 0;
    return pcre2_compile(pattern ? pattern : &empty, size, flags, error, offset, NULL);
}
*/
import "C"

import (
	"fmt"
	"runtime"
	"unsafe"
)

type pcrePattern struct{ code *C.pcre2_code }

func pcreVersion() string {
	var buf [128]byte
	if C.pcre2_config(C.PCRE2_CONFIG_VERSION, unsafe.Pointer(&buf[0])) < 0 {
		return "unknown"
	}
	for i, b := range buf {
		if b == 0 {
			return string(buf[:i])
		}
	}
	return string(buf[:])
}

func pcreError(code C.int) string {
	var buf [512]byte
	n := C.pcre2_get_error_message(code, (*C.uchar)(unsafe.Pointer(&buf[0])), C.size_t(len(buf)))
	if n < 0 {
		return fmt.Sprintf("PCRE2 error %d", int(code))
	}
	return string(buf[:int(n)])
}

func compilePCRE(pattern []byte, sensitive bool) (*pcrePattern, error) {
	var ptr *C.uchar
	if len(pattern) > 0 {
		ptr = (*C.uchar)(unsafe.Pointer(&pattern[0]))
	}
	var code C.int
	var offset C.size_t
	var options C.uint32_t
	if !sensitive {
		options = C.PCRE2_CASELESS
	}
	compiled := C.journal_pattern_compile(ptr, C.size_t(len(pattern)), options, &code, &offset)
	runtime.KeepAlive(pattern)
	if compiled == nil {
		return nil, fmt.Errorf("Bad pattern: %s (offset %d)", pcreError(code), uint64(offset))
	}
	return &pcrePattern{code: compiled}, nil
}

func (p *pcrePattern) Close() {
	if p != nil && p.code != nil {
		C.pcre2_code_free(p.code)
		p.code = nil
	}
}

func (p *pcrePattern) Match(subject []byte) (bool, error) {
	return p.match(subject, nil)
}

func (p *pcrePattern) match(subject []byte, highlight *matchRange) (bool, error) {
	var ptr *C.uchar
	if len(subject) > 0 {
		ptr = (*C.uchar)(unsafe.Pointer(&subject[0]))
	}
	var rc C.int
	if highlight == nil {
		rc = C.journal_pattern_match(p.code, ptr, C.size_t(len(subject)), nil)
	} else {
		var offsets [2]C.size_t
		rc = C.journal_pattern_match(p.code, ptr, C.size_t(len(subject)), &offsets[0])
		if rc >= 0 {
			*highlight = matchRange{start: int(offsets[0]), end: int(offsets[1])}
		}
	}
	runtime.KeepAlive(subject)
	runtime.KeepAlive(p)
	if rc == C.PCRE2_ERROR_NOMATCH {
		return false, nil
	}
	if rc < 0 {
		return false, fmt.Errorf("Pattern matching failed: %s", pcreError(rc))
	}
	return true, nil
}

func compileGrepFilter(pattern string, caseSensitive optionalStringFlag) (*pcrePattern, error) {
	var sensitive bool
	if caseSensitive.set {
		parsed, err := parseBoolOption("--case-sensitive", caseSensitive.value)
		if err != nil {
			return nil, err
		}
		sensitive = parsed
	} else {
		upper, err := compilePCRE([]byte("[[:upper:]]"), true)
		if err != nil {
			return nil, err
		}
		sensitive, err = upper.Match([]byte(pattern))
		upper.Close()
		if err != nil {
			return nil, err
		}
	}
	return compilePCRE([]byte(pattern), sensitive)
}
