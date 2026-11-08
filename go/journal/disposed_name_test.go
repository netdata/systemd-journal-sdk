package journal

import (
	"strconv"
	"strings"
	"testing"
	"time"
)

// systemd names a disposed file "<stem>@<CLOCK_REALTIME usec>-<random>.journal~"
// (src/libsystemd/sd-journal/journal-file.c journal_file_dispose).
func TestDisposedJournalPathUsesMicroseconds(t *testing.T) {
	before := uint64(time.Now().UnixMicro())
	name := disposedJournalPath("/dir/system.journal", 0)
	after := uint64(time.Now().UnixMicro())

	rest, ok := strings.CutPrefix(name, "/dir/system@")
	if !ok || !strings.HasSuffix(rest, ".journal~") {
		t.Fatalf("unexpected disposed name %q", name)
	}
	stamp, err := strconv.ParseUint(strings.SplitN(rest, "-", 2)[0], 16, 64)
	if err != nil {
		t.Fatal(err)
	}
	if stamp < before || stamp > after {
		t.Fatalf("timestamp %d outside [%d, %d] microseconds", stamp, before, after)
	}
}
