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
