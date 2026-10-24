package journal

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"testing"
)

type entrySlotHeaderAccessTrace struct {
	readerAccessor
	readCalls, sliceCalls int
	fail                  bool
}

func (a *entrySlotHeaderAccessTrace) readAt(dst []byte, offset uint64) error {
	a.readCalls++
	if a.fail {
		copy(dst, bytes.Repeat([]byte{0xee}, 8))
		return io.ErrUnexpectedEOF
	}
	return a.readerAccessor.readAt(dst, offset)
}

func (a *entrySlotHeaderAccessTrace) tempSlice(offset, size uint64) ([]byte, error) {
	a.sliceCalls++
	if a.fail {
		return nil, io.ErrUnexpectedEOF
	}
	return a.readerAccessor.tempSlice(offset, size)
}

func entrySlotHeaderTestReader(t testing.TB, mode ReaderAccessMode, windowSize uint64, windows int, prepare func([]byte)) (*Reader, []byte) {
	t.Helper()
	payload := make([]byte, 256)
	for i := range payload {
		payload[i] = byte(13*i + 7)
	}
	if prepare != nil {
		prepare(payload)
	}
	path := filepath.Join(t.TempDir(), "header-bytes")
	if err := os.WriteFile(path, payload, 0o600); err != nil {
		t.Fatal(err)
	}
	file, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	opts := DefaultReaderOptions().WithAccessMode(mode).WithWindowSize(windowSize).WithMaxWindows(windows).WithSnapshot(true)
	accessor, _, err := newReaderAccessor(file, opts)
	if err != nil {
		_ = file.Close()
		t.Fatal(err)
	}
	r := &Reader{file: file, accessor: accessor, options: opts, fileSize: accessor.size()}
	t.Cleanup(func() {
		if r.accessor != nil {
			if err := r.Close(); err != nil {
				t.Error(err)
			}
		}
	})
	return r, payload
}

func TestEntrySlotHeaderAccess(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, windowSize := range []uint64{64, 63, 8, 0} {
			for _, test := range []struct {
				name   string
				offset uint64
				closed bool
				bad    bool
			}{
				{name: "within", offset: 16},
				{name: "last-fitting", offset: 48},
				{name: "crossing", offset: 56},
				{name: "short", offset: 248, bad: true},
				{name: "overflow", offset: ^uint64(0) - 7, bad: true},
				{name: "closed-within", offset: 16, closed: true, bad: true},
				{name: "closed-crossing", offset: 56, closed: true, bad: true},
			} {
				t.Run(fmt.Sprintf("mode=%d/window=%d/%s", mode, windowSize, test.name), func(t *testing.T) {
					r, payload := entrySlotHeaderTestReader(t, mode, windowSize, 2, nil)
					if test.closed {
						if err := r.Close(); err != nil {
							t.Fatal(err)
						}
					}
					header, err := r.readEntrySlotHeader(test.offset)
					if test.bad {
						if header != nil || !errors.Is(err, errInvalidJournal) {
							t.Fatalf("read = %x, %v; want nil, invalid journal", header, err)
						}
						return
					}
					if err != nil || !bytes.Equal(header, payload[test.offset:test.offset+objectHeaderSize]) {
						t.Fatalf("header mismatch at %d: %x, %v", test.offset, header, err)
					}
				})
			}
		}
	}
}

func TestEntrySlotHeaderCachedRouting(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, test := range []struct {
			window, offset uint64
			slice          bool
		}{
			{64, 16, true}, {64, 48, true}, {64, 56, false},
			{63, 16, false}, {63, 56, false}, {8, 16, false},
			{0, 16, true}, {0, 56, true},
		} {
			t.Run(fmt.Sprintf("mode=%d/window=%d/offset=%d", mode, test.window, test.offset), func(t *testing.T) {
				r, payload := entrySlotHeaderTestReader(t, mode, test.window, 2, nil)
				var warm [objectHeaderSize]byte
				if err := r.readAt(warm[:], test.offset); err != nil {
					t.Fatal(err)
				}
				trace := &entrySlotHeaderAccessTrace{readerAccessor: r.accessor}
				r.accessor = trace
				before := r.AccessStats()
				for i := 0; i < 4; i++ {
					header, err := r.readEntrySlotHeader(test.offset)
					if err != nil || !bytes.Equal(header, payload[test.offset:test.offset+objectHeaderSize]) {
						t.Fatalf("cached header mismatch: %x, %v", header, err)
					}
				}
				after := r.AccessStats()
				if trace.sliceCalls != map[bool]int{true: 4, false: 0}[test.slice] ||
					trace.readCalls != map[bool]int{true: 0, false: 4}[test.slice] {
					t.Fatalf("route: slices=%d readAt=%d; native slice=%v", trace.sliceCalls, trace.readCalls, test.slice)
				}
				if after.TempCopyCount != before.TempCopyCount || after.MapCount != before.MapCount ||
					after.ReadWindowCount != before.ReadWindowCount || after.EvictionCount != before.EvictionCount {
					t.Fatalf("cached header introduced I/O or eviction: before=%+v after=%+v", before, after)
				}
			})
		}
	}
}

func TestEntrySlotHeaderPartialFailureRetry(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, windowSize := range []uint64{64, 63} {
			for _, offset := range []uint64{16, 56} {
				t.Run(fmt.Sprintf("mode=%d/window=%d/offset=%d", mode, windowSize, offset), func(t *testing.T) {
					r, payload := entrySlotHeaderTestReader(t, mode, windowSize, 2, nil)
					if _, err := r.readEntrySlotHeader(offset); err != nil {
						t.Fatal(err)
					}
					trace := &entrySlotHeaderAccessTrace{readerAccessor: r.accessor, fail: true}
					r.accessor = trace
					if header, err := r.readEntrySlotHeader(offset); header != nil || !errors.Is(err, io.ErrUnexpectedEOF) {
						t.Fatalf("failed read = %x, %v; want nil, unexpected EOF", header, err)
					}
					trace.fail = false
					for _, retry := range []uint64{120, 16} {
						header, err := r.readEntrySlotHeader(retry)
						if err != nil || !bytes.Equal(header, payload[retry:retry+objectHeaderSize]) {
							t.Fatalf("retry header mismatch at %d: %x, %v", retry, header, err)
						}
					}
				})
			}
		}
	}
}

func TestEntrySlotHeaderRetainedRowsAndEviction(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, pinned := range []bool{false, true} {
			t.Run(fmt.Sprintf("mode=%d/pinned=%v", mode, pinned), func(t *testing.T) {
				r, payload := entrySlotHeaderTestReader(t, mode, 64, 1, nil)
				var row []byte
				if pinned {
					var err error
					row, err = r.readRowSlice(0, 8)
					if err != nil {
						t.Fatal(err)
					}
				}
				before := r.AccessStats()
				for _, offset := range []uint64{80, 144, 56, 120, 184, 16} {
					header, err := r.readEntrySlotHeader(offset)
					if err != nil || !bytes.Equal(header, payload[offset:offset+objectHeaderSize]) {
						t.Fatalf("header mismatch at %d: %x, %v", offset, header, err)
					}
					if pinned && !bytes.Equal(row, payload[:8]) {
						t.Fatal("header access changed retained row bytes")
					}
				}
				after := r.AccessStats()
				if after.ActiveWindows > 1 || after.MappedBytes > 4096 || after.ReadBufferBytes > 64 {
					t.Fatalf("header reads exceeded cache bounds: %+v", after)
				}
				if pinned && after.RowPinnedWindows != 1 {
					t.Fatalf("header reads changed row pins: %+v", after)
				}
				if !pinned && after.EvictionCount <= before.EvictionCount {
					t.Fatal("header sequence did not exercise eviction")
				}
			})
		}
	}
}

func TestEntrySlotHeaderRawBytes(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, window := range []uint64{64, 63} {
			for _, offset := range []uint64{16, 56} {
				for _, test := range []struct {
					name       string
					typ, flags byte
				}{
					{"zero", 0, 0}, {"flagged-zero", 0, 0x80}, {"data", objectTypeData, 0},
				} {
					t.Run(fmt.Sprintf("mode=%d/window=%d/offset=%d/%s", mode, window, offset, test.name), func(t *testing.T) {
						r, payload := entrySlotHeaderTestReader(t, mode, window, 2, func(payload []byte) {
							clear(payload[offset : offset+objectHeaderSize])
							payload[offset], payload[offset+1] = test.typ, test.flags
						})
						header, err := r.readEntrySlotHeader(offset)
						if err != nil || !bytes.Equal(header, payload[offset:offset+objectHeaderSize]) {
							t.Fatalf("raw header mismatch: %x, %v", header, err)
						}
					})
				}
			}
		}
	}
}

func TestEntrySlotHeaderLiveRefresh(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, window := range []uint64{0, 8, 63, 64} {
				t.Run(fmt.Sprintf("compact=%v/mode=%d/window=%d", compact, mode, window), func(t *testing.T) {
					path := filepath.Join(t.TempDir(), "live.journal")
					opts := testOptions()
					opts.Compact = compact
					w, err := Create(path, opts)
					if err != nil {
						t.Fatal(err)
					}
					defer w.Close()
					readerOpts := DefaultReaderOptions().WithAccessMode(mode).WithWindowSize(window).WithMaxWindows(1)
					r, err := OpenFileWithOptions(path, readerOpts)
					if err != nil {
						t.Fatal(err)
					}
					defer r.Close()
					if len(r.entryOffsets) != 0 || r.options.WindowSize == 0 {
						t.Fatal("empty open did not normalize options or loaded entries")
					}
					if err := r.SeekHead(); err != nil {
						t.Fatal(err)
					}
					appendMessage(t, w, "row-1", 1001, 1)
					if changed, err := r.Refresh(); err != nil || !changed || len(r.entryOffsets) != 1 {
						t.Fatalf("first refresh = %v, %v; entries=%d", changed, err, len(r.entryOffsets))
					}
					if ok, err := r.Step(); err != nil || !ok {
						t.Fatalf("first Step = %v, %v", ok, err)
					}
					held, found, err := r.GetRaw([]byte("MESSAGE"))
					if err != nil || !found || string(held) != "row-1" {
						t.Fatalf("first MESSAGE = %q, %v, %v", held, found, err)
					}
					snapshot, err := OpenFileWithOptions(path, readerOpts.WithSnapshot(true))
					if err != nil {
						t.Fatal(err)
					}
					defer snapshot.Close()
					initialArrays := w.header.nEntryArrays
					for i := 2; i <= 33; i++ {
						appendMessage(t, w, fmt.Sprintf("row-%d", i), 1000+uint64(i), uint64(i))
					}
					if w.header.nEntryArrays <= initialArrays {
						t.Fatal("appends did not exercise ENTRY-array growth")
					}
					if changed, err := r.Refresh(); err != nil || !changed || len(r.entryOffsets) != 33 {
						t.Fatalf("growth refresh = %v, %v; entries=%d", changed, err, len(r.entryOffsets))
					}
					if string(held) != "row-1" {
						t.Fatalf("refresh changed retained MESSAGE: %q", held)
					}
					for i := 2; i <= 33; i++ {
						if ok, err := r.Step(); err != nil || !ok {
							t.Fatalf("Step %d = %v, %v", i, ok, err)
						}
						value, found, err := r.GetRaw([]byte("MESSAGE"))
						if err != nil || !found || string(value) != fmt.Sprintf("row-%d", i) {
							t.Fatalf("MESSAGE %d = %q, %v, %v", i, value, found, err)
						}
					}
					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("live EOF = %v, %v", ok, err)
					}
					if ok, err := snapshot.Step(); err != nil || !ok {
						t.Fatalf("snapshot first = %v, %v", ok, err)
					}
					if ok, err := snapshot.Step(); err != nil || ok {
						t.Fatalf("snapshot EOF = %v, %v", ok, err)
					}
				})
			}
		}
	}
}
