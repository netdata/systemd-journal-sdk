package journal

import (
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"unsafe"
)

var readerHeaderAllocationSink *Reader

func BenchmarkReaderAllocation(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		readerHeaderAllocationSink = new(Reader)
	}
}

func TestReaderAllocationShape(t *testing.T) {
	const count = 4096
	readers := make([]*Reader, count)
	var before, after runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&before)
	for i := range readers {
		readers[i] = new(Reader)
	}
	runtime.ReadMemStats(&after)
	runtime.KeepAlive(readers)
	found := false
	for i, class := range after.BySize {
		if delta := class.Mallocs - before.BySize[i].Mallocs; delta >= count {
			t.Logf("reader-size=%d heap-class=%d allocations=%d", unsafe.Sizeof(Reader{}), class.Size, delta)
			found = true
		}
	}
	if !found {
		t.Fatal("retained Reader allocations did not identify an allocation class")
	}
}

func readerOpenBenchmarkFixture(b *testing.B, compact bool, size string) (string, int) {
	b.Helper()
	if size == "small" {
		if root := os.Getenv("JOURNAL_SDK_READER_OPEN_SMALL_CORPUS"); root != "" {
			layout := "regular"
			if compact {
				layout = "compact"
			}
			return filepath.Join(root, layout, "system.journal"), 5
		}
	}
	if size == "large" {
		root := os.Getenv("JOURNAL_SDK_READER_OPEN_CORPUS")
		if root == "" {
			b.Skip("set JOURNAL_SDK_READER_OPEN_CORPUS to the immutable 100k-row corpus")
		}
		layout := "regular"
		if compact {
			layout = "compact"
		}
		return filepath.Join(root, layout, "system.journal"), 100000
	}
	rows := 0
	if size == "small" {
		rows = 5
	}
	opts := testOptions()
	opts.Compact = compact
	cadence := uint64(0)
	opts.LivePublishEveryEntries = &cadence
	path := filepath.Join(b.TempDir(), "open.journal")
	w, err := Create(path, opts)
	if err != nil {
		b.Fatal(err)
	}
	for i := 0; i < rows; i++ {
		if err := w.Append([]Field{StringField("MESSAGE", fmt.Sprintf("row-%d", i))},
			EntryOptions{RealtimeUsec: 1000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			_ = w.Close()
			b.Fatal(err)
		}
	}
	if err := w.Close(); err != nil {
		b.Fatal(err)
	}
	return path, rows
}

func BenchmarkReaderOpen(b *testing.B) {
	for _, compact := range []bool{false, true} {
		for _, size := range []string{"empty", "small", "large"} {
			for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
				b.Run(fmt.Sprintf("compact=%v/size=%s/mode=%d", compact, size, mode), func(b *testing.B) {
					b.StopTimer()
					path, rows := readerOpenBenchmarkFixture(b, compact, size)
					opts := DefaultReaderOptions().WithAccessMode(mode).WithSnapshot(true)
					check := func() {
						r, err := OpenFileWithOptions(path, opts)
						if err != nil {
							b.Fatal(err)
						}
						if r.header.nEntries != uint64(rows) || len(r.entryOffsets) != rows {
							_ = r.Close()
							b.Fatalf("open counts = %d/%d, want %d", r.header.nEntries, len(r.entryOffsets), rows)
						}
						if err := r.Close(); err != nil {
							b.Fatal(err)
						}
					}
					check()
					b.ReportAllocs()
					b.ResetTimer()
					b.StartTimer()
					for i := 0; i < b.N; i++ {
						check()
					}
					b.StopTimer()
				})
			}
		}
	}
}

func BenchmarkReaderEntryHeader(b *testing.B) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, test := range []struct {
			name    string
			window  uint64
			windows int
			offsets []uint64
			pinned  bool
		}{
			{"within", 64, 2, []uint64{16}, false},
			{"crossing", 64, 2, []uint64{56}, false},
			{"non-power", 63, 2, []uint64{16}, false},
			{"tiny", 8, 2, []uint64{16}, false},
			{"default", 0, 2, []uint64{16}, false},
			{"eviction", 64, 1, []uint64{16, 80, 144, 56, 120, 184}, false},
			{"pinned", 64, 1, []uint64{56}, true},
		} {
			b.Run(fmt.Sprintf("mode=%d/%s", mode, test.name), func(b *testing.B) {
				b.StopTimer()
				payload := make([]byte, 512)
				for _, offset := range test.offsets {
					payload[offset] = objectTypeEntry
					binary.LittleEndian.PutUint64(payload[offset+8:offset+16], entryObjectHeaderSize)
				}
				path := filepath.Join(b.TempDir(), "entry-headers")
				if err := os.WriteFile(path, payload, 0o600); err != nil {
					b.Fatal(err)
				}
				file, err := os.Open(path)
				if err != nil {
					b.Fatal(err)
				}
				opts := DefaultReaderOptions().WithAccessMode(mode).WithWindowSize(test.window).WithMaxWindows(test.windows).WithSnapshot(true).normalized()
				accessor, _, err := newReaderAccessor(file, opts)
				if err != nil {
					_ = file.Close()
					b.Fatal(err)
				}
				r := &Reader{file: file, accessor: accessor, options: opts, fileSize: accessor.size(),
					header: journalHeader{arenaSize: uint64(len(payload))}}
				r.configureLayout()
				b.Cleanup(func() { _ = r.Close() })
				if test.pinned {
					if _, err := r.readRowSlice(0, 8); err != nil {
						b.Fatal(err)
					}
				}
				for _, offset := range test.offsets {
					if valid, err := r.validEntryObjectOffset(offset); err != nil || !valid {
						b.Fatalf("entry preflight at %d = %v, %v", offset, valid, err)
					}
				}
				b.ReportAllocs()
				b.ResetTimer()
				b.StartTimer()
				for i := 0; i < b.N; i++ {
					if valid, err := r.validEntryObjectOffset(test.offsets[i%len(test.offsets)]); err != nil || !valid {
						b.Fatalf("entry check = %v, %v", valid, err)
					}
				}
				b.StopTimer()
			})
		}
	}
}
