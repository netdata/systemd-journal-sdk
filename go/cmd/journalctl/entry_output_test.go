package main

import (
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// Export and JSON must render as systemd v262 does (goldens in tests/output).
func TestRunEntryOutputMatchesSystemd(t *testing.T) {
	dir := filepath.Join("..", "..", "..", "tests", "output")
	file := filepath.Join(dir, "entry-output.journal.zst")
	fields := "--output-fields=MESSAGE,MULTI,BIN,__CURSOR,_BOOT_ID,AFTER,LAST"
	for _, tc := range []struct {
		golden string
		args   []string
		json   bool
	}{
		{"export.golden", []string{"-o", "export"}, false},
		{"export-fields.golden", []string{"-o", "export", fields}, false},
		{"json.golden", []string{"-o", "json"}, true},
		{"json-all.golden", []string{"-o", "json", "--all"}, true},
		{"json-fields.golden", []string{"-o", "json", fields}, true},
		{"json-pretty.golden", []string{"-o", "json-pretty"}, true},
	} {
		var stdout, stderr bytes.Buffer
		args := append([]string{"--file", file}, tc.args...)
		if err := run(args, strings.NewReader(""), &stdout, &stderr); err != nil {
			t.Fatalf("%v: %v; stderr=%s", tc.args, err, stderr.String())
		}
		want, err := os.ReadFile(filepath.Join(dir, tc.golden))
		if err != nil {
			t.Fatal(err)
		}
		if !tc.json {
			if !bytes.Equal(stdout.Bytes(), want) {
				t.Errorf("%v: output differs from %s", tc.args, tc.golden)
			}
			continue
		}
		// systemd orders keys by hash table iteration, so compare values.
		if got, want := decodeCLIJSON(t, stdout.Bytes()), decodeCLIJSON(t, want); !reflect.DeepEqual(got, want) {
			t.Errorf("%v: JSON differs from %s\n got: %v\nwant: %v", tc.args, tc.golden, got, want)
		}
	}
}

func decodeCLIJSON(t *testing.T, data []byte) []map[string]any {
	t.Helper()
	var out []map[string]any
	decoder := json.NewDecoder(bytes.NewReader(data))
	for {
		var object map[string]any
		if err := decoder.Decode(&object); err == io.EOF {
			return out
		} else if err != nil {
			t.Fatalf("decode JSON: %v", err)
		}
		out = append(out, object)
	}
}

// systemd's printable rule treats Unicode noncharacters as binary, so the
// short family prints them as blob data.
func TestPrepareShortMessageNoncharactersAreNotPrintable(t *testing.T) {
	if _, printable := prepareShortMessage([]byte("x﷐y"), true); printable {
		t.Fatal("U+FDD0 message reported printable")
	}
	if _, printable := prepareShortMessage([]byte("x﷐y\tz"), true); printable {
		t.Fatal("U+FDD0 message with a tab reported printable")
	}
	if prefix, printable := prepareShortMessage([]byte("ok\n﷐"), true); string(prefix) != "ok" || !printable {
		t.Fatalf("truncated prefix = %q, %v; want \"ok\", true", prefix, printable)
	}
}
