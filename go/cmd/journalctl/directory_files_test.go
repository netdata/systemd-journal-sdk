package main

import (
	"bytes"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

// --verify --directory must check exactly the files a directory read sees:
// top-level journals plus one level of machine-id subdirectories.
func TestRunVerifyDirectoryUsesSystemdLayout(t *testing.T) {
	source := writeCLIJournal(t, []cliEntry{{message: "verify-layout", priority: "6"}})
	root := t.TempDir()
	const plainID = "00112233445566778899aabbccddeeff"
	const dashedID = "00112233-4455-6677-8899-aabbccddeeff"
	for _, dir := range []string{plainID, dashedID, "not-an-id", filepath.Join(plainID, "nested")} {
		if err := os.MkdirAll(filepath.Join(root, dir), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	var want []string
	for _, rel := range []string{
		"system.journal",
		"system@archived.journal~",
		filepath.Join(plainID, "user-1000.journal"),
		filepath.Join(dashedID, "system.journal"),
	} {
		path := filepath.Join(root, rel)
		copyFile(t, source, path)
		want = append(want, path)
	}
	for _, rel := range []string{
		filepath.Join("not-an-id", "ignored.journal"),
		filepath.Join(plainID, "nested", "too-deep.journal"),
	} {
		copyFile(t, source, filepath.Join(root, rel))
	}
	if err := os.WriteFile(filepath.Join(root, "notes.log"), []byte("not a journal"), 0o644); err != nil {
		t.Fatal(err)
	}

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify", "--directory", root}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --verify --directory: %v; stderr=%s", err, stderr.String())
	}
	var got []string
	for _, line := range strings.Split(strings.TrimSpace(stderr.String()), "\n") {
		if path, ok := strings.CutPrefix(line, "PASS: "); ok {
			got = append(got, path)
		} else {
			t.Fatalf("unexpected stderr line %q", line)
		}
	}
	sort.Strings(got)
	sort.Strings(want)
	if strings.Join(got, "\n") != strings.Join(want, "\n") {
		t.Fatalf("verified files:\n got %q\nwant %q", got, want)
	}
}
