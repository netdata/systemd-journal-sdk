package main

import (
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func main() {
	var path string
	var readyFile string
	var entries int
	var delayText string
	var syncEvery int
	var crashAfter int
	var binaryFixture bool
	var compressionStr string
	var compressThreshold int
	var zstdFixture bool

	flag.StringVar(&path, "path", "", "journal path to create")
	flag.StringVar(&readyFile, "ready-file", "", "path to create after the first entry is committed")
	flag.IntVar(&entries, "entries", 1000, "number of entries to append")
	flag.StringVar(&delayText, "delay", "1ms", "delay between appends")
	flag.IntVar(&syncEvery, "sync-every", 25, "sync every N entries")
	flag.IntVar(&crashAfter, "crash-after", 0, "exit with status 17 after N entries without closing")
	flag.BoolVar(&binaryFixture, "binary-fixture", false, "write binary fields in the first entry")
	flag.StringVar(&compressionStr, "compression", "none", "compression: none, zstd")
	flag.IntVar(&compressThreshold, "compress-threshold", 64, "minimum payload size for compression")
	flag.BoolVar(&zstdFixture, "zstd-fixture", false, "write zstd-compressed fields in the first entry")
	flag.Parse()

	if path == "" || readyFile == "" || entries <= 0 {
		fmt.Fprintln(os.Stderr, "path, ready-file, and positive entries are required")
		os.Exit(2)
	}

	delay, err := time.ParseDuration(delayText)
	if err != nil {
		fmt.Fprintf(os.Stderr, "invalid delay: %v\n", err)
		os.Exit(2)
	}

	compression := journal.CompressionNone
	if compressionStr == "zstd" {
		compression = journal.CompressionZSTD
	}

	opts := journal.Options{Compression: compression, CompressThresholdBytes: compressThreshold}
	w, err := journal.Create(path, opts)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create journal: %v\n", err)
		os.Exit(1)
	}

	const realtimeBase = uint64(1_700_001_000_000_000)
	for i := 0; i < entries; i++ {
		fields := []journal.Field{
			journal.StringField("MESSAGE", fmt.Sprintf("live-%06d", i)),
			journal.StringField("PRIORITY", "6"),
			journal.StringField("SYSLOG_IDENTIFIER", "go-live-writer"),
			journal.StringField("LIVE_SEQ", fmt.Sprintf("%06d", i)),
		}
		if binaryFixture && i == 0 {
			fields = []journal.Field{
				journal.StringField("TEST_ID", "binary-interoperability"),
				journal.StringField("MESSAGE", "binary interoperability"),
				journal.StringField("PRIORITY", "6"),
				journal.StringField("LIVE_SEQ", "000000"),
				{Name: "BINARY_PAYLOAD", Value: []byte{0x00, 0x01, 0x02, 'A', '\n', 0x7f, 0x80, 0xff}},
				{Name: "BINARY_MATCH", Value: []byte{'a', 'b', 'c', 0x07, 'd', 'e', 'f'}},
				{Name: "BINARY_EMPTY", Value: []byte{}},
			}
		} else if zstdFixture && i == 0 {
			largePayload := make([]byte, 256)
			for j := range largePayload {
				largePayload[j] = byte(j%26 + 'A')
			}
			fields = []journal.Field{
				journal.StringField("TEST_ID", "zstd-interoperability"),
				journal.StringField("MESSAGE", "zstd interoperability"),
				journal.StringField("PRIORITY", "6"),
				journal.StringField("LIVE_SEQ", "000000"),
				{Name: "COMPRESSED_PAYLOAD", Value: largePayload},
				{Name: "COMPRESSED_MATCH", Value: largePayload[:32]},
			}
		}
		if err := w.Append(fields, journal.EntryOptions{
			RealtimeUsec:  realtimeBase + uint64(i),
			MonotonicUsec: uint64(i + 1),
		}); err != nil {
			fmt.Fprintf(os.Stderr, "append %d: %v\n", i, err)
			_ = w.Close()
			os.Exit(1)
		}

		if i == 0 {
			if err := w.Sync(); err != nil {
				fmt.Fprintf(os.Stderr, "sync first entry: %v\n", err)
				_ = w.Close()
				os.Exit(1)
			}
			if err := os.WriteFile(readyFile, []byte("ready\n"), 0o600); err != nil {
				fmt.Fprintf(os.Stderr, "write ready file: %v\n", err)
				_ = w.Close()
				os.Exit(1)
			}
		} else if syncEvery > 0 && (i+1)%syncEvery == 0 {
			if err := w.Sync(); err != nil {
				fmt.Fprintf(os.Stderr, "sync %d: %v\n", i, err)
				_ = w.Close()
				os.Exit(1)
			}
		}

		if crashAfter > 0 && i+1 >= crashAfter {
			os.Exit(17)
		}
		if delay > 0 {
			time.Sleep(delay)
		}
	}

	if err := w.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "close journal: %v\n", err)
		os.Exit(1)
	}
}
