package main

import (
	"strings"
	"testing"
)

// Unit names of 256 bytes or more end in a keyed SipHash-2-4 of the full
// name, as systemd's unit_name_hash_long (src/basic/unit-name.c) does; pin
// one result.
func TestNormalizeUnitNameHashesLongPaths(t *testing.T) {
	path := "/" + strings.Repeat("segment-name/", 30) + "end"
	name, escaped, err := normalizeUnitName(path)
	if err != nil {
		t.Fatal(err)
	}
	want := strings.Repeat(`segment\x2dname-`, 14) + `segment\_d7b9c885996cfc3e.mount`
	if name != want || escaped != "" {
		t.Fatalf("name = %q escaped = %q, want %q", name, escaped, want)
	}
}
