//go:build cgo

package main

import (
	"bytes"
	"testing"
)

func TestTablePrettyJSONNativeSpacing(t *testing.T) {
	t.Setenv("SYSTEMD_COLORS", "0")
	var out bytes.Buffer
	if err := writeTableJSON(&out, "json-pretty", []map[string]any{{"index": 0, "label": "a:b"}}); err != nil {
		t.Fatal(err)
	}
	want := "[\n\t{\n\t\t\"index\" : 0,\n\t\t\"label\" : \"a:b\"\n\t}\n]\n"
	if out.String() != want {
		t.Fatalf("got %q, want %q", out.String(), want)
	}
}

// Paired with Rust table_index_width_follows_widest_index.
func TestTableIndexWidthFollowsWidestIndex(t *testing.T) {
	for _, tc := range []struct {
		indexes []int
		quiet   bool
		want    int
	}{
		{[]int{-5, 0}, false, 3},
		{[]int{-5, 0}, true, 2},
		{[]int{-220, 0}, false, 4},
		{nil, true, 1},
	} {
		if got := tableIndexWidth(tc.indexes, tc.quiet); got != tc.want {
			t.Fatalf("tableIndexWidth(%v, %v) = %d, want %d", tc.indexes, tc.quiet, got, tc.want)
		}
	}
	header := outputColors{}.tableHeader("INVOCATION ID", 4)
	if want := "IDX  INVOCATION ID "; header[:len(want)] != want {
		t.Fatalf("header = %q", header)
	}
}
