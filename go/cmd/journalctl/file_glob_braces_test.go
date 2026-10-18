//go:build cgo

package main

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestFileBraceAlternatives(t *testing.T) {
	for pattern, expected := range map[string][]string{
		"{a,{b,c}}-é.journal": {"a-é.journal", "b-é.journal", "c-é.journal"},
		"{plain}.journal":     {"{plain}.journal"},
		"{{a,b}}.journal":     {"{a}.journal", "{b}.journal"},
		"{a,}?.journal":       {"a?.journal", "?.journal"},
	} {
		if actual := expandFileBraces(pattern); !reflect.DeepEqual(actual, expected) {
			t.Fatalf("%q: got %q, want %q", pattern, actual, expected)
		}
	}
}

func TestUnicodeFilePatterns(t *testing.T) {
	directory := t.TempDir()
	for _, name := range []string{"é.journal", ".hidden.journal", "literal-[x.journal"} {
		if err := os.WriteFile(filepath.Join(directory, name), nil, 0o600); err != nil {
			t.Fatal(err)
		}
	}
	for pattern, names := range map[string][]string{
		"?.journal": {"é.journal"}, "[!a]*.journal": {"literal-[x.journal", "é.journal"},
		".*.journal": {".hidden.journal"}, "literal-[x.journal": {"literal-[x.journal"},
	} {
		var expected []string
		for _, name := range names {
			expected = append(expected, filepath.Join(directory, name))
		}
		actual, err := globUnicodePaths(filepath.Join(directory, pattern))
		if err != nil || !reflect.DeepEqual(actual, expected) {
			t.Fatalf("%q: got %q (%v), want %q", pattern, actual, err, expected)
		}
	}
}
