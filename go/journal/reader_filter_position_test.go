package journal

import (
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDirectoryFilteredMovementAfterClose(t *testing.T) {
	for _, facadeAPI := range []bool{false, true} {
		for _, backward := range []bool{false, true} {
			t.Run(fmt.Sprintf("facade=%v/back=%v", facadeAPI, backward), func(t *testing.T) {
				dr, err := OpenDirectory(createDirectoryReaderJournals(t, 1, 1))
				if err != nil {
					t.Fatal(err)
				}
				dr.AddMatch([]byte("PRIORITY=6"))
				if backward {
					if err := dr.SeekTail(); err != nil {
						t.Fatal(err)
					}
				}
				var facade *sdJournal
				if facadeAPI {
					facade = newSdJournal(dr)
					err = SdJournalClose(facade)
				} else {
					err = dr.Close()
				}
				if err != nil {
					t.Fatal(err)
				}
				for attempt := 0; attempt < 2; attempt++ {
					var ok bool
					if facade != nil {
						var n int
						if backward {
							n, err = SdJournalPrevious(facade)
						} else {
							n, err = SdJournalNext(facade)
						}
						ok = n != 0
					} else if backward {
						ok, err = dr.StepBack()
					} else {
						ok, err = dr.Step()
					}
					if ok || !errors.Is(err, errInvalidJournal) {
						t.Fatalf("closed movement %d: %v/%v, want invalid journal", attempt, ok, err)
					}
				}
			})
		}
	}
}

func TestDirectoryPositionAfterEntryLoadError(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				for _, filtered := range []bool{false, true} {
					for _, backward := range []bool{false, true} {
						for _, merged := range []bool{false, true} {
							// Unfiltered backward time-search error propagation is tracked in SOW-0045.
							if merged && backward && !filtered {
								continue
							}
							t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v/filter=%v/back=%v/merge=%v", compact, mode, facadeAPI, filtered, backward, merged), func(t *testing.T) {
								dir := t.TempDir()
								times := [2][3]uint64{{1000, 2000, 3000}, {4000, 5000, 6000}}
								if merged {
									times = [2][3]uint64{{1000, 3000, 5000}, {2000, 4000, 6000}}
								}
								for i, rows := range times {
									opts := testOptions()
									opts.Compact = compact
									if merged {
										opts.SeqnumID[0] += byte(i)
									} else {
										opts.HeadSeqnum = uint64(3*i + 1)
									}
									opts.FileID[0] += byte(i)
									w, err := Create(filepath.Join(dir, fmt.Sprintf("%d.journal", i)), opts)
									if err != nil {
										t.Fatal(err)
									}
									for _, time := range rows {
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
								if dr.nonOverlapping == merged {
									t.Fatal("fixture selects wrong directory path")
								}
								var facade *sdJournal
								if facadeAPI {
									facade = newSdJournal(dr)
								}
								if filtered {
									dr.AddMatch([]byte("COMMON=all"))
								}
								step := func(back bool) (bool, error) {
									if facade != nil {
										if back {
											n, err := SdJournalPrevious(facade)
											return n != 0, err
										}
										n, err := SdJournalNext(facade)
										return n != 0, err
									}
									if back {
										return dr.StepBack()
									}
									return dr.Step()
								}
								before, targetFile, targetRow := uint64(3000), 1, 0
								initialBack := backward
								if merged {
									initialBack = !backward
									if !backward {
										targetRow = 1
									}
								} else if backward {
									before, targetFile, targetRow = 4000, 0, 2
								}
								if initialBack {
									if err := dr.SeekTail(); err != nil {
										t.Fatal(err)
									}
								}
								for {
									if ok, err := step(initialBack); err != nil || !ok {
										t.Fatalf("initial movement: %v/%v", ok, err)
									}
									if time, err := dr.GetRealtimeUsec(); err != nil {
										t.Fatal(err)
									} else if time == before {
										break
									}
								}
								cursor, err := dr.GetCursor()
								if err != nil {
									t.Fatal(err)
								}
								wantPayload := fmt.Sprintf("MESSAGE=row-%d", before)
								assertPosition := func() {
									if time, err := dr.GetRealtimeUsec(); err != nil || time != before {
										t.Fatalf("position: %d/%v, want %d", time, err, before)
									}
									if got, err := dr.GetCursor(); err != nil || got != cursor {
										t.Fatalf("cursor changed: %q/%v", got, err)
									}
									payload, ok, err := dr.GetEntryPayload([]byte("MESSAGE"))
									if err != nil || !ok || string(payload) != wantPayload {
										t.Fatalf("payload: %q/%v/%v, want %q", payload, ok, err, wantPayload)
									}
								}
								assertPosition()
								r := dr.files[targetFile]
								offset := r.entryOffsets[targetRow]
								// Evict a prefetched copy so this call observes the controlled damage.
								if _, err := r.accessor.tempSlice(0, 1); err != nil {
									t.Fatal(err)
								}
								file, err := os.OpenFile(filepath.Join(dir, fmt.Sprintf("%d.journal", targetFile)), os.O_WRONLY, 0)
								if err != nil {
									t.Fatal(err)
								}
								defer file.Close()
								if _, err := file.WriteAt([]byte{2}, int64(offset)); err != nil {
									t.Fatal(err)
								}
								if ok, err := step(backward); err == nil || ok {
									t.Fatalf("damaged entry: %v/%v", ok, err)
								}
								assertPosition()
								if _, err := file.WriteAt([]byte{3}, int64(offset)); err != nil {
									t.Fatal(err)
								}
								assertPosition()
								for _, want := range []uint64{before + 1000, before + 2000} {
									if backward {
										want = 2*before - want
									}
									if ok, err := step(backward); err != nil || !ok {
										t.Fatalf("retry: %v/%v", ok, err)
									}
									if got, err := dr.GetRealtimeUsec(); err != nil || got != want {
										t.Fatalf("retry time: %d/%v, want %d", got, err, want)
									}
								}
							})
						}
					}
				}
			}
		}
	}
}

func positionTestStep(r interface {
	Step() (bool, error)
	StepBack() (bool, error)
}, facade *sdJournal, backward bool) (bool, error) {
	if facade != nil {
		var n int
		var err error
		if backward {
			n, err = SdJournalPrevious(facade)
		} else {
			n, err = SdJournalNext(facade)
		}
		return n != 0, err
	}
	if backward {
		return r.StepBack()
	}
	return r.Step()
}

func TestDirectoryInitialEntryErrorRetry(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				for _, filtered := range []bool{false, true} {
					for _, scenario := range []string{"head", "tail", "time-forward", "time-backward"} {
						if !filtered && strings.HasPrefix(scenario, "time-") {
							continue
						}
						t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v/filter=%v/%s", compact, mode, facadeAPI, filtered, scenario), func(t *testing.T) {
							dir := t.TempDir()
							for i := 0; i < 2; i++ {
								opts := testOptions()
								opts.Compact = compact
								opts.HeadSeqnum = uint64(3*i + 1)
								opts.FileID[0] += byte(i)
								w, err := Create(filepath.Join(dir, fmt.Sprintf("%d.journal", i)), opts)
								if err != nil {
									t.Fatal(err)
								}
								for j := 0; j < 3; j++ {
									time := uint64(1000 * (3*i + j + 1))
									if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: time, MonotonicUsec: time}); err != nil {
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
							var facade *sdJournal
							if facadeAPI {
								facade = newSdJournal(dr)
							}
							if filtered {
								dr.AddMatch([]byte("COMMON=all"))
							}
							backward := scenario == "tail" || scenario == "time-backward"
							fileIndex, rowIndex := 0, 0
							want := []uint64{1000, 2000, 3000, 4000, 5000, 6000}
							switch scenario {
							case "tail":
								fileIndex, rowIndex = 1, 2
								if err := dr.SeekTail(); err != nil {
									t.Fatal(err)
								}
							case "time-forward":
								fileIndex, rowIndex = 1, 0
								want = want[1:]
								if err := dr.SeekRealtimeUsec(1500); err != nil {
									t.Fatal(err)
								}
							case "time-backward":
								fileIndex, rowIndex = 1, 1
								want = want[:5]
								if err := dr.SeekRealtimeUsec(5500); err != nil {
									t.Fatal(err)
								}
							}
							step := func() (bool, error) {
								if facade != nil {
									if backward {
										n, err := SdJournalPrevious(facade)
										return n != 0, err
									}
									n, err := SdJournalNext(facade)
									return n != 0, err
								}
								if backward {
									return dr.StepBack()
								}
								return dr.Step()
							}
							offset := dr.files[fileIndex].entryOffsets[rowIndex]
							file, err := os.OpenFile(filepath.Join(dir, fmt.Sprintf("%d.journal", fileIndex)), os.O_WRONLY, 0)
							if err != nil {
								t.Fatal(err)
							}
							defer file.Close()
							if _, err := dr.files[fileIndex].accessor.tempSlice(0, 1); err != nil {
								t.Fatal(err)
							}
							if _, err := file.WriteAt([]byte{2}, int64(offset)); err != nil {
								t.Fatal(err)
							}
							if ok, err := step(); err == nil || ok {
								t.Fatalf("initial damage: %v/%v", ok, err)
							}
							if _, err := dr.GetCursor(); err == nil {
								t.Fatal("failed first movement selected a row")
							}
							if _, err := file.WriteAt([]byte{3}, int64(offset)); err != nil {
								t.Fatal(err)
							}
							for i := range want {
								expected := want[i]
								if backward {
									expected = want[len(want)-1-i]
								}
								if ok, err := step(); err != nil || !ok {
									t.Fatalf("retry: %v/%v, want %d", ok, err, expected)
								}
								if got, err := dr.GetRealtimeUsec(); err != nil || got != expected {
									t.Fatalf("retry time: %d/%v, want %d", got, err, expected)
								}
							}
							if ok, err := step(); err != nil || ok {
								t.Fatalf("EOF: %v/%v", ok, err)
							}
						})
					}
				}
			}
		}
	}
}

func positionTestWant(t *testing.T, r *Reader, facade *sdJournal, backward bool, want uint64) {
	t.Helper()
	if ok, err := positionTestStep(r, facade, backward); err != nil || !ok {
		t.Fatalf("step backward=%v: %v/%v", backward, ok, err)
	}
	if got, err := r.GetRealtimeUsec(); err != nil || got != want {
		t.Fatalf("backward=%v: time=%d/%v, want %d", backward, got, err, want)
	}
}

func TestNativeFilterReverseAnchor(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				for _, conjunction := range []bool{false, true} {
					for _, scenario := range []string{"install", "install-eof", "filtered-eof"} {
						t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v/and=%v/%s", compact, mode, facadeAPI, conjunction, scenario), func(t *testing.T) {
							r, _ := filterTestReader(t, source, mode, false)
							var facade *sdJournal
							if facadeAPI {
								facade = newSdJournal(r)
							}
							if scenario != "filtered-eof" {
								for i := 0; i < 1005; i++ {
									positionTestWant(t, r, facade, false, 1000+uint64(i))
								}
							}
							if conjunction {
								r.AddMatch([]byte("COMMON=all"))
							}
							r.AddMatch([]byte("SPARSE=some"))
							if scenario == "filtered-eof" {
								for _, want := range []uint64{1000, 1500, 2004} {
									positionTestWant(t, r, facade, false, want)
								}
							}
							if scenario != "install" {
								if ok, err := positionTestStep(r, facade, false); err != nil || ok {
									t.Fatalf("EOF=%v/%v", ok, err)
								}
							}
							positionTestWant(t, r, facade, true, 1500)
							positionTestWant(t, r, facade, false, 2004)
							positionTestWant(t, r, facade, true, 1500)
							positionTestWant(t, r, facade, true, 1000)
							if ok, err := positionTestStep(r, facade, true); err != nil || ok {
								t.Fatalf("BOF=%v/%v", ok, err)
							}
							positionTestWant(t, r, facade, false, 1500)
						})
					}
				}
			}
		}
	}
}

func TestReaderPositionAfterEntryLoadError(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		var entries []uint64
		for _, array := range arrays {
			entries = append(entries, array.entries...)
		}
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				for _, sparse := range []bool{false, true} {
					for _, backward := range []bool{false, true} {
						for _, damage := range []string{"type", "size"} {
							t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v/sparse=%v/back=%v/%s", compact, mode, facadeAPI, sparse, backward, damage), func(t *testing.T) {
								r, path := filterTestReader(t, source, mode, false)
								var facade *sdJournal
								if facadeAPI {
									facade = newSdJournal(r)
								}
								before := uint64(1499)
								if backward {
									before = 1501
								}
								if sparse {
									r.AddMatch([]byte("SPARSE=some"))
									before = 1000
									if backward {
										before = 2004
										if err := r.SeekTail(); err != nil {
											t.Fatal(err)
										}
									}
								} else {
									r.AddMatch([]byte("COMMON=all"))
									if err := r.SeekRealtimeUsec(before); err != nil {
										t.Fatal(err)
									}
								}
								positionTestWant(t, r, facade, backward, before)
								offset, damaged := entries[500], []byte{2}
								if damage == "size" {
									offset += 8
									damaged = make([]byte, 8)
									binary.LittleEndian.PutUint64(damaged, 8)
								}
								file, err := os.OpenFile(path, os.O_WRONLY, 0)
								if err != nil {
									t.Fatal(err)
								}
								defer file.Close()
								if _, err := file.WriteAt(damaged, int64(offset)); err != nil {
									t.Fatal(err)
								}
								if ok, err := positionTestStep(r, facade, backward); err == nil || ok {
									t.Fatalf("damaged entry=%v/%v", ok, err)
								}
								if got, err := r.GetRealtimeUsec(); err != nil || got != before {
									t.Fatalf("position while damaged=%d/%v, want %d", got, err, before)
								}
								if _, err := file.WriteAt(source[offset:offset+uint64(len(damaged))], int64(offset)); err != nil {
									t.Fatal(err)
								}
								if got, err := r.GetRealtimeUsec(); err != nil || got != before {
									t.Fatalf("position after error=%d/%v, want %d", got, err, before)
								}
								positionTestWant(t, r, facade, backward, 1500)
							})
						}
					}
				}
			}
		}
	}
}

func TestNativeFilterNestedDirectionChange(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v", compact, mode, facadeAPI), func(t *testing.T) {
					r, _ := filterTestReader(t, source, mode, false)
					var facade *sdJournal
					if facadeAPI {
						facade = newSdJournal(r)
					}
					if err := r.SeekRealtimeUsec(1500); err != nil {
						t.Fatal(err)
					}
					positionTestWant(t, r, facade, false, 1500)
					r.AddMatch([]byte("MESSAGE=row-0500"))
					r.AddMatch([]byte("SPARSE=some"))
					if err := r.AddDisjunction(); err != nil {
						t.Fatal(err)
					}
					r.AddMatch([]byte("TAIL_PAIR=some"))
					positionTestWant(t, r, facade, false, 2003)
					positionTestWant(t, r, facade, true, 1500)
					positionTestWant(t, r, facade, false, 2003)
					positionTestWant(t, r, facade, false, 2004)
					positionTestWant(t, r, facade, true, 2003)
					positionTestWant(t, r, facade, true, 1500)
				})
			}
		}
	}
}

func TestNativeFilterResolvedBackwardSkipsUnvisitedLeaf(t *testing.T) {
	source, _ := recoveryTestFixture(t, false)
	probe, _ := filterTestReader(t, source, ReaderAccessReadAt, false)
	common := filterTestDataOffset(t, probe, "COMMON=all")
	array := binary.LittleEndian.Uint64(source[common+48:])
	source[array] = objectTypeData
	for _, facadeAPI := range []bool{false, true} {
		t.Run(fmt.Sprintf("facade=%v", facadeAPI), func(t *testing.T) {
			r, _ := filterTestReader(t, source, ReaderAccessReadAt, false)
			if err := r.SeekRealtimeUsec(1500); err != nil {
				t.Fatal(err)
			}
			positionTestWant(t, r, nil, false, 1500)
			r.AddMatch([]byte("COMMON=all"))
			r.AddMatch([]byte("ZZ_ABSENT=x"))
			var facade *sdJournal
			if facadeAPI {
				facade = newSdJournal(r)
			}
			if ok, err := positionTestStep(r, facade, true); err != nil || ok {
				t.Fatalf("resolved absent AND: %v/%v", ok, err)
			}
			if err := r.SeekTail(); err != nil {
				t.Fatal(err)
			}
			if ok, err := positionTestStep(r, facade, true); err == nil || ok {
				t.Fatalf("tail must prepare every leaf: %v/%v", ok, err)
			}
		})
	}
}

func TestNativeFilterResolvedEOFDoesNotRescan(t *testing.T) {
	source, _ := recoveryTestFixture(t, false)
	for _, backward := range []bool{false, true} {
		t.Run(fmt.Sprintf("back=%v", backward), func(t *testing.T) {
			r, _ := filterTestReader(t, source, ReaderAccessReadAt, false)
			anchor := uint64(2004)
			if backward {
				anchor = 1000
			}
			if err := r.SeekRealtimeUsec(anchor); err != nil {
				t.Fatal(err)
			}
			positionTestWant(t, r, nil, false, anchor)
			r.AddMatch([]byte("COMMON=all"))
			trace := &filterAccessTrace{readerAccessor: r.accessor}
			r.accessor = trace
			if ok, err := positionTestStep(r, nil, backward); err != nil || ok {
				t.Fatalf("first boundary: %v/%v", ok, err)
			}
			calls := trace.calls
			for i := 0; i < 2; i++ {
				if ok, err := positionTestStep(r, nil, backward); err != nil || ok {
					t.Fatalf("repeated boundary: %v/%v", ok, err)
				}
			}
			if trace.calls != calls {
				t.Fatalf("boundary reread postings: %d -> %d", calls, trace.calls)
			}
			if err := r.SeekRealtimeUsec(1500); err != nil {
				t.Fatal(err)
			}
			if err := r.Next(); err != nil {
				t.Fatal(err)
			}
			want := uint64(1501)
			if backward {
				want = 1499
			}
			positionTestWant(t, r, nil, backward, want)
		})
	}
}

func TestNativeFilterRealtimeProbeBound(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				for _, backward := range []bool{false, true} {
					t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v/back=%v", compact, mode, facadeAPI, backward), func(t *testing.T) {
						r, _ := filterTestReader(t, source, mode, false)
						var facade *sdJournal
						if facadeAPI {
							facade = newSdJournal(r)
						}
						r.AddMatch([]byte("COMMON=all"))
						if err := r.filter.build(r); err != nil {
							t.Fatal(err)
						}
						trace := &filterAccessTrace{readerAccessor: r.accessor}
						r.accessor = trace
						if err := r.SeekRealtimeUsec(1500); err != nil {
							t.Fatal(err)
						}
						positionTestWant(t, r, facade, backward, 1500)
						t.Logf("time-seek reads=%d", trace.calls)
						if trace.calls > 120 {
							t.Fatalf("time seek made %d reads; want at most 120 for 1005 postings", trace.calls)
						}
						want := uint64(1501)
						if backward {
							want = 1499
						}
						positionTestWant(t, r, facade, backward, want)
					})
				}
			}
		}
	}
}

func TestNativeFilterRealtimeDuplicateBounds(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		index := uint64(0)
		for _, array := range arrays {
			for _, entry := range array.entries {
				binary.LittleEndian.PutUint64(source[entry+24:], 1000+2*(index/7))
				index++
			}
		}
		binary.LittleEndian.PutUint64(source[192:], 1000+2*((index-1)/7))
		for _, live := range []bool{false, true} {
			for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
				for _, facadeAPI := range []bool{false, true} {
					t.Run(fmt.Sprintf("compact=%v/live=%v/mode=%d/facade=%v", compact, live, mode, facadeAPI), func(t *testing.T) {
						r, _ := filterTestReader(t, source, mode, live)
						var facade *sdJournal
						if facadeAPI {
							facade = newSdJournal(r)
						}
						r.AddMatch([]byte("COMMON=all"))
						for _, tc := range []struct {
							time uint64
							back bool
							seq  uint64
						}{{0, false, 1}, {0, true, 0}, {999, true, 0}, {1000, false, 1}, {1000, true, 7}, {1001, false, 8}, {1001, true, 7}, {1142, false, 498}, {1142, true, 504}, {1143, false, 505}, {1143, true, 504}, {1286, false, 1002}, {1286, true, 1005}, {1287, false, 0}, {^uint64(0), false, 0}, {^uint64(0), true, 1005}} {
							if err := r.SeekRealtimeUsec(tc.time); err != nil {
								t.Fatal(err)
							}
							ok, err := positionTestStep(r, facade, tc.back)
							if err != nil || ok != (tc.seq != 0) {
								t.Fatalf("seek %d/back=%v: %v/%v, want seq%d", tc.time, tc.back, ok, err, tc.seq)
							}
							if ok {
								entry, err := r.GetEntry()
								if err != nil || entry.Seqnum != tc.seq {
									t.Fatalf("seek %d/back=%v: entry=%v/%v, want seq%d", tc.time, tc.back, entry, err, tc.seq)
								}
							}
						}
					})
				}
			}
		}
	}
}

func TestNativeFilterRealtimeSlotDiscoveryAndRetry(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		probe, _ := filterTestReader(t, source, ReaderAccessReadAt, false)
		data := filterTestDataOffset(t, probe, "COMMON=all")
		if err := probe.Close(); err != nil {
			t.Fatal(err)
		}
		width := uint64(8)
		if compact {
			width = 4
		}
		array := binary.LittleEndian.Uint64(source[data+48:])
		first := array + 24
		remaining := binary.LittleEndian.Uint64(source[data+56:]) - 1
		middle := first + minUint64(remaining, (binary.LittleEndian.Uint64(source[array+8:])-24)/width)/2*width
		var last uint64
		for remaining > 0 {
			count := minUint64(remaining, (binary.LittleEndian.Uint64(source[array+8:])-24)/width)
			last = array + 24 + (count-1)*width
			remaining -= count
			array = binary.LittleEndian.Uint64(source[array+16:])
		}
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, facadeAPI := range []bool{false, true} {
				for _, backward := range []bool{false, true} {
					for _, probed := range []bool{false, true} {
						t.Run(fmt.Sprintf("compact=%v/mode=%d/facade=%v/back=%v/probed=%v", compact, mode, facadeAPI, backward, probed), func(t *testing.T) {
							at := first
							if backward {
								at = last
							}
							if probed {
								at = middle
							}
							damaged := append([]byte(nil), source...)
							clear(damaged[at : at+width])
							r, path := filterTestReader(t, damaged, mode, false)
							var facade *sdJournal
							if facadeAPI {
								facade = newSdJournal(r)
							}
							r.AddMatch([]byte("COMMON=all"))
							if err := r.SeekRealtimeUsec(1500); err != nil {
								t.Fatal(err)
							}
							if probed {
								if ok, err := positionTestStep(r, facade, backward); err == nil || ok {
									t.Fatalf("visited zero slot=%v/%v", ok, err)
								}
								file, err := os.OpenFile(path, os.O_WRONLY, 0)
								if err != nil {
									t.Fatal(err)
								}
								_, err = file.WriteAt(source[at:at+width], int64(at))
								closeErr := file.Close()
								if err != nil || closeErr != nil {
									t.Fatalf("repair=%v/%v", err, closeErr)
								}
							}
							positionTestWant(t, r, facade, backward, 1500)
							if !probed {
								if backward {
									if err := r.SeekTail(); err != nil {
										t.Fatal(err)
									}
								} else {
									if err := r.SeekHead(); err != nil {
										t.Fatal(err)
									}
									positionTestWant(t, r, facade, false, 1000)
								}
								if ok, err := positionTestStep(r, facade, backward); err == nil || ok {
									t.Fatalf("ordinary traversal skipped zero slot=%v/%v", ok, err)
								}
							}
						})
					}
				}
			}
		}
	}
}
