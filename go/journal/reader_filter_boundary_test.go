package journal

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func directoryFilterBoundaryFixture(t *testing.T, compact, merged bool) string {
	t.Helper()
	dir := t.TempDir()
	times := [2][3]uint64{{1000, 2000, 3000}, {4000, 5000, 6000}}
	if merged {
		times = [2][3]uint64{{1000, 3000, 5000}, {2000, 4000, 6000}}
	}
	for i, rows := range times {
		opts := testOptions()
		opts.Compact = compact
		opts.FileID[0] += byte(i)
		if merged {
			opts.SeqnumID[0] += byte(i)
		} else {
			opts.HeadSeqnum = uint64(3*i + 1)
		}
		w, err := Create(filepath.Join(dir, fmt.Sprintf("%d.journal", i)), opts)
		if err != nil {
			t.Fatal(err)
		}
		for _, time := range rows {
			if err := w.Append([]Field{StringField("COMMON", "all"), StringField("MESSAGE", fmt.Sprintf("row-%d", time))}, EntryOptions{RealtimeUsec: time, MonotonicUsec: time}); err != nil {
				t.Fatal(err)
			}
		}
		if err := w.Close(); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func TestDirectoryFilterMutationKeepsEmittedAnchor(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, merged := range []bool{false, true} {
			dir := directoryFilterBoundaryFixture(t, compact, merged)
			for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
				for _, facadeAPI := range []bool{false, true} {
					for _, backward := range []bool{false, true} {
						for _, mutation := range []string{"install", "replace", "and", "or"} {
							t.Run(fmt.Sprintf("compact=%v/merge=%v/mode=%d/facade=%v/back=%v/%s", compact, merged, mode, facadeAPI, backward, mutation), func(t *testing.T) {
								dr, err := OpenDirectoryWithOptions(dir, DefaultReaderOptions().WithAccessMode(mode).WithSnapshot(true))
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
								addValue := func(value string) {
									if facade != nil {
										if err := SdJournalAddMatch(facade, []byte(value)); err != nil {
											t.Fatal(err)
										}
									} else {
										dr.AddMatch([]byte(value))
									}
								}
								if mutation == "replace" {
									addValue("COMMON=all")
								}
								for want := uint64(1000); want <= 3000; want += 1000 {
									if ok, err := positionTestStep(dr, facade, false); err != nil || !ok {
										t.Fatalf("initial step=%v/%v", ok, err)
									}
									if got, err := dr.GetRealtimeUsec(); err != nil || got != want {
										t.Fatalf("initial time=%d/%v, want %d", got, err, want)
									}
								}
								if mutation == "replace" {
									addValue("MESSAGE=row-1000")
									addValue("MESSAGE=row-5000")
								} else if mutation == "or" {
									addValue("MESSAGE=row-1000")
								} else {
									addValue("COMMON=all")
								}
								if mutation == "and" || mutation == "or" {
									if facade != nil {
										if mutation == "and" {
											err = SdJournalAddConjunction(facade)
										} else {
											err = SdJournalAddDisjunction(facade)
										}
									} else if mutation == "and" {
										err = dr.AddConjunction()
									} else {
										err = dr.AddDisjunction()
									}
									if err != nil {
										t.Fatal(err)
									}
									if mutation == "and" {
										addValue("MESSAGE=row-1000")
									}
									addValue("MESSAGE=row-5000")
								}
								if got, err := dr.GetRealtimeUsec(); err != nil || got != 3000 {
									t.Fatalf("mutation changed selected row: %d/%v", got, err)
								}
								wants := []uint64{4000, 5000, 6000}
								if backward {
									wants = []uint64{2000, 1000}
								}
								if mutation != "install" {
									wants = []uint64{5000}
									if backward {
										wants = []uint64{1000}
									}
								}
								for _, want := range wants {
									if ok, err := positionTestStep(dr, facade, backward); err != nil || !ok {
										t.Fatalf("mutated step=%v/%v, want %d", ok, err, want)
									}
									if got, err := dr.GetRealtimeUsec(); err != nil || got != want {
										t.Fatalf("mutated time=%d/%v, want %d", got, err, want)
									}
								}
								if ok, err := positionTestStep(dr, facade, backward); err != nil || ok {
									t.Fatalf("mutated EOF=%v/%v", ok, err)
								}
							})
						}
					}
				}
			}
		}
	}
}

func TestDirectoryFilterMutationKeepsFailedRealtimeSeek(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, merged := range []bool{false, true} {
			for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
				for _, facadeAPI := range []bool{false, true} {
					for _, backward := range []bool{false, true} {
						t.Run(fmt.Sprintf("compact=%v/merge=%v/mode=%d/facade=%v/back=%v", compact, merged, mode, facadeAPI, backward), func(t *testing.T) {
							dir := directoryFilterBoundaryFixture(t, compact, merged)
							dr, err := OpenDirectoryWithOptions(dir, DefaultReaderOptions().WithAccessMode(mode).WithSnapshot(true))
							if err != nil {
								t.Fatal(err)
							}
							defer dr.Close()
							var facade *sdJournal
							if facadeAPI {
								facade = newSdJournal(dr)
							}
							addMatch := func() {
								if facade != nil {
									if err := SdJournalAddMatch(facade, []byte("COMMON=all")); err != nil {
										t.Fatal(err)
									}
								} else {
									dr.AddMatch([]byte("COMMON=all"))
								}
							}
							addMatch()
							if err := dr.SeekRealtimeUsec(3500); err != nil {
								t.Fatal(err)
							}
							fileIndex, rowIndex, want := 1, 0, uint64(4000)
							if backward {
								fileIndex, rowIndex, want = 0, 2, 3000
							}
							if merged {
								rowIndex = 1
							}
							path := filepath.Join(dir, fmt.Sprintf("%d.journal", fileIndex))
							var offset uint64
							for _, reader := range dr.files {
								if reader.file.Name() == path {
									offset = reader.entryOffsets[rowIndex]
								}
							}
							if offset == 0 {
								t.Fatal("missing target ENTRY")
							}
							file, err := os.OpenFile(path, os.O_WRONLY, 0)
							if err != nil {
								t.Fatal(err)
							}
							defer file.Close()
							if _, err := file.WriteAt([]byte{objectTypeData}, int64(offset)); err != nil {
								t.Fatal(err)
							}
							if ok, err := positionTestStep(dr, facade, backward); err == nil || ok {
								t.Fatalf("damaged seek=%v/%v", ok, err)
							}
							addMatch()
							if _, err := file.WriteAt([]byte{objectTypeEntry}, int64(offset)); err != nil {
								t.Fatal(err)
							}
							if ok, err := positionTestStep(dr, facade, backward); err != nil || !ok {
								t.Fatalf("repaired seek=%v/%v", ok, err)
							}
							if got, err := dr.GetRealtimeUsec(); err != nil || got != want {
								t.Fatalf("repaired time=%d/%v, want %d", got, err, want)
							}
						})
					}
				}
			}
		}
	}
}

func TestDirectoryFlushMatchesDiscardsPendingRealtimeSeek(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, merged := range []bool{false, true} {
			dir := directoryFilterBoundaryFixture(t, compact, merged)
			for _, facadeAPI := range []bool{false, true} {
				t.Run(fmt.Sprintf("compact=%v/merge=%v/facade=%v", compact, merged, facadeAPI), func(t *testing.T) {
					dr, err := OpenDirectory(dir)
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
						if err := SdJournalAddMatch(facade, []byte("COMMON=all")); err != nil {
							t.Fatal(err)
						}
						if err := SdJournalSeekRealtimeUsec(facade, 5500); err != nil {
							t.Fatal(err)
						}
						if err := SdJournalFlushMatches(facade); err != nil {
							t.Fatal(err)
						}
					} else {
						dr.AddMatch([]byte("COMMON=all"))
						if err := dr.SeekRealtimeUsec(5500); err != nil {
							t.Fatal(err)
						}
						dr.FlushMatches()
					}
					if ok, err := positionTestStep(dr, facade, false); err != nil || !ok {
						t.Fatalf("flushed step: %v/%v", ok, err)
					}
					if got, err := dr.GetRealtimeUsec(); err != nil || got != 1000 {
						t.Fatalf("flushed time=%d/%v, want 1000", got, err)
					}
				})
			}
		}
	}
}

func TestDirectoryFilteredEOFReversalKeepsNearestMatch(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, merged := range []bool{false, true} {
			dir := directoryFilterBoundaryFixture(t, compact, merged)
			for _, facadeAPI := range []bool{false, true} {
				for _, backward := range []bool{false, true} {
					t.Run(fmt.Sprintf("compact=%v/merge=%v/facade=%v/back=%v", compact, merged, facadeAPI, backward), func(t *testing.T) {
						dr, err := OpenDirectory(dir)
						if err != nil {
							t.Fatal(err)
						}
						defer dr.Close()
						if dr.nonOverlapping == merged {
							t.Fatal("fixture selects wrong directory path")
						}
						dr.AddMatch([]byte("COMMON=all"))
						if backward {
							if err := dr.SeekTail(); err != nil {
								t.Fatal(err)
							}
						}
						var facade *sdJournal
						if facadeAPI {
							facade = newSdJournal(dr)
						}
						for i := uint64(1); i <= 6; i++ {
							want := 1000 * i
							if backward {
								want = 7000 - want
							}
							if ok, err := positionTestStep(dr, facade, backward); err != nil || !ok {
								t.Fatalf("step=%v/%v, want %d", ok, err, want)
							}
							if got, err := dr.GetRealtimeUsec(); err != nil || got != want {
								t.Fatalf("time=%d/%v, want %d", got, err, want)
							}
						}
						for i := 0; i < 2; i++ {
							if ok, err := positionTestStep(dr, facade, backward); err != nil || ok {
								t.Fatalf("EOF %d=%v/%v", i, ok, err)
							}
						}
						want := uint64(5000)
						if backward {
							want = 2000
						}
						if ok, err := positionTestStep(dr, facade, !backward); err != nil || !ok {
							t.Fatalf("reverse=%v/%v", ok, err)
						}
						if got, err := dr.GetRealtimeUsec(); err != nil || got != want {
							t.Fatalf("reverse time=%d/%v, want %d", got, err, want)
						}
					})
				}
			}
		}
	}
}

func TestFlushMatchesClearsSelectedRow(t *testing.T) {
	dir := directoryFilterBoundaryFixture(t, false, false)
	for _, directory := range []bool{false, true} {
		for _, facadeAPI := range []bool{false, true} {
			t.Run(fmt.Sprintf("directory=%v/facade=%v", directory, facadeAPI), func(t *testing.T) {
				var reader sdReader
				var step interface {
					Step() (bool, error)
					StepBack() (bool, error)
				}
				if directory {
					dr, err := OpenDirectory(dir)
					if err != nil {
						t.Fatal(err)
					}
					reader, step = dr, dr
				} else {
					r, err := OpenFile(filepath.Join(dir, "0.journal"))
					if err != nil {
						t.Fatal(err)
					}
					reader, step = r, r
				}
				defer reader.Close()
				reader.AddMatch([]byte("COMMON=all"))
				var facade *sdJournal
				if facadeAPI {
					facade = newSdJournal(reader)
				}
				if ok, err := positionTestStep(step, facade, false); err != nil || !ok {
					t.Fatalf("first: %v/%v", ok, err)
				}
				if _, err := reader.GetEntry(); err != nil {
					t.Fatal(err)
				}
				if facade != nil {
					if err := SdJournalFlushMatches(facade); err != nil {
						t.Fatal(err)
					}
				} else {
					reader.FlushMatches()
				}
				if _, err := reader.GetRealtimeUsec(); err == nil {
					t.Fatal("flushed reader retains timestamp")
				}
				if _, err := reader.GetCursor(); err == nil {
					t.Fatal("flushed reader retains cursor")
				}
				if _, err := reader.GetEntry(); err == nil {
					t.Fatal("flushed reader retains entry")
				}
				if ok, err := positionTestStep(step, facade, false); err != nil || !ok {
					t.Fatalf("rewound: %v/%v", ok, err)
				}
				if got, err := reader.GetRealtimeUsec(); err != nil || got != 1000 {
					t.Fatalf("rewound time=%d/%v", got, err)
				}
			})
		}
	}
}
