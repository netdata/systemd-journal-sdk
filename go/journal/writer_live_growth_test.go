package journal

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

func TestLiveReadersDuringWriterArenaGrowth(t *testing.T) {
	for _, compact := range []bool{false, true} {
		t.Run(map[bool]string{false: "regular", true: "compact"}[compact], func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "system.journal")
			opts := testOptions()
			opts.Compact = compact
			opts.DataHashTableBuckets, opts.FieldHashTableBuckets = 64, 16
			w, err := Create(path, opts)
			if err != nil {
				t.Fatal(err)
			}
			defer w.Close()
			if err := w.AppendRaw([][]byte{[]byte("MESSAGE=seed")}, EntryOptions{RealtimeUsec: 1_700_000_000_000_001, MonotonicUsec: 1}); err != nil {
				t.Fatal(err)
			}
			open := func() *Reader {
				r, err := OpenFile(path)
				if err != nil {
					t.Fatal(err)
				}
				t.Cleanup(func() { _ = r.Close() })
				return r
			}
			existing := open()
			assertGrowthEntry(t, existing, []byte("seed"))
			value := bytes.Repeat([]byte{'x'}, 9*1024*1024-8)
			payload := append([]byte("MESSAGE="), value...)
			var during *Reader
			// Append and AppendRaw use this lazy payload path. Observe immediately
			// after the large DATA object, before the next ENTRY is published.
			err = w.appendPayloads(2, func(i int) uint64 {
				if i == 0 {
					return uint64(len(payload))
				}
				return uint64(len("ACTION=growth"))
			}, func(i int) []byte {
				if i == 0 {
					return payload
				}
				info, err := os.Stat(path)
				if err != nil {
					t.Fatal(err)
				}
				end := headerSize + w.header.arenaSize
				if end <= 8*1024*1024 || uint64(info.Size()) < end {
					t.Fatalf("growth not physically allocated: declared=%d physical=%d", end, info.Size())
				}
				during = open()
				assertGrowthEntry(t, during, []byte("seed"))
				for _, r := range []*Reader{existing, during} {
					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("unpublished entry visible: ok=%v err=%v", ok, err)
					}
				}
				return []byte("ACTION=growth")
			}, EntryOptions{RealtimeUsec: 1_700_000_000_000_002, MonotonicUsec: 2})
			if err != nil {
				t.Fatal(err)
			}
			if during == nil {
				t.Fatal("growth checkpoint was not reached")
			}
			for _, r := range []*Reader{existing, during} {
				assertGrowthEntry(t, r, value)
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("unexpected final entry: ok=%v err=%v", ok, err)
				}
			}
		})
	}
}

func assertGrowthEntry(t *testing.T, reader *Reader, expected []byte) {
	t.Helper()
	if ok, err := reader.Step(); err != nil || !ok {
		t.Fatalf("missing committed entry: ok=%v err=%v", ok, err)
	}
	value, found, err := reader.GetRaw([]byte("MESSAGE"))
	if err != nil || !found || !bytes.Equal(value, expected) {
		t.Fatalf("committed payload mismatch: found=%v bytes=%d err=%v", found, len(value), err)
	}
}
