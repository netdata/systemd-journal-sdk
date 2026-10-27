package journal

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDirectoryFilterReplacementKeepsSequentialDiscovery(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, installed := range []bool{false, true} {
			t.Run(fmt.Sprintf("compact=%v/installed=%v", compact, installed), func(t *testing.T) {
				dir := t.TempDir()
				for index := 0; index < 3; index++ {
					opts := testOptions()
					opts.Compact = compact
					opts.FileID[0] += byte(index)
					opts.HeadSeqnum = uint64(3*index + 1)
					w, err := Create(filepath.Join(dir, fmt.Sprintf("%d.journal", index)), opts)
					if err != nil {
						t.Fatal(err)
					}
					for row := 1; row <= 3; row++ {
						time := uint64(3*index+row) * 1000
						if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: time, MonotonicUsec: time}); err != nil {
							t.Fatal(err)
						}
					}
					if err := w.Close(); err != nil {
						t.Fatal(err)
					}
				}
				dr, err := OpenDirectoryWithOptions(dir, DefaultReaderOptions().WithSnapshot(true))
				if err != nil {
					t.Fatal(err)
				}
				defer dr.Close()
				if !dr.nonOverlapping {
					t.Fatal("fixture must select sequential traversal")
				}
				if installed {
					dr.AddMatch([]byte("COMMON=all"))
				}
				for time := uint64(1000); time <= 4000; time += 1000 {
					if ok, err := positionTestStep(dr, nil, false); err != nil || !ok {
						t.Fatalf("initial step: %v/%v", ok, err)
					}
					if got, err := dr.GetRealtimeUsec(); err != nil || got != time {
						t.Fatalf("initial time: %d/%v, want %d", got, err, time)
					}
				}
				file, err := os.OpenFile(filepath.Join(dir, "2.journal"), os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				defer file.Close()
				offset := int64(dr.files[2].entryOffsets[0])
				if _, err := file.WriteAt([]byte{1}, offset); err != nil {
					t.Fatal(err)
				}
				dr.AddMatch([]byte("COMMON=all"))
				for _, time := range []uint64{5000, 6000} {
					if ok, err := positionTestStep(dr, nil, false); err != nil || !ok {
						t.Fatalf("nearer healthy step: %v/%v, want %d", ok, err, time)
					}
					if got, err := dr.GetRealtimeUsec(); err != nil || got != time {
						t.Fatalf("nearer healthy time: %d/%v, want %d", got, err, time)
					}
				}
				if ok, err := positionTestStep(dr, nil, false); err == nil || ok {
					t.Fatalf("damaged required ENTRY must fail: %v/%v", ok, err)
				}
				if got, err := dr.GetRealtimeUsec(); err != nil || got != 6000 {
					t.Fatalf("error changed selected row: %d/%v", got, err)
				}
				if _, err := file.WriteAt([]byte{3}, offset); err != nil {
					t.Fatal(err)
				}
				if ok, err := positionTestStep(dr, nil, false); err != nil || !ok {
					t.Fatalf("repair retry: %v/%v", ok, err)
				}
				if got, err := dr.GetRealtimeUsec(); err != nil || got != 7000 {
					t.Fatalf("repair time: %d/%v", got, err)
				}
			})
		}
	}
}

func TestDirectoryRecoveryMergeErrorRetainsPrefetchedRow(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v", compact, mode, facadeAPI), func(t *testing.T) {
					dir := t.TempDir()
					for index := 0; index < 3; index++ {
						opts := testOptions()
						opts.Compact = compact
						opts.FileID[0] += byte(index)
						opts.HeadSeqnum = uint64(3*index + 1)
						w, err := Create(filepath.Join(dir, fmt.Sprintf("%d.journal", index)), opts)
						if err != nil {
							t.Fatal(err)
						}
						for row := 1; row <= 3; row++ {
							time := uint64(3*index+row) * 1000
							fields := []Field{StringField("MESSAGE", fmt.Sprintf("row-%d", time)), StringField("COMMON", "all"), StringField("PADDING", strings.Repeat("p", 2048)+fmt.Sprint(time))}
							if err := w.Append(fields, EntryOptions{RealtimeUsec: time, MonotonicUsec: time}); err != nil {
								t.Fatal(err)
							}
						}
						if err := w.Close(); err != nil {
							t.Fatal(err)
						}
					}
					dr, err := OpenDirectoryWithOptions(dir, DefaultReaderOptions().WithAccessMode(mode).WithWindowSize(1024).WithMaxWindows(1).WithSnapshot(true))
					if err != nil {
						t.Fatal(err)
					}
					defer dr.Close()
					if !dr.nonOverlapping {
						t.Fatal("fixture must start with sequential traversal")
					}
					dr.AddMatch([]byte("COMMON=all"))
					if err := dr.SeekTail(); err != nil {
						t.Fatal(err)
					}
					var facade *sdJournal
					if facadeAPI {
						facade = newSdJournal(dr)
					}
					for time := uint64(9000); time >= 4000; time -= 1000 {
						if ok, err := positionTestStep(dr, facade, true); err != nil || !ok {
							t.Fatalf("initial step: %v/%v", ok, err)
						}
						if got, err := dr.GetRealtimeUsec(); err != nil || got != time {
							t.Fatalf("initial time: %d/%v, want %d", got, err, time)
						}
					}
					cursor, err := dr.GetCursor()
					if err != nil {
						t.Fatal(err)
					}
					for index, row := range map[int]int{0: 2, 2: 1} {
						r := dr.files[index]
						offset := r.entryOffsets[row]
						if _, err := r.accessor.tempSlice(0, 1); err != nil {
							t.Fatal(err)
						}
						file, err := os.OpenFile(filepath.Join(dir, fmt.Sprintf("%d.journal", index)), os.O_WRONLY, 0)
						if err != nil {
							t.Fatal(err)
						}
						defer file.Close()
						damage := []byte{2}
						if index == 0 {
							damage = make([]byte, 16)
						}
						if _, err := file.WriteAt(damage, int64(offset)); err != nil {
							t.Fatal(err)
						}
					}
					if ok, err := positionTestStep(dr, facade, true); err == nil || ok {
						t.Fatalf("merge probe must fail: %v/%v", ok, err)
					}
					if got, err := dr.GetRealtimeUsec(); err != nil || got != 4000 {
						t.Fatalf("selected time: %d/%v, want 4000", got, err)
					}
					if got, err := dr.GetCursor(); err != nil || got != cursor {
						t.Fatalf("selected cursor changed: %q/%v", got, err)
					}
					if len(dr.RecoveryStatuses()) != 1 || dr.nonOverlapping {
						t.Fatal("missing tail must switch the directory to merging")
					}
					file, err := os.OpenFile(filepath.Join(dir, "2.journal"), os.O_WRONLY, 0)
					if err != nil {
						t.Fatal(err)
					}
					defer file.Close()
					if _, err := file.WriteAt([]byte{3}, int64(dr.files[2].entryOffsets[1])); err != nil {
						t.Fatal(err)
					}
					for _, time := range []uint64{2000, 1000} {
						if ok, err := positionTestStep(dr, facade, true); err != nil || !ok {
							t.Fatalf("repaired retry: %v/%v", ok, err)
						}
						if got, err := dr.GetRealtimeUsec(); err != nil || got != time {
							t.Fatalf("retry time: %d/%v, want %d", got, err, time)
						}
					}
					if ok, err := positionTestStep(dr, facade, true); err != nil || ok {
						t.Fatalf("recovered EOF: %v/%v", ok, err)
					}
				})
			}
		}
	}
}
