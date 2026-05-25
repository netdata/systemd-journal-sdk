package journal

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestVerifyFileDetectsCorruption(t *testing.T) {
	path := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "corrupted", "zstd-truncated-frame.zst")
	err := VerifyFile(path)
	if err == nil {
		t.Fatal("expected verification error for truncated zstd frame, got nil")
	}
	if !strings.Contains(err.Error(), "corrupt") {
		t.Fatalf("expected error to contain 'corrupt', got: %v", err)
	}
}

func TestVerifyFilePassesOnValidFixture(t *testing.T) {
	path := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "no-rtc", "system.journal.zst")
	err := VerifyFile(path)
	if err != nil {
		t.Fatalf("expected verification to pass for valid fixture, got: %v", err)
	}
}
