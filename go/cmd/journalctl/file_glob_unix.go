//go:build cgo && (linux || darwin || freebsd)

package main

/*
#define _GNU_SOURCE
#include <glob.h>
#include <locale.h>
#include <stdlib.h>

static int journal_file_glob(const char *pattern, glob_t *paths) {
    locale_t selected = newlocale(LC_ALL_MASK, "", (locale_t) 0);
    if (!selected)
        selected = newlocale(LC_ALL_MASK, "C", (locale_t) 0);
    if (!selected)
        return GLOB_NOSPACE;
    locale_t previous = uselocale(selected);
    if (!previous) {
        freelocale(selected);
        return GLOB_ABORTED;
    }
    int result = glob(pattern, GLOB_BRACE | GLOB_NOCHECK | GLOB_NOSORT, NULL, paths);
    uselocale(previous);
    freelocale(selected);
    return result;
}
*/
import "C"

import (
	"fmt"
	"strings"
	"unsafe"
)

func globFilePaths(pattern string) ([]string, error) {
	// NOCHECK leaves an unescaped literal unchanged, including a missing path.
	if !strings.ContainsAny(pattern, "*?[]{}\\") {
		return nil, nil
	}
	if strings.IndexByte(pattern, 0) >= 0 {
		return nil, fmt.Errorf("invalid NUL in file pattern")
	}
	value := C.CString(pattern)
	defer C.free(unsafe.Pointer(value))
	var matches C.glob_t
	status := C.journal_file_glob(value, &matches)
	defer C.globfree(&matches)
	if status != 0 {
		return nil, fmt.Errorf("file pattern expansion failed (%d)", int(status))
	}
	if uint64(matches.gl_pathc) > uint64(^uint(0)>>1) {
		return nil, fmt.Errorf("too many file pattern matches")
	}
	paths := make([]string, 0, int(matches.gl_pathc))
	for _, path := range unsafe.Slice(matches.gl_pathv, int(matches.gl_pathc)) {
		paths = append(paths, C.GoString(path))
	}
	return paths, nil
}
