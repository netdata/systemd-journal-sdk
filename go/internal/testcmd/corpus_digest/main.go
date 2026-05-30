package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"sort"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const (
	schemaVersion    = "systemd-journal-sdk-corpus-logical-v1"
	schemaMagic      = "systemd-journal-sdk-corpus-logical-v1\x00"
	defaultWindowLen = uint64(32 * 1024 * 1024)
)

var metadataPayloadNames = map[string]struct{}{
	"_BOOT_ID": {},
}

type counts struct {
	Entries                       uint64 `json:"entries"`
	Payloads                      uint64 `json:"payloads"`
	PayloadBytes                  uint64 `json:"payload_bytes"`
	BinaryPayloads                uint64 `json:"binary_payloads"`
	PayloadsWithoutSeparator      uint64 `json:"payloads_without_separator"`
	EntriesWithRepeatedFieldNames uint64 `json:"entries_with_repeated_field_names"`
	RepeatedFieldNameOccurrences  uint64 `json:"repeated_field_name_occurrences"`
	LargestPayloadBytes           uint64 `json:"largest_payload_bytes"`
}

type canonicalDigest struct {
	hash   hashWriter
	counts counts
}

type hashWriter interface {
	Write([]byte) (int, error)
	Sum([]byte) []byte
}

func newCanonicalDigest() *canonicalDigest {
	h := sha256.New()
	_, _ = h.Write([]byte(schemaMagic))
	return &canonicalDigest{hash: h}
}

func (d *canonicalDigest) writeU64(value uint64) {
	var buf [8]byte
	binary.BigEndian.PutUint64(buf[:], value)
	_, _ = d.hash.Write(buf[:])
}

func (d *canonicalDigest) writeBytes(tag byte, value []byte) {
	_, _ = d.hash.Write([]byte{tag})
	d.writeU64(uint64(len(value)))
	_, _ = d.hash.Write(value)
}

func (d *canonicalDigest) writeNamedBytes(tag byte, name []byte, value []byte) {
	_, _ = d.hash.Write([]byte{tag})
	d.writeU64(uint64(len(name)))
	_, _ = d.hash.Write(name)
	d.writeU64(uint64(len(value)))
	_, _ = d.hash.Write(value)
}

func binaryPayload(payload []byte) bool {
	for _, b := range payload {
		if b < 32 && b != '\t' {
			return true
		}
	}
	return false
}

func payloadName(payload []byte) ([]byte, bool) {
	for i, b := range payload {
		if b == '=' {
			if i == 0 {
				return nil, false
			}
			return payload[:i], true
		}
	}
	return nil, false
}

func sortPayloads(payloads [][]byte) {
	sort.Slice(payloads, func(i, j int) bool {
		return bytes.Compare(payloads[i], payloads[j]) < 0
	})
}

func (d *canonicalDigest) addEntry(entry *journal.Entry) {
	_, _ = d.hash.Write([]byte("E"))
	d.writeU64(d.counts.Entries)
	d.writeNamedBytes('M', []byte("__REALTIME_TIMESTAMP"), []byte(fmt.Sprintf("%d", entry.Realtime)))
	d.writeNamedBytes('M', []byte("__MONOTONIC_TIMESTAMP"), []byte(fmt.Sprintf("%d", entry.Monotonic)))
	d.writeNamedBytes('M', []byte("__SEQNUM"), []byte(fmt.Sprintf("%d", entry.Seqnum)))
	d.writeNamedBytes('M', []byte("__BOOT_ID"), []byte(hex.EncodeToString(entry.BootID[:])))

	payloads := make([][]byte, 0, len(entry.Payloads))
	seen := map[string]struct{}{}
	repeated := false
	var repeatedOccurrences uint64
	for _, payload := range entry.Payloads {
		name, ok := payloadName(payload)
		if ok {
			if _, metadata := metadataPayloadNames[string(name)]; metadata {
				continue
			}
		}
		payloads = append(payloads, payload)
		d.counts.Payloads++
		d.counts.PayloadBytes += uint64(len(payload))
		if uint64(len(payload)) > d.counts.LargestPayloadBytes {
			d.counts.LargestPayloadBytes = uint64(len(payload))
		}
		if binaryPayload(payload) {
			d.counts.BinaryPayloads++
		}
		if !ok {
			d.counts.PayloadsWithoutSeparator++
			continue
		}
		key := string(name)
		if _, ok := seen[key]; ok {
			repeated = true
			repeatedOccurrences++
		} else {
			seen[key] = struct{}{}
		}
	}
	if repeated {
		d.counts.EntriesWithRepeatedFieldNames++
		d.counts.RepeatedFieldNameOccurrences += repeatedOccurrences
	}

	sortPayloads(payloads)
	for _, payload := range payloads {
		d.writeBytes('P', payload)
	}
	_, _ = d.hash.Write([]byte("e"))
	d.counts.Entries++
}

func (d *canonicalDigest) result() map[string]interface{} {
	return map[string]interface{}{
		"schema":         schemaVersion,
		"logical_digest": hex.EncodeToString(d.hash.Sum(nil)),
		"counts":         d.counts,
	}
}

func parseOptions(bounds, access string) (journal.ReaderOptions, error) {
	opts := journal.DefaultReaderOptions()
	switch bounds {
	case "live":
		opts = opts.WithBounds(journal.ReaderBoundsLive)
	case "snapshot":
		opts = opts.WithBounds(journal.ReaderBoundsSnapshot)
	default:
		return opts, fmt.Errorf("invalid --bounds: %s", bounds)
	}
	switch access {
	case "read-at":
		opts = opts.WithAccessMode(journal.ReaderAccessReadAt)
	case "mmap", "windowed", "whole-file":
		opts = opts.WithAccessMode(journal.ReaderAccessMmap)
	default:
		return opts, fmt.Errorf("invalid --access: %s", access)
	}
	return opts, nil
}

func main() {
	input := flag.String("input", "", "journal input file")
	bounds := flag.String("bounds", "snapshot", "reader bounds: snapshot or live")
	access := flag.String("access", "mmap", "reader access: mmap or read-at")
	windowSize := flag.Uint64("window-size", defaultWindowLen, "reserved for parity with Rust")
	flag.Parse()
	_ = windowSize
	if *input == "" {
		fmt.Fprintln(os.Stderr, "--input is required")
		os.Exit(2)
	}
	opts, err := parseOptions(*bounds, *access)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	started := time.Now()
	reader, err := journal.OpenFileWithOptions(*input, opts)
	if err != nil {
		fmt.Fprintf(os.Stderr, "open input: %v\n", err)
		os.Exit(1)
	}
	defer reader.Close()
	if err := reader.SeekHead(); err != nil {
		fmt.Fprintf(os.Stderr, "seek head: %v\n", err)
		os.Exit(1)
	}
	digest := newCanonicalDigest()
	for {
		ok, err := reader.Step()
		if err != nil {
			fmt.Fprintf(os.Stderr, "read entry: %v\n", err)
			os.Exit(1)
		}
		if !ok {
			break
		}
		entry, err := reader.GetEntry()
		if err != nil {
			fmt.Fprintf(os.Stderr, "get entry: %v\n", err)
			os.Exit(1)
		}
		digest.addEntry(entry)
	}
	result := digest.result()
	result["driver"] = "go"
	result["elapsed_seconds"] = time.Since(started).Seconds()
	if stat, err := os.Stat(*input); err == nil {
		result["input_bytes"] = stat.Size()
	}
	_ = json.NewEncoder(os.Stdout).Encode(result)
}
