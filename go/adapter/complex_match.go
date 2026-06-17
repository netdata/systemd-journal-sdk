package main

import (
	"fmt"
	"github.com/netdata/systemd-journal-sdk/go/journal"
	"os"
)

func runComplexMatchTest(tc *TestCase) Result {
	path, cleanup, err := createComplexMatchFixture()
	if err != nil {
		return failResult(tc, tc.Expected.ResultFormat, err)
	}
	defer cleanup()

	r, err := journal.OpenFile(path)
	if err != nil {
		return failResult(tc, tc.Expected.ResultFormat, err)
	}
	defer r.Close()
	addSystemdComplexMatchExpression(r)

	matched, err := collectMatchedEntries(r)
	if err != nil {
		return failResult(tc, tc.Expected.ResultFormat, err)
	}
	if len(matched) != 2 {
		return Result{
			TestName:     tc.TestName,
			ResultFormat: tc.Expected.ResultFormat,
			Status:       "FAIL",
			Actual:       matched,
			Error:        fmt.Sprintf("matched %d entries, want 2", len(matched)),
		}
	}
	return Result{
		TestName:     tc.TestName,
		ResultFormat: tc.Expected.ResultFormat,
		Status:       "PASS",
		Actual:       matched,
	}
}

func createComplexMatchFixture() (string, func(), error) {
	tmp, err := os.CreateTemp("", "go-journal-match-*.journal")
	if err != nil {
		return "", nil, err
	}
	path := tmp.Name()
	_ = tmp.Close()
	cleanup := func() { _ = os.Remove(path) }
	if err := writeComplexMatchFixture(path); err != nil {
		cleanup()
		return "", nil, err
	}
	return path, cleanup, nil
}

func writeComplexMatchFixture(path string) error {
	machineID, err := journal.ParseUUID("00112233445566778899aabbccddeeff")
	if err != nil {
		return err
	}
	bootID, err := journal.ParseUUID("ffeeddccbbaa99887766554433221100")
	if err != nil {
		return err
	}
	w, err := journal.Create(path, journal.Options{
		MachineID: machineID,
		BootID:    bootID,
	})
	if err != nil {
		return err
	}
	for i, fields := range complexMatchEntries() {
		if err := w.Append(fields, journal.EntryOptions{
			RealtimeUsec:     1_700_000_000_000_000 + uint64(i),
			RealtimeUsecSet:  true,
			MonotonicUsec:    uint64(i + 1),
			MonotonicUsecSet: true,
		}); err != nil {
			_ = w.Close()
			return err
		}
	}
	return w.Close()
}

func complexMatchEntries() [][]journal.Field {
	return [][]journal.Field{
		{
			journal.StringField("L3", "ok"),
			journal.StringField("TWO", "two"),
			journal.StringField("ONE", "one"),
		},
		{
			journal.StringField("L4_1", "yes"),
			journal.StringField("L4_2", "ok"),
			journal.StringField("PIFF", "paff"),
			journal.StringField("QUUX", "xxxxx"),
			journal.StringField("HALLO", "WALDO"),
			{Name: "B", Value: []byte{'C', 0, 'D'}},
			{Name: "A", Value: []byte{1, 2}},
		},
		{journal.StringField("L3", "ok")},
		{
			journal.StringField("TWO", "two"),
			journal.StringField("ONE", "one"),
		},
	}
}

func collectMatchedEntries(r *journal.Reader) ([]map[string]string, error) {
	var matched []map[string]string
	for {
		ok, err := r.Step()
		if err != nil || !ok {
			return matched, err
		}
		entry, err := r.GetEntry()
		if err != nil {
			return nil, err
		}
		matched = append(matched, stringEntryFields(entry))
	}
}

func stringEntryFields(entry *journal.Entry) map[string]string {
	fields := make(map[string]string, len(entry.Fields))
	for k, v := range entry.Fields {
		fields[k] = string(v)
	}
	return fields
}

func addSystemdComplexMatchExpression(r interface {
	AddMatch([]byte)
	AddDisjunction()
	AddConjunction()
}) {
	r.AddMatch([]byte{'A', '=', 1, 2})
	r.AddMatch([]byte{'B', '=', 'C', 0, 'D'})
	r.AddMatch([]byte("HALLO=WALDO"))
	r.AddMatch([]byte("QUUX=mmmm"))
	r.AddMatch([]byte("QUUX=xxxxx"))
	r.AddMatch([]byte("HALLO="))
	r.AddMatch([]byte("QUUX=xxxxx"))
	r.AddMatch([]byte("QUUX=yyyyy"))
	r.AddMatch([]byte("PIFF=paff"))
	r.AddDisjunction()
	r.AddMatch([]byte("ONE=one"))
	r.AddMatch([]byte("ONE=two"))
	r.AddMatch([]byte("TWO=two"))
	r.AddConjunction()
	r.AddMatch([]byte("L4_1=yes"))
	r.AddMatch([]byte("L4_1=ok"))
	r.AddMatch([]byte("L4_2=yes"))
	r.AddMatch([]byte("L4_2=ok"))
	r.AddDisjunction()
	r.AddMatch([]byte("L3=yes"))
	r.AddMatch([]byte("L3=ok"))
}
