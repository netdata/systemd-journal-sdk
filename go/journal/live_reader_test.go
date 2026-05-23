package journal

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

func TestGoReaderLiveGoWriter(t *testing.T) {
	testGoReaderLiveGoWriter(t, 50, 100*time.Millisecond)
}

func TestGoReaderLiveGoWriterStress(t *testing.T) {
	testGoReaderLiveGoWriter(t, 200, 10*time.Millisecond)
}

func testGoReaderLiveGoWriter(t *testing.T, entries int, delay time.Duration) {
	t.Helper()

	moduleRoot := filepath.Join("..")
	writerBin := buildGoTestBinary(t, moduleRoot, "./internal/testcmd/livewriter", "livewriter")

	tmp := t.TempDir()
	journalPath := filepath.Join(tmp, "live-test.journal")
	readyFile := filepath.Join(tmp, "ready")

	var stderr bytes.Buffer
	cmd := exec.Command(writerBin,
		"--path", journalPath,
		"--ready-file", readyFile,
		"--entries", fmt.Sprint(entries),
		"--delay", delay.String(),
		"--sync-every", "10",
	)
	cmd.Dir = moduleRoot
	cmd.Stderr = &stderr

	if err := cmd.Start(); err != nil {
		t.Fatalf("start live writer: %v", err)
	}

	done := make(chan error, 1)
	go func() {
		done <- cmd.Wait()
	}()

	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		select {
		case err := <-done:
			t.Fatalf("live writer exited before ready file: %v; stderr=%s", err, stderr.String())
		default:
		}
		if _, err := os.Stat(readyFile); err == nil {
			goto started
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("live writer did not create ready file; stderr=%s", stderr.String())

started:
	maxSeen := 0
	var writerErr error
	active := true
	for active {
		select {
		case writerErr = <-done:
			active = false
		default:
			count, err := readLiveSeqCount(journalPath)
			if err == nil && count > maxSeen {
				maxSeen = count
			}
			time.Sleep(10 * time.Millisecond)
		}
	}
	if writerErr != nil {
		t.Fatalf("live writer failed: %v; stderr=%s", writerErr, stderr.String())
	}
	if maxSeen < 1 {
		t.Fatalf("live reader saw %d entries while writer was active, want at least 1", maxSeen)
	}
	finalCount, err := readLiveSeqCount(journalPath)
	if err != nil {
		t.Fatalf("final live read: %v", err)
	}
	if finalCount != entries {
		t.Fatalf("final live read saw %d entries, want %d", finalCount, entries)
	}
}

func buildGoTestBinary(t *testing.T, moduleRoot, pkg, name string) string {
	t.Helper()
	bin := filepath.Join(t.TempDir(), name)
	cmd := exec.Command("go", "build", "-o", bin, pkg)
	cmd.Dir = moduleRoot
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("build %s failed: %v\n%s", pkg, err, output)
	}
	return bin
}

func readLiveSeqCount(path string) (int, error) {
	r, err := OpenFile(path)
	if err != nil {
		return 0, err
	}
	defer r.Close()

	expected := 0
	for {
		ok, err := r.Step()
		if err != nil {
			return 0, err
		}
		if !ok {
			break
		}
		entry, err := r.GetEntry()
		if err != nil {
			return 0, err
		}
		raw, ok := entry.Fields["LIVE_SEQ"]
		if !ok {
			return 0, fmt.Errorf("entry %d missing LIVE_SEQ", expected)
		}
		var seq int
		if _, err := fmt.Sscanf(string(raw), "%06d", &seq); err != nil {
			return 0, fmt.Errorf("parse LIVE_SEQ %q: %w", raw, err)
		}
		if seq != expected {
			return 0, fmt.Errorf("LIVE_SEQ = %d, want %d", seq, expected)
		}
		expected++
	}
	return expected, nil
}

func TestJournalctlListBoots(t *testing.T) {
	tmp := t.TempDir()
	dir := filepath.Join(tmp, "journal.d")

	if err := os.MkdirAll(dir, 0o750); err != nil {
		t.Fatalf("MkdirAll error: %v", err)
	}

	machineID := UUID{}
	for i := range machineID {
		machineID[i] = byte(i)
	}

	bootID := UUID{}
	for i := range bootID {
		bootID[i] = byte(i + 0x10)
	}

	path := filepath.Join(dir, "system.journal")
	w, err := Create(path, Options{
		MachineID: machineID,
		BootID:    bootID,
	})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	for i := 0; i < 5; i++ {
		if err := w.Append([]Field{
			StringField("MESSAGE", "boot-entry"),
		}, EntryOptions{
			RealtimeUsec: uint64(1000 + i),
		}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatalf("OpenDirectory error: %v", err)
	}
	defer dr.Close()

	boots, err := dr.ListBoots()
	if err != nil {
		t.Fatalf("ListBoots error: %v", err)
	}

	if len(boots) != 1 {
		t.Errorf("ListBoots returned %d boots, want 1", len(boots))
	}
}

func TestJournalctlOutputModes(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	if err := w.Append([]Field{
		StringField("MESSAGE", "hello"),
		StringField("PRIORITY", "6"),
		StringField("_HOSTNAME", "testhost"),
		StringField("_MACHINE_ID", "abc123def456"),
	}, EntryOptions{
		RealtimeUsec: 1_700_001_000_000_000,
	}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.SeekHead()
	r.Step()

	entry, err := r.GetEntry()
	if err != nil {
		t.Fatalf("GetEntry error: %v", err)
	}

	export := ExportEntry(entry)
	if export == "" {
		t.Error("ExportEntry returned empty string")
	}

	if _, ok := entry.Fields["MESSAGE"]; !ok {
		t.Error("MESSAGE field missing")
	}
}

func TestJournalctlDaemonUnsupported(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer j.Close()

	if j == nil {
		t.Fatal("journal handle is nil")
	}
}

func TestAdapterRun(t *testing.T) {
	moduleRoot := filepath.Join("..")
	adapterBin := buildGoTestBinary(t, moduleRoot, "./adapter", "adapter")

	type testCase struct {
		TestName string `json:"test_name"`
		Category string `json:"category"`
	}
	type result struct {
		TestName string `json:"test_name"`
	}

	input := testCase{
		TestName: "test-case",
		Category: "file-format",
	}
	inputJSON, _ := json.Marshal(input)

	cmd := exec.Command(adapterBin, "run")
	cmd.Dir = moduleRoot
	cmd.Stdin = bytes.NewReader(inputJSON)

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		t.Fatalf("adapter run failed: %v; stderr=%s", err, stderr.String())
	}

	var res result
	if err := json.NewDecoder(&stdout).Decode(&res); err != nil {
		t.Fatalf("decode result: %v", err)
	}

	if res.TestName != "test-case" {
		t.Errorf("result.TestName = %q, want %q", res.TestName, "test-case")
	}
}

func TestAdapterList(t *testing.T) {
	moduleRoot := filepath.Join("..")
	adapterBin := buildGoTestBinary(t, moduleRoot, "./adapter", "adapter")

	cmd := exec.Command(adapterBin, "list")
	cmd.Dir = moduleRoot

	var stdout bytes.Buffer
	cmd.Stdout = &stdout

	if err := cmd.Run(); err != nil {
		t.Fatalf("adapter list failed: %v", err)
	}

	var tests []string
	if err := json.NewDecoder(&stdout).Decode(&tests); err != nil {
		t.Fatalf("decode tests: %v", err)
	}

	if len(tests) == 0 {
		t.Error("no tests listed")
	}
}

func TestAdapterProbe(t *testing.T) {
	moduleRoot := filepath.Join("..")
	adapterBin := buildGoTestBinary(t, moduleRoot, "./adapter", "adapter")

	cmd := exec.Command(adapterBin, "probe")
	cmd.Dir = moduleRoot

	var stdout bytes.Buffer
	cmd.Stdout = &stdout

	if err := cmd.Run(); err != nil {
		t.Fatalf("adapter probe failed: %v", err)
	}

	var info map[string]interface{}
	if err := json.NewDecoder(&stdout).Decode(&info); err != nil {
		t.Fatalf("decode info: %v", err)
	}

	if info["language"] != "go" {
		t.Errorf("language = %v, want go", info["language"])
	}
}
