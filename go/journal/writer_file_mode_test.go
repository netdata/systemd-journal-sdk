package journal

import (
	"os"
	"path/filepath"
	"testing"
)

func TestCreateFileModeDefaultAndOverride(t *testing.T) {
	if os.PathSeparator == '\\' {
		t.Skip("POSIX file modes are not enforced on Windows")
	}

	defaultPath := filepath.Join(t.TempDir(), "default-mode.journal")
	defaultWriter, err := Create(defaultPath, testOptions())
	if err != nil {
		t.Fatalf("Create(default) error = %v", err)
	}
	closeWriterForTest(t, defaultWriter, "default mode")
	assertFileMode(t, defaultPath, defaultJournalFileMode)

	overridePath := filepath.Join(t.TempDir(), "override-mode.journal")
	opts := testOptions()
	opts.FileMode = JournalFileMode(0o600)
	overrideWriter, err := Create(overridePath, opts)
	if err != nil {
		t.Fatalf("Create(override) error = %v", err)
	}
	closeWriterForTest(t, overrideWriter, "override mode")
	assertFileMode(t, overridePath, 0o600)
}

func assertFileMode(t *testing.T, path string, want os.FileMode) {
	t.Helper()
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat %s: %v", path, err)
	}
	if got := info.Mode().Perm(); got != want {
		t.Fatalf("mode %s = %#o, want %#o", path, got, want)
	}
}
