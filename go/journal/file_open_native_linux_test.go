//go:build linux

package journal

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

func TestNativeReaderFileDescriptor(t *testing.T) {
	path := filepath.Join(t.TempDir(), "native-descriptor")
	want := []byte("positioned journal bytes")
	if err := os.WriteFile(path, want, 0o600); err != nil {
		t.Fatal(err)
	}
	f, err := openNativeReaderFile(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = f.Close() })
	flags, _, errno := syscall.Syscall(syscall.SYS_FCNTL, f.Fd(), syscall.F_GETFL, 0)
	if errno != 0 {
		t.Fatal(errno)
	}
	if flags&syscall.O_ACCMODE != syscall.O_RDONLY || flags&syscall.O_NONBLOCK != 0 {
		t.Fatalf("descriptor flags = %#x; want read-only, blocking", flags)
	}
	flags, _, errno = syscall.Syscall(syscall.SYS_FCNTL, f.Fd(), syscall.F_GETFD, 0)
	if errno != 0 || flags&syscall.FD_CLOEXEC == 0 {
		t.Fatalf("descriptor inheritance flags = %#x, %v; want close-on-exec", flags, errno)
	}
	got := make([]byte, len(want))
	if n, err := f.ReadAt(got, 0); err != nil || n != len(got) || !bytes.Equal(got, want) {
		t.Fatalf("ReadAt = %d, %v, %q; want %q", n, err, got, want)
	}
	if n, err := f.WriteAt([]byte("x"), 0); n != 0 || !errors.Is(err, syscall.EBADF) {
		t.Fatalf("WriteAt = %d, %v; want read-only descriptor error", n, err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := f.ReadAt(got, 0); !errors.Is(err, os.ErrClosed) {
		t.Fatalf("ReadAt after Close = %v; want closed file", err)
	}
}

func TestNativeReaderFilePaths(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "native-path")
	if err := os.WriteFile(path, []byte("native bytes"), 0o600); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(dir, "native-link")
	if err := os.Symlink(path, link); err != nil {
		t.Fatal(err)
	}
	bytePath := filepath.Join(dir, "native-\xff")
	if err := os.WriteFile(bytePath, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	for name, input := range map[string]string{"regular": path, "symlink": link, "directory": dir, "byte-name": bytePath} {
		t.Run(name, func(t *testing.T) {
			f, err := openNativeReaderFile(input)
			if err != nil {
				t.Fatal(err)
			}
			defer func() { _ = f.Close() }()
			got, err := f.Stat()
			if err != nil {
				t.Fatal(err)
			}
			want, err := os.Stat(input)
			if err != nil {
				t.Fatal(err)
			}
			if !os.SameFile(got, want) || got.Mode() != want.Mode() || f.Name() != input {
				t.Fatalf("descriptor identity/name differs from requested path")
			}
		})
	}
}

func TestNativeReaderFileErrors(t *testing.T) {
	dir := t.TempDir()
	file := filepath.Join(dir, "not-directory")
	if err := os.WriteFile(file, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	broken := filepath.Join(dir, "broken-link")
	if err := os.Symlink(filepath.Join(dir, "missing"), broken); err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct {
		name, path string
		err        error
	}{
		{"missing", filepath.Join(dir, "missing"), syscall.ENOENT},
		{"empty", "", syscall.ENOENT},
		{"not-directory", filepath.Join(file, "child"), syscall.ENOTDIR},
		{"nul", filepath.Join(dir, "invalid\x00name"), syscall.EINVAL},
		{"broken-link", broken, syscall.ENOENT},
	} {
		t.Run(test.name, func(t *testing.T) {
			f, err := openNativeReaderFile(test.path)
			if f != nil {
				_ = f.Close()
				t.Fatal("failed open returned a file")
			}
			var pathErr *os.PathError
			if !errors.As(err, &pathErr) || pathErr.Op != "open" || pathErr.Path != test.path || !errors.Is(err, test.err) {
				t.Fatalf("open error = %#v; want typed path error matching %v", err, test.err)
			}
		})
	}
}

func TestNativeReaderFileLargeOffset(t *testing.T) {
	path := filepath.Join(t.TempDir(), "native-sparse")
	const offset int64 = 1<<31 + 4096
	want := []byte("large-offset bytes")
	writer, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := writer.WriteAt(want, offset); err != nil {
		_ = writer.Close()
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	f, err := openNativeReaderFile(path)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = f.Close() }()
	st, err := f.Stat()
	if err != nil || st.Size() != offset+int64(len(want)) {
		t.Fatalf("large file Stat = %v, %v", st, err)
	}
	got := make([]byte, len(want))
	if n, err := f.ReadAt(got, offset); err != nil || n != len(got) || !bytes.Equal(got, want) {
		t.Fatalf("large-offset ReadAt = %d, %v, %q", n, err, got)
	}
}
