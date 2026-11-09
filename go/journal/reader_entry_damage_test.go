package journal

import (
	"bytes"
	"encoding/binary"
	"os"
	"path/filepath"
	"testing"
)

// Paired with Rust tests/entry_damage.rs.
const objectHeaderToPayload = 64

func TestDamagedDataObjectCostsTheFieldNotTheEntry(t *testing.T) {
	cases := []struct {
		name        string
		patch       func([]byte, int)
		payloadKept bool
	}{
		{"no-separator", func(b []byte, at int) { b[at+6] = '_' }, true},
		{"wrong-type", func(b []byte, at int) { b[at-objectHeaderToPayload] = 3 }, false},
		{"oversized", func(b []byte, at int) {
			binary.LittleEndian.PutUint64(b[at-objectHeaderToPayload+8:], 1<<40)
		}, false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "damage.journal")
			w, err := Create(path, testOptions())
			if err != nil {
				t.Fatal(err)
			}
			if err := w.AppendMapWithOptions(map[string]string{"MESSAGE": "kept", "BROKEN": "1"}, testEntryOptions(1)); err != nil {
				t.Fatal(err)
			}
			if err := w.Close(); err != nil {
				t.Fatal(err)
			}
			data := readFileForTest(t, path)
			at := bytes.Index(data, []byte("BROKEN=1"))
			if at < 0 {
				t.Fatal("payload not in file")
			}
			c.patch(data, at)
			if err := os.WriteFile(path, data, 0o644); err != nil {
				t.Fatal(err)
			}

			j, err := SdJournalOpenFiles([]string{path}, 0)
			if err != nil {
				t.Fatal(err)
			}
			defer j.Close()
			if n, err := SdJournalNext(j); err != nil || n != 1 {
				t.Fatalf("next = %d, %v", n, err)
			}
			entry, err := SdJournalGetEntry(j)
			if err != nil {
				t.Fatal(err)
			}
			if got := string(entry.Fields["MESSAGE"]); got != "kept" {
				t.Fatalf("MESSAGE = %q", got)
			}
			if _, ok := entry.Fields["BROKEN"]; ok {
				t.Fatal("damaged field was returned")
			}
			want := 1
			if c.payloadKept {
				want = 2
			}
			if len(entry.Payloads) != want {
				t.Fatalf("payloads = %d, want %d", len(entry.Payloads), want)
			}
			assertDamagedPayloads := func(api string, got [][]byte) {
				t.Helper()
				kept := false
				for _, payload := range got {
					kept = kept || bytes.Equal(payload, []byte("MESSAGE=kept"))
					if bytes.HasPrefix(payload, []byte("BROKEN=")) {
						t.Fatalf("%s returned the damaged field: %q", api, got)
					}
				}
				if len(got) != want || !kept {
					t.Fatalf("%s payloads = %q", api, got)
				}
			}
			assertDamagedPayloads("GetEntry", entry.Payloads)

			r, err := OpenFile(path)
			if err != nil {
				t.Fatal(err)
			}
			defer r.Close()
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("step = %v, %v", ok, err)
			}
			collected, err := r.CollectEntryPayloads()
			if err != nil {
				t.Fatal(err)
			}
			assertDamagedPayloads("CollectEntryPayloads", collected)
			var enumerated [][]byte
			for {
				payload, ok, err := r.EnumerateEntryPayload()
				if err != nil {
					t.Fatal(err)
				}
				if !ok {
					break
				}
				enumerated = append(enumerated, bytes.Clone(payload))
			}
			assertDamagedPayloads("EnumerateEntryPayload", enumerated)
		})
	}
}

// Paired with Rust missing_main_entry_array_hides_every_entry.
func TestMissingMainEntryArrayHidesEveryEntry(t *testing.T) {
	for _, filtered := range []bool{false, true} {
		path := headerGateJournal(t, false)
		patchHeader(t, path, 176, u64Bytes(0))
		j, err := SdJournalOpenFiles([]string{path}, 0)
		if err != nil {
			t.Fatal(err)
		}
		defer j.Close()
		if filtered {
			if err := SdJournalAddMatch(j, []byte("MESSAGE=gate")); err != nil {
				t.Fatal(err)
			}
		}
		if n, err := SdJournalNext(j); err != nil || n != 0 {
			t.Fatalf("filtered=%v next = %d, %v", filtered, n, err)
		}
	}
}
