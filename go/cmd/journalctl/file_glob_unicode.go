//go:build cgo

package main

import (
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The fallback uses the same Unicode grammar as Rust, without libc glob.
func globUnicodePaths(pattern string) ([]string, error) {
	volume := filepath.VolumeName(pattern)
	rest := pattern[len(volume):]
	prefix := volume
	if len(rest) > 0 && os.IsPathSeparator(rest[0]) {
		prefix += string(filepath.Separator)
	}
	prefixes := []string{prefix}
	for _, part := range strings.FieldsFunc(rest, func(ch rune) bool { return ch < 128 && os.IsPathSeparator(uint8(ch)) }) {
		if !isGlobPattern(part) {
			for i, prefix := range prefixes {
				prefixes[i] = joinGlobComponent(prefix, part, volume)
			}
			continue
		}
		matcher, err := regexp.Compile(globPatternToRegex(part))
		if err != nil {
			return nil, err
		}
		var next []string
		for _, prefix := range prefixes {
			directory := prefix
			if directory == "" {
				directory = "."
			}
			entries, err := os.ReadDir(directory)
			if err != nil {
				continue
			}
			for _, entry := range entries {
				name := entry.Name()
				if strings.HasPrefix(name, ".") && !strings.HasPrefix(part, ".") {
					continue
				}
				if matcher.MatchString(name) {
					next = append(next, joinGlobComponent(prefix, name, volume))
				}
			}
		}
		prefixes = next
	}
	paths := prefixes[:0]
	for _, path := range prefixes {
		if _, err := os.Stat(path); err == nil {
			paths = append(paths, path)
		}
	}
	sort.Strings(paths)
	return paths, nil
}

func joinGlobComponent(prefix, component, volume string) string {
	// Preserve relative drive paths and parent components for OS resolution.
	if prefix == "" || os.IsPathSeparator(prefix[len(prefix)-1]) || prefix == volume && strings.HasSuffix(volume, ":") {
		return prefix + component
	}
	return prefix + string(filepath.Separator) + component
}
