package journal

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"unsafe"
)

type recoveryEOFCountingAccessor struct {
	readerAccessor
	reads int
}

func (a *recoveryEOFCountingAccessor) readAt(dst []byte, offset uint64) error {
	a.reads++
	return a.readerAccessor.readAt(dst, offset)
}

func TestRecoveryEOFSkipsUnchangedPrefix(t *testing.T) {
	source, arrays := recoveryTestFixture(t, false)
	last := arrays[len(arrays)-1].entries[len(arrays[len(arrays)-1].entries)-1]
	for _, mode := range readerTestAccessModes() {
		for _, filtered := range []bool{false, true} {
			t.Run(fmt.Sprintf("%d/%v", mode, filtered), func(t *testing.T) {
				path := filepath.Join(t.TempDir(), "system.journal")
				damaged := bytes.Clone(source)
				clear(damaged[last : last+16])
				if err := os.WriteFile(path, damaged, 0o600); err != nil {
					t.Fatal(err)
				}
				r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode))
				if err != nil {
					t.Fatal(err)
				}
				t.Cleanup(func() { r.Close() })
				if filtered {
					r.AddMatch([]byte("COMMON=all"))
				}
				if got := len(recoveryTestTimes(t, r, false)); got != 1004 {
					t.Fatalf("prefix=%d", got)
				}
				probe := &recoveryEOFCountingAccessor{readerAccessor: r.accessor}
				r.accessor = probe
				for i := 0; i < 10; i++ {
					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("EOF=%v/%v", ok, err)
					}
				}
				if probe.reads > 10 {
					t.Fatalf("unchanged EOF reread prefix: %d reads", probe.reads)
				}
				allocs := testing.AllocsPerRun(10, func() {
					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("EOF=%v/%v", ok, err)
					}
				})
				t.Logf("EOF allocations/poll=%g; Reader=%d bytes; recovery state=%d bytes", allocs, unsafe.Sizeof(Reader{}), unsafe.Sizeof(readerRecoveryState{}))
				after, err := os.ReadFile(path)
				if err != nil || !bytes.Equal(after, damaged) {
					t.Fatalf("reader changed source: %v", err)
				}
				if ok, err := r.StepBack(); err != nil || !ok {
					t.Fatalf("reverse=%v/%v", ok, err)
				}
				// Restoration is synthetic; the following real append changes the
				// declared population, which must invalidate the terminal memo.
				if got := recoveryTestTimes(t, r, false); filtered && len(got) != 1 {
					t.Fatalf("forward after reverse=%v", got)
				}
				if err := os.WriteFile(path, source, 0o600); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("same-count restoration=%v/%v", ok, err)
				}
				w, err := OpenWithOptions(path, testOptions())
				if err != nil {
					t.Fatal(err)
				}
				if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: 3000, MonotonicUsec: 3000}); err != nil {
					w.Close()
					t.Fatal(err)
				}
				if err := w.Close(); err != nil {
					t.Fatal(err)
				}
				if got := recoveryTestTimes(t, r, false); len(got) != 2 || got[0] != 2004 || got[1] != 3000 {
					t.Fatalf("changed-count rows=%v", got)
				}
			})
		}
	}
}
