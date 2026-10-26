package journal

import (
	"errors"
	"fmt"
	"path/filepath"
	"syscall"
	"testing"
)

func TestDirectionalCursorNativeBoundaries(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "system.journal")
	opts := testOptions()
	writer, err := Create(path, opts)
	if err != nil {
		t.Fatal(err)
	}
	boots := []UUID{{1}, {2}, {3}}
	for i := 0; i < 6; i++ {
		timestamp := uint64(1000 + i)
		if i == 3 {
			timestamp = 1002
		}
		message := "other"
		if i%2 == 1 {
			message = "keep"
		}
		boot := boots[i/2]
		if err := writer.Append([]Field{StringField("ROW", fmt.Sprint(i)), StringField("MESSAGE", message), StringField("_BOOT_ID", boot.String())}, EntryOptions{RealtimeUsec: timestamp, MonotonicUsec: uint64(i%2 + 1), BootID: boot}); err != nil {
			t.Fatal(err)
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	seq := opts.SeqnumID.String()
	cases := []struct {
		cursor        string
		direction     Direction
		raw, filtered string
	}{
		{"t=3ea", DirectionForward, "2", "3"},
		{"t=3ea", DirectionBackward, "3", "3"},
		{"s=" + seq + ";i=1;t=3ed", DirectionBackward, "0", ""},
		{"s=" + seq + ";i=ffffffff", DirectionBackward, "5", "5"},
		{"s=" + seq + ";i=ffffffff", DirectionForward, "", ""},
		{"s=ffffffffffffffffffffffffffffffff;i=3", DirectionBackward, "5", "5"},
		{"s=ffffffffffffffffffffffffffffffff;i=3;m=0", DirectionBackward, "5", "5"},
		{"b=" + boots[1].String() + ";m=0", DirectionBackward, "1", "1"},
		{"b=" + boots[1].String() + ";m=0", DirectionForward, "2", "3"},
		{"b=" + boots[1].String() + ";m=ffffffff", DirectionForward, "4", "5"},
		{"b=ffffffffffffffffffffffffffffffff;m=0", DirectionBackward, "", ""},
		{"b=ffffffffffffffffffffffffffffffff;m=0;t=3ea", DirectionBackward, "3", "3"},
	}
	for _, directory := range []bool{false, true} {
		for _, filtered := range []bool{false, true} {
			for _, tc := range cases {
				t.Run(fmt.Sprintf("dir=%v/filter=%v/%s/%d", directory, filtered, tc.cursor, tc.direction), func(t *testing.T) {
					var j *sdJournal
					var err error
					if directory {
						j, err = SdJournalOpenDirectory(dir, 0)
					} else {
						j, err = SdJournalOpen(path, 0)
					}
					if err != nil {
						t.Fatal(err)
					}
					defer j.Close()
					if filtered {
						if err := SdJournalAddMatch(j, []byte("MESSAGE=keep")); err != nil {
							t.Fatal(err)
						}
					}
					if err := SdJournalSeekCursorDirection(j, tc.cursor, tc.direction); err != nil {
						t.Fatal(err)
					}
					want := tc.raw
					if filtered {
						want = tc.filtered
					}
					if want == "" {
						if _, err := SdJournalGetCursor(j); !errors.Is(err, ErrNoEntry) {
							t.Fatalf("expected absent cursor: %v", err)
						}
						return
					}
					entry, err := SdJournalGetEntry(j)
					if err != nil {
						t.Fatal(err)
					}
					if got := string(entry.Fields["ROW"]); got != want {
						t.Fatalf("row=%s want%s", got, want)
					}
				})
			}
		}
	}
}

func TestDirectionalCursorIndexedErrorsAndRecovery(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("compact=%v/access=%d", compact, mode), func(t *testing.T) {
				r, _ := filterTestReader(t, source, mode, false)
				unrelated := filterTestDataOffset(t, r, "MESSAGE=row-0001")
				r.AddMatch([]byte("COMMON=all"))
				trace := &filterAccessTrace{readerAccessor: r.accessor, forbidden: map[uint64]bool{unrelated: true}, faultAt: r.entryOffsets[500], fail: true}
				r.accessor = trace
				cursor := fmt.Sprintf("s=%s;i=%x", r.header.seqnumID.String(), 501)
				if err := r.SeekCursorDirection(cursor, DirectionBackward); !errors.Is(err, syscall.EIO) {
					t.Fatalf("I/O error=%v", err)
				}
				trace.fail = false
				trace.calls = 0
				if err := r.SeekCursorDirection(cursor, DirectionBackward); err != nil {
					t.Fatal(err)
				}
				if got, err := r.GetRealtimeUsec(); got != 1500 || err != nil {
					t.Fatalf("time=%d/%v", got, err)
				}
				if trace.calls > 160 {
					t.Fatalf("indexed seek performed %d accessor reads", trace.calls)
				}
				t.Logf("indexed metadata seek: %d accessor reads for1005 entries", trace.calls)
				tail := arrays[len(arrays)-1].entries
				recovered, _ := filterTestReader(t, source[:tail[len(tail)-1]], mode, false)
				recovered.AddMatch([]byte("COMMON=all"))
				if err := recovered.SeekCursorDirection("t=ffffffffffffffff", DirectionBackward); err != nil {
					t.Fatal(err)
				}
				if got, err := recovered.GetRealtimeUsec(); got != 2003 || err != nil {
					t.Fatalf("recovered last=%d/%v", got, err)
				}
				if !recovered.RecoveryStatus().Incomplete() {
					t.Fatal("recovery status missing")
				}
			})
		}
	}
}

func TestDirectionalCursorExactSequenceOverridesBootOrder(t *testing.T) {
	dir := t.TempDir()
	for i, times := range [][]uint64{{1100, 1200}, {1150, 1180}} {
		opts := testOptions()
		opts.SeqnumID = UUID{byte(10 + i)}
		opts.BootID = UUID{byte(20 + i)}
		w, err := Create(filepath.Join(dir, fmt.Sprintf("%d.journal", i)), opts)
		if err != nil {
			t.Fatal(err)
		}
		for n, time := range times {
			if err := w.Append([]Field{StringField("MESSAGE", fmt.Sprintf("%d-%d", i, n)), StringField("_BOOT_ID", opts.BootID.String())}, EntryOptions{RealtimeUsec: time, MonotonicUsec: uint64(n + 1)}); err != nil {
				t.Fatal(err)
			}
		}
		if err := w.Close(); err != nil {
			t.Fatal(err)
		}
	}
	single, err := OpenFile(filepath.Join(dir, "0.journal"))
	if err != nil {
		t.Fatal(err)
	}
	if ok, err := single.Step(); !ok || err != nil {
		t.Fatalf("first=%v/%v", ok, err)
	}
	cursor, err := single.GetCursor()
	if err != nil {
		t.Fatal(err)
	}
	single.Close()
	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	if err := dr.SeekCursorDirection(cursor, DirectionForward); err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"0-0", "0-1"} {
		entry, err := dr.GetEntry()
		if err != nil {
			t.Fatal(err)
		}
		if got := string(entry.Fields["MESSAGE"]); got != want {
			t.Fatalf("got%s want%s", got, want)
		}
		ok, err := dr.Step()
		if err != nil {
			t.Fatal(err)
		}
		if ok != (want == "0-0") {
			t.Fatalf("step after%s=%v", want, ok)
		}
	}
}

func TestDirectionalCursorClosedReader(t *testing.T) {
	source, _ := recoveryTestFixture(t, false)
	initial, path := filterTestReader(t, source, ReaderAccessMmap, false)
	if err := initial.Close(); err != nil {
		t.Fatal(err)
	}
	for _, directory := range []bool{false, true} {
		for _, state := range []string{"raw", "pending", "compiled"} {
			t.Run(fmt.Sprintf("directory=%v/%s", directory, state), func(t *testing.T) {
				var j *sdJournal
				var err error
				if directory {
					j, err = SdJournalOpenDirectory(filepath.Dir(path), 0)
				} else {
					j, err = SdJournalOpen(path, 0)
				}
				if err != nil {
					t.Fatal(err)
				}
				if state != "raw" {
					if err := SdJournalAddMatch(j, []byte("COMMON=all")); err != nil {
						t.Fatal(err)
					}
				}
				if state == "compiled" {
					if err := j.SeekCursorDirection("t=1", DirectionForward); err != nil {
						t.Fatal(err)
					}
				}
				if err := j.Close(); err != nil {
					t.Fatal(err)
				}
				for _, direction := range []Direction{DirectionForward, DirectionBackward, DirectionForward} {
					if err := j.SeekCursorDirection("t=1", direction); !errors.Is(err, errInvalidJournal) {
						t.Fatalf("closed seek: %v", err)
					}
				}
			})
		}
	}
}
