package journal

import (
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func TestCountedArrayCyclesRejectRequiredRevisit(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		for _, postings := range []bool{false, true} {
			for _, descending := range []bool{false, true} {
				t.Run(fmt.Sprintf("compact=%v/postings=%v/descending=%v", compact, postings, descending), func(t *testing.T) {
					data := append([]byte(nil), source...)
					head, next := arrays[0].offset, arrays[1].offset
					var postingHeader uint64
					count := uint64(len(arrays[0].entries) + len(arrays[1].entries) + 1)
					if postings {
						probe, _ := filterTestReader(t, source, ReaderAccessReadAt, false)
						common := filterTestDataOffset(t, probe, "COMMON=all")
						postingHeader = common
						head = binary.LittleEndian.Uint64(data[common+48:])
						next = binary.LittleEndian.Uint64(data[head+16:])
						width := uint64(8)
						if compact {
							width = 4
						}
						count = 2 + (binary.LittleEndian.Uint64(data[head+8:])-24)/width + (binary.LittleEndian.Uint64(data[next+8:])-24)/width
						binary.LittleEndian.PutUint64(data[common+56:], count)
					} else {
						binary.LittleEndian.PutUint64(data[152:], count)
					}
					if descending {
						head, next = next, head
						if postings {
							binary.LittleEndian.PutUint64(data[postingHeader+48:], head)
						} else {
							binary.LittleEndian.PutUint64(data[176:], head)
						}
					}
					binary.LittleEndian.PutUint64(data[head+16:], next)
					binary.LittleEndian.PutUint64(data[next+16:], head)
					path := filepath.Join(t.TempDir(), "system.journal")
					if err := os.WriteFile(path, data, 0o600); err != nil {
						t.Fatal(err)
					}
					r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
					if err != nil {
						if errors.Is(err, errCorruptObject) {
							return
						}
						t.Fatal(err)
					}
					defer r.Close()
					if postings {
						r.AddMatch([]byte("COMMON=all"))
					}
					for i := uint64(0); i <= count; i++ {
						ok, err := r.Step()
						if err != nil {
							if !errors.Is(err, errCorruptObject) {
								t.Fatal(err)
							}
							return
						}
						if !ok {
							t.Fatal("required array revisit returned successful EOF")
						}
					}
					t.Fatal("required array revisit returned duplicate rows")
				})
			}
		}
	}
}

func TestCountedArrayAcceptsAcyclicBackwardLink(t *testing.T) {
	for _, compact := range []bool{false, true} {
		data, arrays := recoveryTestFixture(t, compact)
		a, b := arrays[0], arrays[1]
		entries := append(append([]uint64(nil), a.entries...), b.entries...)
		width := uint64(8)
		if compact {
			width = 4
		}
		put := func(at, value uint64) {
			if compact {
				binary.LittleEndian.PutUint32(data[at:], uint32(value))
			} else {
				binary.LittleEndian.PutUint64(data[at:], value)
			}
		}
		for i, entry := range entries {
			offset, slot := b.offset, i
			if i >= len(b.entries) {
				offset, slot = a.offset, i-len(b.entries)
			}
			put(offset+24+uint64(slot)*width, entry)
		}
		binary.LittleEndian.PutUint64(data[152:], uint64(len(entries)))
		binary.LittleEndian.PutUint64(data[176:], b.offset)
		binary.LittleEndian.PutUint64(data[b.offset+16:], a.offset)
		binary.LittleEndian.PutUint64(data[a.offset+16:], 0)
		r, _ := filterTestReader(t, data, ReaderAccessReadAt, false)
		got := recoveryTestTimes(t, r, false)
		if len(got) != len(entries) {
			t.Fatalf("acyclic backward link: %v", got)
		}
		for i, time := range got {
			if time != 1000+uint64(i) {
				t.Fatalf("acyclic backward link row %d: %d", i, time)
			}
		}
	}
}

func TestCountedArrayIgnoresUnusedTerminalLink(t *testing.T) {
	data, arrays := recoveryTestFixture(t, false)
	count := len(arrays[0].entries) + len(arrays[1].entries)
	binary.LittleEndian.PutUint64(data[152:], uint64(count))
	binary.LittleEndian.PutUint64(data[arrays[1].offset+16:], arrays[1].offset)
	r, _ := filterTestReader(t, data, ReaderAccessReadAt, false)
	if got := recoveryTestTimes(t, r, false); len(got) != count {
		t.Fatalf("unused tail: %d rows, want %d", len(got), count)
	}
}

func TestWriterCountedArrayRejectsRevisitBeforeMutation(t *testing.T) {
	for _, tailLookup := range []bool{false, true} {
		for _, descending := range []bool{false, true} {
			t.Run(fmt.Sprintf("tail=%v/descending=%v", tailLookup, descending), func(t *testing.T) {
				w, err := Create(filepath.Join(t.TempDir(), "system.journal"), testOptions())
				if err != nil {
					t.Fatal(err)
				}
				defer w.Close()
				a, err := w.allocateOffsetArray(4)
				if err != nil {
					t.Fatal(err)
				}
				b, err := w.allocateOffsetArray(8)
				if err != nil {
					t.Fatal(err)
				}
				if err := w.writeUint64At(a+16, b); err != nil {
					t.Fatal(err)
				}
				if err := w.writeUint64At(b+16, a); err != nil {
					t.Fatal(err)
				}
				head := a
				if descending {
					head = b
				}
				before := make([]byte, 32)
				if err := w.readAt(before, head+24); err != nil {
					t.Fatal(err)
				}
				if tailLookup {
					w.header.entryArrayOffset, w.header.nEntries, w.header.tailEntryArrayOffset = head, 13, 0
					_, err = w.entryArrayTailOffset()
				} else {
					_, _, err = w.appendToDataEntryArray(head, 13, 1024)
				}
				if !errors.Is(err, errInvalidJournal) {
					t.Fatalf("required array revisit: %v", err)
				}
				after := make([]byte, len(before))
				if err := w.readAt(after, head+24); err != nil {
					t.Fatal(err)
				}
				if string(after) != string(before) {
					t.Fatal("rejected cyclic append changed the array")
				}
			})
		}
	}
}
