//go:build cgo && !linux && !darwin && !freebsd

package main

import (
	"fmt"
	"strings"
	"unicode/utf8"
)

func globFilePaths(pattern string) ([]string, error) {
	if !utf8.ValidString(pattern) {
		return nil, fmt.Errorf("file patterns require Unicode filenames on this platform")
	}
	if !strings.ContainsAny(pattern, "*?[]{}") {
		return nil, nil
	}
	var paths []string
	for _, alternative := range expandFileBraces(pattern) {
		matches, err := globUnicodePaths(alternative)
		if err != nil {
			return nil, err
		}
		paths = append(paths, matches...)
	}
	return paths, nil
}
