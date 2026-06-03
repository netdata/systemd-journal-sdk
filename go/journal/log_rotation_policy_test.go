package journal

import (
	"fmt"
	"strings"
	"testing"
	"time"
)

func TestLogDerivesRotationDefaultsFromRetention(t *testing.T) {
	maxSize := uint64(128 * 1024 * 1024)
	maxAge := 20*time.Second + time.Microsecond
	options := testOptions()
	options.DataHashTableBuckets = 0
	options.FieldHashTableBuckets = 0
	log, _ := newTestLog(t, LogConfig{
		Options: options,
		RetentionPolicy: RetentionPolicy{}.
			WithMaxBytes(maxSize * 20).
			WithMaxAge(maxAge),
	})
	defer log.Close()

	if log.rotation.MaxFileSize == nil || *log.rotation.MaxFileSize != maxSize {
		t.Fatalf("derived max file size = %v, want %d", log.rotation.MaxFileSize, maxSize)
	}
	if want := time.Second + time.Microsecond; log.rotation.MaxDuration == nil || *log.rotation.MaxDuration != want {
		t.Fatalf("derived max duration = %v, want %s", log.rotation.MaxDuration, want)
	}
	if got := log.options.DataHashTableBuckets; got != dataHashBucketsForMaxFileSize(maxSize) {
		t.Fatalf("data hash buckets = %d, want %d", got, dataHashBucketsForMaxFileSize(maxSize))
	}
	if got := log.options.FieldHashTableBuckets; got != defaultFieldHashBuckets {
		t.Fatalf("field hash buckets = %d, want %d", got, defaultFieldHashBuckets)
	}
}

func TestLogDerivedSizeRotationFromRetention(t *testing.T) {
	requireJournalctl(t)

	maxSize := uint64(16 * 1024 * 1024)
	options := testOptions()
	options.DataHashTableBuckets = 0
	options.FieldHashTableBuckets = 0
	log, dir := newTestLog(t, LogConfig{
		Options:         options,
		Source:          "system",
		RetentionPolicy: RetentionPolicy{}.WithMaxBytes(maxSize * 20),
	})

	for i := 0; i < 12; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("derived-size-rotation-%d", i)),
			StringField("PAYLOAD", fmt.Sprintf("%05d-%s", i, strings.Repeat("x", 2*1024*1024))),
			StringField("TEST_ID", "derived-size-rotation"),
		}, EntryOptions{RealtimeUsec: 1_700_002_092_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) < 2 {
		t.Fatalf("derived size rotation files = %d, want at least 2; files=%v", len(files), files)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=derived-size-rotation")
	if len(rows) != 12 {
		t.Fatalf("derived size rotation row count = %d, want 12", len(rows))
	}
	for _, path := range files {
		snapshot := readJournalSnapshot(t, path)
		if snapshot.header.dataHashTableSize/hashItemSize != uint64(dataHashBucketsForMaxFileSize(maxSize)) {
			t.Fatalf("%s data hash buckets = %d, want %d", path, snapshot.header.dataHashTableSize/hashItemSize, dataHashBucketsForMaxFileSize(maxSize))
		}
	}
}

func TestLogDerivedDurationRotationFromRetention(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
		RetentionPolicy: RetentionPolicy{}.
			WithMaxAge(20*time.Second + time.Microsecond),
	})

	base := uint64(time.Now().UnixMicro())
	for i, realtime := range []uint64{base, base + 1_000_000, base + 1_000_001} {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("derived-duration-rotation-%d", i)),
			StringField("TEST_ID", "derived-duration-rotation"),
		}, EntryOptions{RealtimeUsec: realtime, MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 2 {
		t.Fatalf("derived duration rotation files = %d, want 2; files=%v", len(files), files)
	}
	counts := make([]uint64, 0, len(files))
	for _, path := range files {
		counts = append(counts, readJournalSnapshot(t, path).header.nEntries)
	}
	if got, want := counts, []uint64{2, 1}; !equalUint64s(got, want) {
		t.Fatalf("derived duration rotation counts = %v, want %v", got, want)
	}
}

func TestLogDerivedRotationSmallRetentionClampsToMinimum(t *testing.T) {
	log, _ := newTestLog(t, LogConfig{
		Options: testOptions(),
		RetentionPolicy: RetentionPolicy{}.
			WithMaxBytes(1_000_000),
	})
	defer log.Close()

	if log.rotation.MaxFileSize == nil || *log.rotation.MaxFileSize != journalFileSizeMin {
		t.Fatalf("small-retention derived max file size = %v, want %d", log.rotation.MaxFileSize, journalFileSizeMin)
	}
}

func TestLogDerivedRotationCompactMaxFileSizeClamp(t *testing.T) {
	options := testOptions()
	options.Compact = true
	options.DataHashTableBuckets = 0
	options.FieldHashTableBuckets = 0
	log, _ := newTestLog(t, LogConfig{
		Options: options,
		RetentionPolicy: RetentionPolicy{}.
			WithMaxBytes((journalCompactSizeMax + pageSize) * 20),
	})
	defer log.Close()

	if log.rotation.MaxFileSize == nil || *log.rotation.MaxFileSize != journalCompactSizeMax {
		t.Fatalf("compact derived max file size = %v, want %d", log.rotation.MaxFileSize, journalCompactSizeMax)
	}
}

func TestLogExplicitRotationOverridesRetentionDerivedDefaults(t *testing.T) {
	explicitSize := uint64(64 * 1024 * 1024)
	explicitDuration := 2 * time.Second
	options := testOptions()
	options.DataHashTableBuckets = 0
	options.FieldHashTableBuckets = 0
	log, _ := newTestLog(t, LogConfig{
		Options: options,
		RotationPolicy: RotationPolicy{}.
			WithMaxFileSize(explicitSize).
			WithMaxDuration(explicitDuration),
		RetentionPolicy: RetentionPolicy{}.
			WithMaxBytes(uint64(128*1024*1024) * 20).
			WithMaxAge(20 * time.Second),
	})
	defer log.Close()

	if log.rotation.MaxFileSize == nil || *log.rotation.MaxFileSize != explicitSize {
		t.Fatalf("explicit max file size = %v, want %d", log.rotation.MaxFileSize, explicitSize)
	}
	if log.rotation.MaxDuration == nil || *log.rotation.MaxDuration != explicitDuration {
		t.Fatalf("explicit max duration = %v, want %s", log.rotation.MaxDuration, explicitDuration)
	}
	if got := log.options.DataHashTableBuckets; got != dataHashBucketsForMaxFileSize(explicitSize) {
		t.Fatalf("data hash buckets = %d, want %d", got, dataHashBucketsForMaxFileSize(explicitSize))
	}
}
