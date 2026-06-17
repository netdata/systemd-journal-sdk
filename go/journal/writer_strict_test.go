package journal

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestCreateRequiresMachineAndBootID(t *testing.T) {
	path := filepath.Join(t.TempDir(), "strict.journal")
	if _, err := Create(path, Options{}); !errors.Is(err, ErrMissingMachineID) {
		t.Fatalf("Create(empty options) error = %v, want ErrMissingMachineID", err)
	}
	if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("Create(empty options) created file or unexpected stat error: %v", err)
	}

	opts := testOptions()
	opts.BootID = UUID{}
	if _, err := Create(path, opts); !errors.Is(err, ErrMissingBootID) {
		t.Fatalf("Create(missing boot id) error = %v, want ErrMissingBootID", err)
	}
}

func TestAppendRequiresMonotonicUsec(t *testing.T) {
	path := filepath.Join(t.TempDir(), "strict-append.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	defer w.Close()

	if err := w.Append([]Field{StringField("MESSAGE", "missing monotonic")}, EntryOptions{}); !errors.Is(err, ErrMissingMonotonicUsec) {
		t.Fatalf("Append(missing monotonic) error = %v, want ErrMissingMonotonicUsec", err)
	}
	if err := w.AppendMap(map[string]string{"MESSAGE": "missing monotonic"}); !errors.Is(err, ErrMissingMonotonicUsec) {
		t.Fatalf("AppendMap(missing monotonic) error = %v, want ErrMissingMonotonicUsec", err)
	}
}

func TestOpenWithOptionsRejectsPartialIdentity(t *testing.T) {
	path := filepath.Join(t.TempDir(), "strict-open.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	opts := Options{MachineID: testMachineID}
	if _, err := OpenWithOptions(path, opts); !errors.Is(err, ErrMissingBootID) {
		t.Fatalf("OpenWithOptions(partial identity) error = %v, want ErrMissingBootID", err)
	}
}

func TestOpenEmptyFileRequiresBootIDBeforeAppend(t *testing.T) {
	path := filepath.Join(t.TempDir(), "strict-empty-open.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	w, err = Open(path)
	if err != nil {
		t.Fatalf("Open(empty file) error = %v", err)
	}
	defer w.Close()

	err = w.Append([]Field{StringField("MESSAGE", "missing boot")}, EntryOptions{
		MonotonicUsec:    1,
		MonotonicUsecSet: true,
	})
	if !errors.Is(err, ErrMissingBootID) {
		t.Fatalf("Append(empty file without boot id) error = %v, want ErrMissingBootID", err)
	}
	if got := w.header.nEntries; got != 0 {
		t.Fatalf("Append(empty file without boot id) wrote %d entries, want 0", got)
	}
}

func TestOpenEmptyFileAllowsExplicitEntryBootID(t *testing.T) {
	path := filepath.Join(t.TempDir(), "strict-empty-open-entry-boot.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	w, err = Open(path)
	if err != nil {
		t.Fatalf("Open(empty file) error = %v", err)
	}
	defer w.Close()

	if err := w.Append([]Field{StringField("MESSAGE", "explicit boot")}, EntryOptions{
		BootID:           testBootID,
		MonotonicUsec:    1,
		MonotonicUsecSet: true,
	}); err != nil {
		t.Fatalf("Append(empty file with explicit entry boot id) error = %v", err)
	}
}
