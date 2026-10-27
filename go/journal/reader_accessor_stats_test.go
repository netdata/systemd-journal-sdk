package journal

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestReaderAccessorInitialStats(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessAuto, ReaderAccessMmap, ReaderAccessReadAt} {
		for _, size := range []int{0, 256} {
			t.Run(fmt.Sprintf("mode=%d/size=%d", mode, size), func(t *testing.T) {
				path := filepath.Join(t.TempDir(), "initial-stats")
				if err := os.WriteFile(path, make([]byte, size), 0o600); err != nil {
					t.Fatal(err)
				}
				file, err := os.Open(path)
				if err != nil {
					t.Fatal(err)
				}
				defer func() { _ = file.Close() }()
				opts := DefaultReaderOptions().WithAccessMode(mode).WithWindowSize(64).WithMaxWindows(2).WithSnapshot(true)
				accessor, got, err := newReaderAccessor(file, opts)
				emptyWindowsMapping := runtime.GOOS == "windows" && size == 0 && mode != ReaderAccessReadAt
				if emptyWindowsMapping && mode == ReaderAccessMmap {
					if accessor != nil {
						_ = accessor.close()
					}
					if err == nil || accessor != nil {
						t.Fatal("empty Windows file must reject explicit mmap construction")
					}
					return
				}
				if err != nil {
					t.Fatal(err)
				}
				defer func() { _ = accessor.close() }()
				selected := ReaderAccessMmap
				if mode == ReaderAccessReadAt || emptyWindowsMapping {
					selected = ReaderAccessReadAt
				}
				if got != accessor.stats() || got.RequestedAccessMode != mode || got.FileSize != uint64(size) ||
					got.WindowSize != 64 || got.MaxWindows != 2 || got.RowPinnedWindows != 0 ||
					got.RowArenaBytes != 0 || got.RowArenaLimitBytes != opts.MaxRowArenaBytes || got.SelectedAccessMode != selected {
					t.Fatalf("initial accessor statistics mismatch: %+v", got)
				}
				mapped := mode != ReaderAccessReadAt && size != 0
				if mapped {
					if got.SelectedAccessMode != ReaderAccessMmap || got.ActiveWindows != 1 || got.MapCount != 1 || got.MappedBytes != 64 {
						t.Fatalf("initial mapped statistics mismatch: %+v", got)
					}
				} else if got.ActiveWindows != 0 || got.MapCount != 0 || got.MappedBytes != 0 {
					t.Fatalf("initial empty/lazy statistics mismatch: %+v", got)
				}
				if (got.FallbackReason != "") != emptyWindowsMapping || got.ReadWindowCount != 0 || got.UnmapCount != 0 || got.ReadBufferBytes != 0 {
					t.Fatalf("unexpected initial accessor work: %+v", got)
				}
			})
		}
	}
}

func TestReaderAccessorInitialFallbackStats(t *testing.T) {
	path := filepath.Join(t.TempDir(), "initial-fallback")
	if err := os.WriteFile(path, make([]byte, 256), 0o600); err != nil {
		t.Fatal(err)
	}
	file, err := os.OpenFile(path, os.O_WRONLY, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = file.Close() }()
	opts := DefaultReaderOptions().WithAccessMode(ReaderAccessAuto).WithWindowSize(64).WithMaxWindows(2).WithSnapshot(true)
	accessor, got, err := newReaderAccessor(file, opts)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = accessor.close() }()
	if got != accessor.stats() || got.RequestedAccessMode != ReaderAccessAuto || got.SelectedAccessMode != ReaderAccessReadAt ||
		got.FallbackReason == "" || got.FileSize != 256 || got.WindowSize != 64 || got.MaxWindows != 2 ||
		got.ActiveWindows != 0 || got.MapCount != 0 || got.UnmapCount != 0 || got.ReadWindowCount != 0 ||
		got.MappedBytes != 0 || got.ReadBufferBytes != 0 || got.RowArenaLimitBytes != opts.MaxRowArenaBytes {
		t.Fatalf("initial fallback statistics mismatch: %+v", got)
	}
}
