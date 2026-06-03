package journal

import (
	"bytes"
	"errors"
	"path/filepath"
	"strings"
	"testing"
)

func TestNewLogStrictIdentityRequiresMachineAndBootID(t *testing.T) {
	_, err := NewLog(t.TempDir(), LogConfig{IdentityMode: LogIdentityStrict})
	if !errors.Is(err, ErrInvalidJournal) {
		t.Fatalf("NewLog(strict identity without IDs) error = %v, want ErrInvalidJournal", err)
	}

	options := testOptions()
	log, err := NewLog(t.TempDir(), LogConfig{
		Options:      options,
		IdentityMode: LogIdentityStrict,
	})
	if err != nil {
		t.Fatalf("NewLog(strict identity with IDs) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestNewLogRejectsExplicitZeroPolicyLimits(t *testing.T) {
	if _, err := NewLog(t.TempDir(), LogConfig{
		Options:        testOptions(),
		RotationPolicy: RotationPolicy{}.WithMaxEntries(0),
	}); !errors.Is(err, ErrInvalidJournal) {
		t.Fatalf("NewLog(zero max entries) error = %v, want ErrInvalidJournal", err)
	}
	if _, err := NewLog(t.TempDir(), LogConfig{
		Options:         testOptions(),
		RetentionPolicy: RetentionPolicy{}.WithMaxBytes(0),
	}); !errors.Is(err, ErrInvalidJournal) {
		t.Fatalf("NewLog(zero max bytes) error = %v, want ErrInvalidJournal", err)
	}
}

func TestLogAppendAddsSourceRealtimeAndClampsEntryRealtime(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "source realtime one"),
		StringField("TEST_ID", "source-realtime-clamp"),
	}, EntryOptions{
		RealtimeUsec:       1_700_002_800_000_000,
		MonotonicUsec:      10,
		SourceRealtimeUsec: 1_600_000_000_000_000,
	}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("MESSAGE", "source realtime two"),
		StringField("TEST_ID", "source-realtime-clamp"),
	}, EntryOptions{
		RealtimeUsec:       1_700_002_799_999_000,
		MonotonicUsec:      5,
		SourceRealtimeUsec: 1_600_000_000_000_001,
	}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	verifyJournalctl(t, log.ActivePath())

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=source-realtime-clamp")
	if len(rows) != 2 {
		t.Fatalf("row count = %d, want 2", len(rows))
	}
	assertJSONField(t, rows[0], "_SOURCE_REALTIME_TIMESTAMP", "1600000000000000")
	assertJSONField(t, rows[1], "_SOURCE_REALTIME_TIMESTAMP", "1600000000000001")
	if got := parseU64JSONField(t, rows[1], "__REALTIME_TIMESTAMP"); got != 1_700_002_800_000_001 {
		t.Fatalf("second realtime = %d, want clamped 1700002800000001", got)
	}
	if got := parseU64JSONField(t, rows[1], "__MONOTONIC_TIMESTAMP"); got != 11 {
		t.Fatalf("second monotonic = %d, want clamped 11", got)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogExplicitZeroMonotonicOverrideIsClamped(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "zero monotonic one"),
		StringField("TEST_ID", "zero-monotonic-clamp"),
	}, EntryOptions{RealtimeUsec: 1_700_003_050_000_000, MonotonicUsec: 10}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("MESSAGE", "zero monotonic two"),
		StringField("TEST_ID", "zero-monotonic-clamp"),
	}, EntryOptions{
		RealtimeUsec:       1_700_003_050_000_001,
		MonotonicUsec:      0,
		MonotonicUsecSet:   true,
		SourceRealtimeUsec: 1_600_000_000_000_010,
	}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("MESSAGE", "zero realtime three"),
		StringField("TEST_ID", "zero-monotonic-clamp"),
	}, EntryOptions{
		RealtimeUsec:       0,
		RealtimeUsecSet:    true,
		MonotonicUsec:      12,
		SourceRealtimeUsec: 1_600_000_000_000_011,
	}); err != nil {
		t.Fatalf("Append(third) error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	verifyJournalctl(t, log.ActivePath())

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=zero-monotonic-clamp")
	if len(rows) != 3 {
		t.Fatalf("row count = %d, want 3", len(rows))
	}
	if got := parseU64JSONField(t, rows[1], "__MONOTONIC_TIMESTAMP"); got != 11 {
		t.Fatalf("second monotonic = %d, want explicit zero clamped to 11", got)
	}
	if got := parseU64JSONField(t, rows[2], "__REALTIME_TIMESTAMP"); got != 1_700_003_050_000_002 {
		t.Fatalf("third realtime = %d, want explicit zero clamped to 1700003050000002", got)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogDifferentBootDoesNotSeedMonotonicClampFromPreviousTail(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	bootA := UUID{0xaa, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1}
	bootB := UUID{0xbb, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2}
	machineID := testMachineID

	first, err := NewLog(root, LogConfig{
		Options:      Options{MachineID: machineID, BootID: bootA},
		Source:       "system",
		IdentityMode: LogIdentityStrict,
	})
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	if err := first.Append([]Field{
		StringField("MESSAGE", "cross boot first"),
		StringField("TEST_ID", "cross-boot-monotonic"),
	}, EntryOptions{RealtimeUsec: 1_700_003_100_000_000, MonotonicUsec: 100}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := first.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	second, err := NewLog(root, LogConfig{
		Options:      Options{MachineID: machineID, BootID: bootB},
		Source:       "system",
		IdentityMode: LogIdentityStrict,
	})
	if err != nil {
		t.Fatalf("NewLog(second) error = %v", err)
	}
	if err := second.Append([]Field{
		StringField("MESSAGE", "cross boot second"),
		StringField("TEST_ID", "cross-boot-monotonic"),
	}, EntryOptions{RealtimeUsec: 1_700_003_100_000_001, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := second.Close(); err != nil {
		t.Fatalf("Close(second) error = %v", err)
	}

	dir := filepath.Join(root, machineID.String())
	for _, path := range journalFiles(t, dir) {
		verifyJournalctl(t, path)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=cross-boot-monotonic")
	if len(rows) != 2 {
		t.Fatalf("row count = %d, want 2", len(rows))
	}
	if got := parseU64JSONField(t, rows[0], "__MONOTONIC_TIMESTAMP"); got != 100 {
		t.Fatalf("first monotonic = %d, want 100", got)
	}
	if got := parseU64JSONField(t, rows[1], "__MONOTONIC_TIMESTAMP"); got != 1 {
		t.Fatalf("second monotonic = %d, want unseeded cross-boot value 1", got)
	}
	assertJSONField(t, rows[0], "_BOOT_ID", bootA.String())
	assertJSONField(t, rows[1], "_BOOT_ID", bootB.String())
}

func TestLogAppendMapWithOptionsAddsSourceRealtime(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.AppendMapWithOptions(map[string]string{
		"MESSAGE": "map source realtime",
		"TEST_ID": "map-source-realtime",
	}, EntryOptions{
		RealtimeUsec:       1_700_002_850_000_000,
		MonotonicUsec:      1,
		SourceRealtimeUsec: 1_600_000_100_000_000,
	}); err != nil {
		t.Fatalf("AppendMapWithOptions() error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=map-source-realtime")
	if len(rows) != 1 {
		t.Fatalf("row count = %d, want 1", len(rows))
	}
	assertJSONField(t, rows[0], "_SOURCE_REALTIME_TIMESTAMP", "1600000100000000")
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogDefaultJournaldPolicyPreservesProtectedFields(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "journald policy preserves trusted fields"),
		StringField("TEST_ID", "journald-field-policy"),
		StringField("_HOSTNAME", "synthetic-host"),
		StringField("_TRANSPORT", "snmptrap"),
	}, EntryOptions{RealtimeUsec: 1_700_002_401_100_000, MonotonicUsec: 11}); err != nil {
		t.Fatalf("Append(journald fields) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=journald-field-policy")
	if len(rows) != 1 {
		t.Fatalf("row count = %d, want 1; rows=%v", len(rows), rows)
	}
	assertJSONField(t, rows[0], "_HOSTNAME", "synthetic-host")
	assertJSONField(t, rows[0], "_TRANSPORT", "snmptrap")
	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("journal file count = %d, want 1; files=%v", len(files), files)
	}
	snapshot := readJournalSnapshot(t, files[0])
	if _, ok := snapshot.dataByPayload["_BOOT_ID="+testBootID.String()]; !ok {
		t.Fatalf("missing indexed _BOOT_ID payload")
	}
	for _, path := range files {
		verifyJournalctl(t, path)
	}
}

func TestLogAppendRawJournaldPolicyAddsSourceRealtime(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.AppendRaw([][]byte{
		[]byte("MESSAGE=raw directory payload"),
		[]byte("TEST_ID=raw-journald-field-policy"),
		[]byte("_HOSTNAME=synthetic-host"),
		[]byte("BINARY=a\x00=b=c"),
	}, EntryOptions{
		RealtimeUsec:       1_700_002_401_150_000,
		MonotonicUsec:      11,
		SourceRealtimeUsec: 1_700_002_401_149_999,
	}); err != nil {
		t.Fatalf("AppendRaw(journald payloads) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=raw-journald-field-policy")
	if len(rows) != 1 {
		t.Fatalf("row count = %d, want 1; rows=%v", len(rows), rows)
	}
	assertJSONField(t, rows[0], "_HOSTNAME", "synthetic-host")
	assertJSONField(t, rows[0], "_BOOT_ID", testBootID.String())
	assertJSONField(t, rows[0], "_SOURCE_REALTIME_TIMESTAMP", "1700002401149999")
	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("journal file count = %d, want 1; files=%v", len(files), files)
	}
	snapshot := readJournalSnapshot(t, files[0])
	if _, ok := snapshot.dataByPayload["_BOOT_ID="+testBootID.String()]; !ok {
		t.Fatalf("missing indexed _BOOT_ID payload")
	}
	if got := snapshot.dataByPayload["BINARY=a\x00=b=c"].payload; !bytes.Equal(got, []byte{'B', 'I', 'N', 'A', 'R', 'Y', '=', 'a', 0, '=', 'b', '=', 'c'}) {
		t.Fatalf("raw binary payload = %q", got)
	}
	for _, path := range files {
		verifyJournalctl(t, path)
	}
}

func TestLogJournalAppPolicyDropsProtectedAndInvalidFields(t *testing.T) {
	requireJournalctl(t)

	opts := testOptions()
	opts.FieldNamePolicy = FieldNamePolicyJournalApp
	log, dir := newTestLog(t, LogConfig{
		Options: opts,
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "journal app policy keeps valid fields"),
		StringField("TEST_ID", "journal-app-field-policy"),
		StringField("_HOSTNAME", "dropped-host"),
		StringField("foo.bar", "dropped-dot"),
	}, EntryOptions{RealtimeUsec: 1_700_002_401_200_000, MonotonicUsec: 12}); err != nil {
		t.Fatalf("Append(journal-app fields) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("_HOSTNAME", "drop-only"),
	}, EntryOptions{RealtimeUsec: 1_700_002_401_200_001, MonotonicUsec: 13}); !errors.Is(err, errEntryEmpty) {
		t.Fatalf("Append(drop-only journal-app field) error = %v, want errEntryEmpty", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=journal-app-field-policy")
	if len(rows) != 1 {
		t.Fatalf("row count = %d, want 1; rows=%v", len(rows), rows)
	}
	assertJSONField(t, rows[0], "MESSAGE", "journal app policy keeps valid fields")
	assertJSONField(t, rows[0], "_BOOT_ID", testBootID.String())
	if _, ok := rows[0]["_HOSTNAME"]; ok {
		t.Fatalf("journal-app row kept protected field: %v", rows[0])
	}
	if _, ok := rows[0]["foo.bar"]; ok {
		t.Fatalf("journal-app row kept invalid dotted field: %v", rows[0])
	}
	for _, path := range journalFiles(t, dir) {
		verifyJournalctl(t, path)
	}
}

func TestLogAppendRawJournalAppPolicyDropsProtectedAndInvalidPayloads(t *testing.T) {
	requireJournalctl(t)

	opts := testOptions()
	opts.FieldNamePolicy = FieldNamePolicyJournalApp
	log, dir := newTestLog(t, LogConfig{
		Options: opts,
		Source:  "system",
	})
	if err := log.AppendRaw([][]byte{
		[]byte("MESSAGE=raw journal app policy keeps valid fields"),
		[]byte("TEST_ID=raw-journal-app-field-policy"),
		[]byte("_HOSTNAME=dropped-host"),
		[]byte("foo.bar=dropped-dot"),
	}, EntryOptions{RealtimeUsec: 1_700_002_401_250_000, MonotonicUsec: 13}); err != nil {
		t.Fatalf("AppendRaw(journal-app payloads) error = %v", err)
	}
	if err := log.AppendRaw([][]byte{
		[]byte("_HOSTNAME=drop-only"),
	}, EntryOptions{RealtimeUsec: 1_700_002_401_250_001, MonotonicUsec: 14}); !errors.Is(err, errEntryEmpty) {
		t.Fatalf("AppendRaw(drop-only journal-app payload) error = %v, want errEntryEmpty", err)
	}
	if err := log.AppendRaw([][]byte{[]byte("NO_EQUALS")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("AppendRaw(journal-app malformed payload) error = %v, want errFieldName", err)
	}
	if err := log.AppendRaw([][]byte{[]byte("=bad")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("AppendRaw(journal-app empty-name payload) error = %v, want errFieldName", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=raw-journal-app-field-policy")
	if len(rows) != 1 {
		t.Fatalf("row count = %d, want 1; rows=%v", len(rows), rows)
	}
	assertJSONField(t, rows[0], "MESSAGE", "raw journal app policy keeps valid fields")
	assertJSONField(t, rows[0], "_BOOT_ID", testBootID.String())
	if _, ok := rows[0]["_HOSTNAME"]; ok {
		t.Fatalf("journal-app raw row kept protected field: %v", rows[0])
	}
	if _, ok := rows[0]["foo.bar"]; ok {
		t.Fatalf("journal-app raw row kept invalid dotted field: %v", rows[0])
	}
	for _, path := range journalFiles(t, dir) {
		verifyJournalctl(t, path)
	}
}

func TestLogRawPolicyAllowsStructureOnlyFieldNames(t *testing.T) {
	longName := strings.Repeat("a", 1024)
	opts := testOptions()
	opts.FieldNamePolicy = FieldNamePolicyRaw
	log, dir := newTestLog(t, LogConfig{
		Options: opts,
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("lowercase", "ok"),
		StringField("foo.bar", "dot"),
		StringField("field name", "space"),
		StringField(longName, "long"),
		{Name: "BINARY", Value: []byte{'a', 0, '=', 'b'}},
	}, EntryOptions{RealtimeUsec: 1_700_002_401_300_000, MonotonicUsec: 13}); err != nil {
		t.Fatalf("Append(raw fields) error = %v", err)
	}
	if err := log.Append([]Field{StringField("BAD=NAME", "bad")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("Append(raw name containing '=') error = %v, want errFieldName", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("journal file count = %d, want 1; files=%v", len(files), files)
	}
	snapshot := readJournalSnapshot(t, files[0])
	for _, field := range []string{"lowercase", "foo.bar", "field name", longName, "BINARY"} {
		if _, ok := snapshot.fieldByPayload[field]; !ok {
			t.Fatalf("missing raw FIELD object %q", field)
		}
	}
	if got := snapshot.dataByPayload["BINARY=a\x00=b"].payload; !bytes.Equal(got, []byte{'B', 'I', 'N', 'A', 'R', 'Y', '=', 'a', 0, '=', 'b'}) {
		t.Fatalf("raw binary payload = %q", got)
	}
}

func TestLogAppendRawRawPolicyAllowsStructureOnlyPayloadNames(t *testing.T) {
	longName := strings.Repeat("a", 1024)
	opts := testOptions()
	opts.FieldNamePolicy = FieldNamePolicyRaw
	log, dir := newTestLog(t, LogConfig{
		Options: opts,
		Source:  "system",
	})
	if err := log.AppendRaw([][]byte{
		[]byte("lowercase=ok"),
		[]byte("foo.bar=dot"),
		[]byte("field name=space"),
		[]byte(longName + "=long"),
		[]byte("BINARY=a\x00=b"),
	}, EntryOptions{RealtimeUsec: 1_700_002_401_350_000, MonotonicUsec: 14}); err != nil {
		t.Fatalf("AppendRaw(raw payloads) error = %v", err)
	}
	if err := log.AppendRaw([][]byte{[]byte("NO_EQUALS")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("AppendRaw(no equals) error = %v, want errFieldName", err)
	}
	if err := log.AppendRaw([][]byte{[]byte("=bad")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("AppendRaw(empty name) error = %v, want errFieldName", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("journal file count = %d, want 1; files=%v", len(files), files)
	}
	snapshot := readJournalSnapshot(t, files[0])
	for _, field := range []string{"lowercase", "foo.bar", "field name", longName, "BINARY"} {
		if _, ok := snapshot.fieldByPayload[field]; !ok {
			t.Fatalf("missing raw FIELD object %q", field)
		}
	}
	if got := snapshot.dataByPayload["BINARY=a\x00=b"].payload; !bytes.Equal(got, []byte{'B', 'I', 'N', 'A', 'R', 'Y', '=', 'a', 0, '=', 'b'}) {
		t.Fatalf("raw binary payload = %q", got)
	}
}
