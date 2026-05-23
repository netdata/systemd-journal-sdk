package journal

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestGoWriterLiveStockReaders(t *testing.T) {
	testGoWriterLiveStockReaders(t, "live-stock-readers.journal", 600, time.Millisecond, 25, 30*time.Second)
}

func TestGoWriterLiveStockReadersStress(t *testing.T) {
	testGoWriterLiveStockReaders(t, "live-stock-readers-stress.journal", 1200, 0, 1, 45*time.Second)
}

func testGoWriterLiveStockReaders(t *testing.T, filename string, entries int, delay time.Duration, syncEvery int, timeout time.Duration) {
	t.Helper()
	requireLiveTools(t)

	repoRoot, moduleRoot := repositoryRoots(t)
	readerBin := buildLibsystemdLiveReader(t, repoRoot)
	tmp := t.TempDir()
	journalPath := filepath.Join(tmp, filename)
	readyFile := filepath.Join(tmp, "ready")

	output := runLiveHarness(t, moduleRoot, repoRoot, liveHarnessOptions{
		JournalPath:       journalPath,
		ReadyFile:         readyFile,
		ExpectedEntries:   entries,
		LibsystemdReader:  readerBin,
		PollReaders:       2,
		FollowReaders:     1,
		LibsystemdReaders: 1,
		ReaderTimeout:     timeout,
		WriterTimeout:     timeout,
		WriterEntries:     entries,
		WriterDelay:       delay,
		WriterSyncEvery:   syncEvery,
		AllowedWriterExit: 0,
		SkipVerify:        false,
	})
	assertLiveHarnessSummary(t, output, entries)
}

func TestGoWriterLiveInterruptionReopenAndVerify(t *testing.T) {
	requireLiveTools(t)

	repoRoot, moduleRoot := repositoryRoots(t)
	readerBin := buildLibsystemdLiveReader(t, repoRoot)
	tmp := t.TempDir()
	journalPath := filepath.Join(tmp, "live-interrupted.journal")
	readyFile := filepath.Join(tmp, "ready")

	output := runLiveHarness(t, moduleRoot, repoRoot, liveHarnessOptions{
		JournalPath:       journalPath,
		ReadyFile:         readyFile,
		ExpectedEntries:   160,
		LibsystemdReader:  readerBin,
		PollReaders:       2,
		FollowReaders:     1,
		LibsystemdReaders: 1,
		ReaderTimeout:     30 * time.Second,
		WriterTimeout:     30 * time.Second,
		WriterEntries:     300,
		WriterDelay:       time.Millisecond,
		WriterSyncEvery:   20,
		WriterCrashAfter:  160,
		AllowedWriterExit: 17,
		SkipVerify:        true,
	})
	assertLiveHarnessSummary(t, output, 160)
	verifyJournalctl(t, journalPath)

	w, err := Open(journalPath)
	if err != nil {
		t.Fatalf("Open(interrupted journal) error = %v", err)
	}
	const realtimeBase = uint64(1_700_001_000_000_000)
	for i := 160; i < 240; i++ {
		if err := w.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("live-%06d", i)),
			StringField("PRIORITY", "6"),
			StringField("SYSLOG_IDENTIFIER", "go-live-writer"),
			StringField("LIVE_SEQ", fmt.Sprintf("%06d", i)),
		}, EntryOptions{
			RealtimeUsec:  realtimeBase + uint64(i),
			MonotonicUsec: uint64(i + 1),
		}); err != nil {
			_ = w.Close()
			t.Fatalf("Append(after interruption %d) error = %v", i, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close(after interruption) error = %v", err)
	}

	verifyJournalctl(t, journalPath)
	if got := runJournalctlLineCount(t, journalPath, "PRIORITY=6"); got != 240 {
		t.Fatalf("reopened interrupted journal row count = %d, want 240", got)
	}
}

func TestGoWriterLiveRejectsSecondWriter(t *testing.T) {
	requireLiveTools(t)

	_, moduleRoot := repositoryRoots(t)
	writerBin := buildGoLiveWriter(t, moduleRoot)
	tmp := t.TempDir()
	journalPath := filepath.Join(tmp, "live-single-writer.journal")
	readyFile := filepath.Join(tmp, "ready")

	var stdout lockedBuffer
	var stderr lockedBuffer
	cmd := exec.Command(
		writerBin,
		"--path", journalPath,
		"--ready-file", readyFile,
		"--entries", "1000",
		"--delay", "5ms",
		"--sync-every", "10",
	)
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Start(); err != nil {
		t.Fatalf("start live writer: %v", err)
	}

	done := make(chan error, 1)
	go func() {
		done <- cmd.Wait()
	}()
	defer func() {
		if cmd.ProcessState != nil {
			return
		}
		select {
		case <-done:
			return
		default:
		}
		_ = cmd.Process.Kill()
		select {
		case <-done:
		case <-time.After(5 * time.Second):
			t.Fatalf("live writer did not exit after targeted kill; stdout=%s stderr=%s", stdout.String(), stderr.String())
		}
	}()

	waitForReadyFile(t, readyFile, done, stdout.String, stderr.String)
	w, err := Open(journalPath)
	if err == nil {
		_ = w.Close()
		t.Fatalf("Open succeeded while the live writer held the journal lock")
	}
}

type lockedBuffer struct {
	mu  sync.Mutex
	buf bytes.Buffer
}

func (b *lockedBuffer) Write(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.Write(p)
}

func (b *lockedBuffer) String() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.String()
}

type liveHarnessOptions struct {
	JournalPath       string
	ReadyFile         string
	ExpectedEntries   int
	LibsystemdReader  string
	PollReaders       int
	FollowReaders     int
	LibsystemdReaders int
	ReaderTimeout     time.Duration
	WriterTimeout     time.Duration
	WriterEntries     int
	WriterDelay       time.Duration
	WriterSyncEvery   int
	WriterCrashAfter  int
	AllowedWriterExit int
	SkipVerify        bool
}

func requireLiveTools(t *testing.T) {
	t.Helper()
	for _, tool := range []string{"go", "python3", "journalctl", "pkg-config", "cc"} {
		if _, err := exec.LookPath(tool); err != nil {
			t.Skipf("%s is required for live concurrency tests: %v", tool, err)
		}
	}
}

func waitForReadyFile(t *testing.T, readyFile string, done <-chan error, stdout, stderr func() string) {
	t.Helper()

	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		select {
		case err := <-done:
			t.Fatalf("live writer exited before ready file: %v; stdout=%s stderr=%s", err, stdout(), stderr())
		default:
		}
		if _, err := os.Stat(readyFile); err == nil {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("live writer did not create ready file; stdout=%s stderr=%s", stdout(), stderr())
}

func repositoryRoots(t *testing.T) (string, string) {
	t.Helper()

	cwd, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd() error = %v", err)
	}
	moduleRoot := filepath.Clean(filepath.Join(cwd, ".."))
	repoRoot := filepath.Clean(filepath.Join(moduleRoot, ".."))
	return repoRoot, moduleRoot
}

func buildLibsystemdLiveReader(t *testing.T, repoRoot string) string {
	t.Helper()

	cflagsOutput, err := exec.Command("pkg-config", "--cflags", "libsystemd").Output()
	if err != nil {
		t.Skipf("libsystemd pkg-config metadata is required: %v", err)
	}
	libsOutput, err := exec.Command("pkg-config", "--libs", "libsystemd").Output()
	if err != nil {
		t.Skipf("libsystemd pkg-config metadata is required: %v", err)
	}

	source := filepath.Join(repoRoot, "tests", "conformance", "live", "libsystemd_live_reader.c")
	output := filepath.Join(t.TempDir(), "libsystemd_live_reader")
	args := append(strings.Fields(string(cflagsOutput)), source, "-o", output)
	args = append(args, strings.Fields(string(libsOutput))...)
	cmd := exec.Command("cc", args...)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Skipf("libsystemd live reader build failed: %v\n%s", err, out)
	}
	return output
}

func runLiveHarness(t *testing.T, moduleRoot, repoRoot string, opts liveHarnessOptions) []byte {
	t.Helper()

	writerBin := buildGoLiveWriter(t, moduleRoot)
	harness := filepath.Join(repoRoot, "tests", "conformance", "live", "run_live_concurrency.py")
	args := []string{
		harness,
		"--journal", opts.JournalPath,
		"--ready-file", opts.ReadyFile,
		"--expected-entries", fmt.Sprint(opts.ExpectedEntries),
		"--poll-journalctl-readers", fmt.Sprint(opts.PollReaders),
		"--follow-journalctl-readers", fmt.Sprint(opts.FollowReaders),
		"--libsystemd-readers", fmt.Sprint(opts.LibsystemdReaders),
		"--libsystemd-reader-bin", opts.LibsystemdReader,
		"--reader-timeout-sec", fmt.Sprintf("%.3f", opts.ReaderTimeout.Seconds()),
		"--writer-timeout-sec", fmt.Sprintf("%.3f", opts.WriterTimeout.Seconds()),
		"--allowed-writer-exit-code", fmt.Sprint(opts.AllowedWriterExit),
	}
	if opts.SkipVerify {
		args = append(args, "--skip-verify")
	}
	args = append(args,
		"--",
		writerBin,
		"--path", opts.JournalPath,
		"--ready-file", opts.ReadyFile,
		"--entries", fmt.Sprint(opts.WriterEntries),
		"--delay", opts.WriterDelay.String(),
		"--sync-every", fmt.Sprint(opts.WriterSyncEvery),
	)
	if opts.WriterCrashAfter > 0 {
		args = append(args, "--crash-after", fmt.Sprint(opts.WriterCrashAfter))
	}

	ctx, cancel := context.WithTimeout(context.Background(), opts.WriterTimeout+opts.ReaderTimeout+20*time.Second)
	defer cancel()

	cmd := exec.CommandContext(ctx, "python3", args...)
	cmd.Dir = moduleRoot
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("live concurrency harness failed: %v\n%s", err, output)
	}
	return output
}

func buildGoLiveWriter(t *testing.T, moduleRoot string) string {
	t.Helper()

	output := filepath.Join(t.TempDir(), "go-live-writer")
	cmd := exec.Command("go", "build", "-o", output, "./internal/testcmd/livewriter")
	cmd.Dir = moduleRoot
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build go live writer failed: %v\n%s", err, out)
	}
	return output
}

func assertLiveHarnessSummary(t *testing.T, output []byte, expectedEntries int) {
	t.Helper()

	var summary struct {
		SystemdVersion  string `json:"systemd_version"`
		ExpectedEntries int    `json:"expected_entries"`
		Readers         []struct {
			Reader     string `json:"reader"`
			MaxEntries int    `json:"max_entries"`
			Entries    int    `json:"entries"`
		} `json:"readers"`
	}
	if err := json.Unmarshal(output, &summary); err != nil {
		t.Fatalf("json.Unmarshal(live harness output) error = %v\n%s", err, output)
	}
	if summary.SystemdVersion == "" {
		t.Fatalf("live harness did not record systemd version: %s", output)
	}
	if summary.ExpectedEntries != expectedEntries {
		t.Fatalf("expected_entries = %d, want %d", summary.ExpectedEntries, expectedEntries)
	}
	if len(summary.Readers) == 0 {
		t.Fatalf("live harness returned no reader evidence: %s", output)
	}
	for i, reader := range summary.Readers {
		observed := reader.Entries
		if observed == 0 {
			observed = reader.MaxEntries
		}
		if observed < expectedEntries {
			t.Fatalf("reader %d (%s) observed %d entries, want at least %d; output=%s", i, reader.Reader, observed, expectedEntries, output)
		}
	}
}
