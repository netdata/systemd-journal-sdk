// Command entry_output_fixture writes tests/output/entry-output.journal, the
// shared fixture whose export and JSON renderings by systemd's journalctl are
// the golden files next to it. Run from go/:
//
//	go run ./internal/testcmd/entry_output_fixture ../tests/output/entry-output.journal
package main

import (
	"bytes"
	"fmt"
	"os"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: entry_output_fixture OUTPUT")
		os.Exit(2)
	}
	if err := run(os.Args[1]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func id(b byte) journal.UUID {
	var u journal.UUID
	for i := range u {
		u[i] = b
	}
	return u
}

func run(path string) error {
	_ = os.Remove(path)
	w, err := journal.Create(path, journal.Options{
		MachineID:       id(0x11),
		BootID:          id(0x22),
		SeqnumID:        id(0x33),
		FileID:          id(0x44),
		Compression:     journal.CompressionZSTD,
		FieldNamePolicy: journal.FieldNamePolicyRaw,
	})
	if err != nil {
		return err
	}
	binary := []byte{0x00, 0x01, 0xff, '\n'}
	entries := [][]string{
		{
			"MESSAGE=hello \"quoted\" \\ back <tag> & more",
			"PRIORITY=6",
			"MULTI=first",
			"MULTI=second",
			"TEXT_NL=line one\nline two",
			"TAB=a\tb",
			"BIN=" + string(binary),
			"NONCHAR=x﷐y",
			"FFFE=\xef\xbf\xbe",
			"REPLACEMENT=�",
			"C1=\u0085",
			"DEL=a\x7fb",
			"CR=a\rb",
			"UNICODE=héllo ☃",
			"EMPTY=",
			"_BOOT_ID=" + id(0x55).String(),
			"BIG=" + string(bytes.Repeat([]byte("A"), 5000)),
			"BIGBIN=" + string(bytes.Repeat([]byte{0x00, 0xfe}, 2500)),
			"EDGE=" + string(bytes.Repeat([]byte("B"), 4090)),
			"__CURSOR=data cursor",
			"__CUSTOM=protected",
		},
		{
			"MESSAGE=second entry",
			"MULTI=first",
			"lower=invalid name",
			"AFTER=after invalid",
		},
		{
			"MESSAGE=third entry",
			"1DIGIT=invalid name",
			"LAST=after digit",
		},
	}
	for i, fields := range entries {
		payloads := make([][]byte, len(fields))
		for j, field := range fields {
			payloads[j] = []byte(field)
		}
		if err := w.AppendRaw(payloads, journal.EntryOptions{
			RealtimeUsec:  1700000000000000 + uint64(i)*1000,
			MonotonicUsec: 1000 + uint64(i),
		}); err != nil {
			return err
		}
	}
	return w.Close()
}
