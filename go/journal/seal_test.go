package journal

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func testSealOpts() *SealOptions {
	return &SealOptions{
		Seed:         make([]byte, 12),
		IntervalUsec: 1000000,
		StartUsec:    1000000,
	}
}

func testVerificationKey(opts *SealOptions) string {
	// Format: 24-hex-seed / start-interval
	seedHex := fmt.Sprintf("%024x", opts.Seed)
	start := opts.StartUsec / opts.IntervalUsec
	return fmt.Sprintf("%s/%x-%x", seedHex, start, opts.IntervalUsec)
}

func TestWriterSealedBasic(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}

	if err := w.Append([]Field{
		StringField("MESSAGE", "hello sealed world"),
		StringField("PRIORITY", "6"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	cmd := exec.Command("journalctl", "--verify", "--verify-key", key, "--file", path)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl verify failed: %v\n%s", err, out)
	}
	t.Logf("journalctl verify output:\n%s", out)
}

func TestWriterSealedIntervalCrossing(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}

	// Entry in epoch 0 (realtime == start)
	if err := w.Append([]Field{
		StringField("MESSAGE", "epoch0"),
	}, EntryOptions{RealtimeUsec: 1000000}); err != nil {
		t.Fatalf("append epoch0: %v", err)
	}

	// Entry in epoch 1 (crosses interval)
	if err := w.Append([]Field{
		StringField("MESSAGE", "epoch1"),
	}, EntryOptions{RealtimeUsec: 2000000}); err != nil {
		t.Fatalf("append epoch1: %v", err)
	}

	// Entry in epoch 2
	if err := w.Append([]Field{
		StringField("MESSAGE", "epoch2"),
	}, EntryOptions{RealtimeUsec: 3000000}); err != nil {
		t.Fatalf("append epoch2: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	cmd := exec.Command("journalctl", "--verify", "--verify-key", key, "--file", path)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl verify failed: %v\n%s", err, out)
	}
	t.Logf("journalctl verify output:\n%s", out)
}

func TestWriterSealedWrongKeyFails(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}

	if err := w.Append([]Field{
		StringField("MESSAGE", "hello"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	wrongKey := "000000000000000000000001/1-f4240"
	cmd := exec.Command("journalctl", "--verify", "--verify-key", wrongKey, "--file", path)
	out, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("expected journalctl verify to fail with wrong key, got:\n%s", out)
	}
	t.Logf("journalctl wrong-key output (expected failure):\n%s", out)
}

func TestWriterSealedTamperedDataFails(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}

	if err := w.Append([]Field{
		StringField("MESSAGE", "hello"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	// Tamper with a byte in the DATA object payload area
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		t.Fatalf("open for tamper: %v", err)
	}
	// Flip a bit somewhere past the header + data object header
	_, _ = f.WriteAt([]byte{0xff}, 512)
	f.Close()

	key := testVerificationKey(opts.Seal)
	cmd := exec.Command("journalctl", "--verify", "--verify-key", key, "--file", path)
	out, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("expected journalctl verify to fail with tampered data, got:\n%s", out)
	}
	t.Logf("journalctl tamper output (expected failure):\n%s", out)
}

func TestWriterUnsealedDoesNotSetFlags(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("create unsealed writer: %v", err)
	}
	if w.header.compatibleFlags&compatibleSealed != 0 {
		t.Fatalf("unsealed writer set SEALED flag")
	}
	if w.header.compatibleFlags&compatibleSealedContinuous != 0 {
		t.Fatalf("unsealed writer set SEALED_CONTINUOUS flag")
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}
}

func TestWriterCompactSealedStockVerify(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts(), Compact: true}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create compact sealed writer: %v", err)
	}

	if err := w.Append([]Field{
		StringField("MESSAGE", "compact sealed"),
		StringField("PRIORITY", "6"),
	}, EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("append entry: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	cmd := exec.Command("journalctl", "--verify", "--verify-key", key, "--file", path)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl verify compact+sealed failed: %v\n%s", err, out)
	}
	t.Logf("journalctl verify compact+sealed output:\n%s", out)
}

func TestWriterSealedFirstEntryFutureEpoch(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}

	// Write the first entry at epoch 2 (realtime = start + 2 * interval = 3_000_000).
	// This exercises FSS epoch-evolution during the first-tag path.
	if err := w.Append([]Field{
		StringField("MESSAGE", "future epoch first entry"),
	}, EntryOptions{RealtimeUsec: 3000000}); err != nil {
		t.Fatalf("append future-epoch first entry: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	cmd := exec.Command("journalctl", "--verify", "--verify-key", key, "--file", path)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl verify first-entry future-epoch failed: %v\n%s", err, out)
	}
	t.Logf("journalctl verify first-entry future-epoch output:\n%s", out)
}

func TestWriterSealedEntryBeforeStartRejected(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}

	// Stock verification rejects entries older than the first tag epoch, so
	// writers must reject this input instead of producing an invalid file.
	if err := w.Append([]Field{
		StringField("MESSAGE", "before sealing start"),
	}, EntryOptions{RealtimeUsec: 500000}); err == nil {
		t.Fatalf("expected before-start entry to be rejected")
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}
}

func TestWriterSealedMultiIntervalGap(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}

	if err := w.Append([]Field{
		StringField("MESSAGE", "epoch0"),
	}, EntryOptions{RealtimeUsec: 1000000}); err != nil {
		t.Fatalf("append epoch0: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "epoch5"),
	}, EntryOptions{RealtimeUsec: 6000000}); err != nil {
		t.Fatalf("append epoch5: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	cmd := exec.Command("journalctl", "--verify", "--verify-key", key, "--file", path)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl verify multi-interval gap failed: %v\n%s", err, out)
	}
	t.Logf("journalctl verify multi-interval gap output:\n%s", out)
}

func TestWriterSealedEmptyFileStockVerify(t *testing.T) {
	requireJournalctl(t)

	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	opts := Options{Seal: testSealOpts()}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("create sealed writer: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	key := testVerificationKey(opts.Seal)
	cmd := exec.Command("journalctl", "--verify", "--verify-key", key, "--file", path)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl verify empty sealed file failed: %v\n%s", err, out)
	}
	t.Logf("journalctl verify empty sealed file output:\n%s", out)
}
