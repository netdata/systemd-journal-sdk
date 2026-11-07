package journal

import (
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"testing"
)

// directoryFilesLayout builds the systemd directory layout cases the listing
// must accept or ignore, and returns the root and the expected sorted files.
func directoryFilesLayout(t *testing.T) (string, []string) {
	t.Helper()
	root := t.TempDir()
	const plainID = "00112233445566778899aabbccddeeff"
	const dashedID = "00112233-4455-6677-8899-AABBCCDDEEFF"
	mkdir := func(rel string) {
		if err := os.MkdirAll(filepath.Join(root, rel), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	touch := func(rel string) string {
		path := filepath.Join(root, rel)
		if err := os.WriteFile(path, nil, 0o644); err != nil {
			t.Fatal(err)
		}
		return path
	}
	mkdir(plainID)
	mkdir(dashedID)
	mkdir("not-an-id")
	mkdir("00112233445566778899aabbccddeef")  // 31 characters
	mkdir("0011223344556677.899aabbccddeeff") // contains a dot
	mkdir(filepath.Join(plainID, "nested"))
	mkdir("dir.journal")

	want := []string{
		touch("system.journal"),
		touch("system@archived.journal~"),
		touch("whole.journal.zst"),
		touch("whole.journal~.zst"),
		touch(filepath.Join(plainID, "user-1000.journal")),
		touch(filepath.Join(dashedID, "system.journal")),
	}
	linkTarget := touch("target.bin")
	if err := os.Symlink(linkTarget, filepath.Join(root, "linked.journal")); err != nil {
		t.Fatal(err)
	}
	want = append(want, filepath.Join(root, "linked.journal"))

	touch("notes.log")
	touch("system.journal.bak")
	touch(filepath.Join("not-an-id", "ignored.journal"))
	touch(filepath.Join("00112233445566778899aabbccddeef", "ignored.journal"))
	touch(filepath.Join("0011223344556677.899aabbccddeeff", "ignored.journal"))
	touch(filepath.Join(plainID, "nested", "too-deep.journal"))
	touch(filepath.Join(plainID, "notes.log"))

	sort.Strings(want)
	return root, want
}

func TestDirectoryFilesFollowsSystemdLayout(t *testing.T) {
	root, want := directoryFilesLayout(t)
	got, err := DirectoryFiles(root)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("files:\n got %q\nwant %q", got, want)
	}
}

func TestDirectoryFilesMissingDirectoryFails(t *testing.T) {
	if _, err := DirectoryFiles(filepath.Join(t.TempDir(), "missing")); err == nil {
		t.Fatal("expected an error for a missing directory")
	}
}

func TestUniqueFilesKeepsFirstSpellingOfEachFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "real.journal")
	other := filepath.Join(dir, "other.journal")
	for _, p := range []string{path, other} {
		if err := os.WriteFile(p, nil, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	link := filepath.Join(dir, "link.journal")
	if err := os.Symlink(path, link); err != nil {
		t.Fatal(err)
	}
	dotted := filepath.Join(dir, ".") + string(filepath.Separator) + "real.journal"
	missing := filepath.Join(dir, "missing.journal")

	got := UniqueFiles([]string{link, path, dotted, other, missing, missing})
	want := []string{link, other, missing}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("UniqueFiles = %q, want %q", got, want)
	}
}
