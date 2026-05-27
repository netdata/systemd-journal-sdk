package main

import (
	"bufio"
	"bytes"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const oversizedLimit = 4 * 1024 * 1024
const seqnumIDHex = "22222222222222222222222222222222"
const defaultArchiveRealtime = 1_700_000_000_000_000

var (
	bootID    = mustUUID("0123456789abcdef0123456789abcdef")
	machineID = mustUUID("fedcba9876543210fedcba9876543210")
	seqnumID  = mustUUID(seqnumIDHex)
	fileID    = mustUUID("33333333333333333333333333333333")
)

type valueDescriptor struct {
	Kind   string `json:"kind"`
	Text   string `json:"text"`
	Base64 string `json:"base64"`
	Size   int    `json:"size"`
	Byte   byte   `json:"byte"`
}

type fieldRecord struct {
	Name  string          `json:"name"`
	Value valueDescriptor `json:"value"`
}

type acceptedRecord struct {
	RecordType    string        `json:"record_type"`
	EntryID       string        `json:"entry_id"`
	RealtimeUsec  uint64        `json:"realtime_usec"`
	MonotonicUsec uint64        `json:"monotonic_usec"`
	BootID        string        `json:"boot_id"`
	Fields        []fieldRecord `json:"fields"`
}

type rejectedRecord struct {
	RecordType    string                 `json:"record_type"`
	CaseID        string                 `json:"case_id"`
	ExpectedError string                 `json:"expected_error"`
	Input         map[string]interface{} `json:"input"`
}

type result struct {
	Records int      `json:"records"`
	Errors  []string `json:"errors"`
}

func mustUUID(s string) journal.UUID {
	b, err := hex.DecodeString(s)
	if err != nil || len(b) != 16 {
		panic("invalid test UUID")
	}
	var id journal.UUID
	copy(id[:], b)
	return id
}

func materializeValue(v valueDescriptor) ([]byte, error) {
	switch v.Kind {
	case "utf8":
		return []byte(v.Text), nil
	case "bytes":
		data, err := base64.StdEncoding.DecodeString(v.Base64)
		if err != nil {
			return nil, err
		}
		if v.Size != 0 && len(data) != v.Size {
			return nil, fmt.Errorf("bytes size mismatch: expected %d, got %d", v.Size, len(data))
		}
		return data, nil
	case "repeat":
		return bytes.Repeat([]byte{v.Byte}, v.Size), nil
	default:
		return nil, fmt.Errorf("unknown value kind: %q", v.Kind)
	}
}

func validFieldName(name string) bool {
	if name == "" || len([]byte(name)) > 64 {
		return false
	}
	if name[0] >= '0' && name[0] <= '9' {
		return false
	}
	for i := 0; i < len(name); i++ {
		c := name[i]
		if c != '_' && (c < 'A' || c > 'Z') && (c < '0' || c > '9') {
			return false
		}
	}
	return true
}

func expectedRejection(input map[string]interface{}) string {
	if raw, ok := input["raw_payload"].(string); ok {
		idx := strings.IndexByte(raw, '=')
		if idx < 0 {
			return "EINVAL"
		}
		if !validFieldName(raw[:idx]) {
			return "EINVAL"
		}
		return ""
	}
	name, _ := input["field_name"].(string)
	if !validFieldName(name) {
		return "EINVAL"
	}
	rawValue, ok := input["value"]
	if !ok || rawValue == nil {
		return "EINVAL"
	}
	valueBytes, err := json.Marshal(rawValue)
	if err != nil {
		return "EINVAL"
	}
	var value valueDescriptor
	if err := json.Unmarshal(valueBytes, &value); err != nil {
		return "EINVAL"
	}
	if value.Kind == "repeat" && value.Size > oversizedLimit {
		return "E2BIG"
	}
	return ""
}

func makeWriter(path string, compact bool, maxSizeBytes uint64) (*journal.Writer, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	return journal.Create(path, journal.Options{
		MachineID:              machineID,
		BootID:                 bootID,
		SeqnumID:               seqnumID,
		FileID:                 fileID,
		HeadSeqnum:             1,
		MaxFileSize:            maxSizeBytes,
		Compression:            journal.CompressionNone,
		CompressThresholdBytes: 512,
		Compact:                compact,
	})
}

func archivePathFor(output string, headRealtime uint64) string {
	prefix := strings.TrimSuffix(output, ".journal")
	return fmt.Sprintf("%s@%s-%016x-%016x.journal", prefix, seqnumIDHex, uint64(1), headRealtime)
}

func finalizeWriter(w *journal.Writer, output string, finalState string, headRealtime uint64) error {
	switch finalState {
	case "online":
		return w.Close()
	case "offline":
		return w.CloseOffline()
	case "archived":
		archivePath := archivePathFor(output, headRealtime)
		_ = os.Remove(archivePath)
		return w.ArchiveTo(archivePath)
	default:
		return fmt.Errorf("invalid final state %q", finalState)
	}
}

func ingestAccepted(dataset, output string, finalState string, compact bool, maxSizeBytes uint64) result {
	w, err := makeWriter(output, compact, maxSizeBytes)
	if err != nil {
		return result{Errors: []string{err.Error()}}
	}
	defer w.Close()

	file, err := os.Open(dataset)
	if err != nil {
		return result{Errors: []string{err.Error()}}
	}
	defer file.Close()

	res := result{Errors: []string{}}
	headRealtime := uint64(0)
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 64*1024), 8*1024*1024)
	lineNo := 0
	for scanner.Scan() {
		lineNo++
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var rec acceptedRecord
		if err := json.Unmarshal([]byte(line), &rec); err != nil {
			res.Errors = append(res.Errors, fmt.Sprintf("line %d: decode failed: %v", lineNo, err))
			continue
		}
		if rec.RecordType != "accepted" {
			continue
		}
		fields := make([]journal.Field, 0, len(rec.Fields))
		bad := false
		for _, f := range rec.Fields {
			value, err := materializeValue(f.Value)
			if err != nil {
				res.Errors = append(res.Errors, fmt.Sprintf("line %d %s: %v", lineNo, rec.EntryID, err))
				bad = true
				break
			}
			fields = append(fields, journal.Field{Name: f.Name, Value: value})
		}
		if bad {
			continue
		}
		entryBootID := bootID
		if rec.BootID != "" {
			entryBootID = mustUUID(rec.BootID)
		}
		if err := w.Append(fields, journal.EntryOptions{
			RealtimeUsec:  rec.RealtimeUsec,
			MonotonicUsec: rec.MonotonicUsec,
			BootID:        entryBootID,
		}); err != nil {
			res.Errors = append(res.Errors, fmt.Sprintf("line %d %s: append failed: %v", lineNo, rec.EntryID, err))
			continue
		}
		if headRealtime == 0 {
			headRealtime = rec.RealtimeUsec
		}
		res.Records++
	}
	if err := scanner.Err(); err != nil {
		res.Errors = append(res.Errors, err.Error())
	}
	if err := w.Sync(); err != nil {
		res.Errors = append(res.Errors, err.Error())
	}
	if headRealtime == 0 {
		headRealtime = defaultArchiveRealtime
	}
	if err := finalizeWriter(w, output, finalState, headRealtime); err != nil {
		res.Errors = append(res.Errors, err.Error())
	}
	return res
}

func ingestRejections(dataset, output string, finalState string, compact bool, maxSizeBytes uint64) result {
	file, err := os.Open(dataset)
	if err != nil {
		return result{Errors: []string{err.Error()}}
	}
	defer file.Close()

	var w *journal.Writer
	headRealtime := uint64(defaultArchiveRealtime)
	res := result{Errors: []string{}}
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 64*1024), 8*1024*1024)
	lineNo := 0
	for scanner.Scan() {
		lineNo++
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var rec rejectedRecord
		if err := json.Unmarshal([]byte(line), &rec); err != nil {
			res.Errors = append(res.Errors, fmt.Sprintf("line %d: decode failed: %v", lineNo, err))
			continue
		}
		if rec.RecordType != "rejected" {
			continue
		}
		if got := expectedRejection(rec.Input); got != "" {
			if got == rec.ExpectedError {
				res.Records++
			} else {
				res.Errors = append(res.Errors, fmt.Sprintf("line %d %s: got %s, expected %s", lineNo, rec.CaseID, got, rec.ExpectedError))
			}
			continue
		}

		if w == nil {
			w, err = makeWriter(output, compact, maxSizeBytes)
			if err != nil {
				res.Errors = append(res.Errors, err.Error())
				break
			}
			defer w.Close()
		}
		valueBytes, _ := json.Marshal(rec.Input["value"])
		var value valueDescriptor
		if err := json.Unmarshal(valueBytes, &value); err != nil {
			res.Errors = append(res.Errors, fmt.Sprintf("line %d %s: %v", lineNo, rec.CaseID, err))
			continue
		}
		fieldValue, err := materializeValue(value)
		if err != nil {
			res.Errors = append(res.Errors, fmt.Sprintf("line %d %s: %v", lineNo, rec.CaseID, err))
			continue
		}
		err = w.Append([]journal.Field{{Name: rec.Input["field_name"].(string), Value: fieldValue}}, journal.EntryOptions{BootID: bootID})
		if err == nil {
			headRealtime = defaultArchiveRealtime
			res.Errors = append(res.Errors, fmt.Sprintf("line %d %s: unexpectedly accepted", lineNo, rec.CaseID))
		} else if rec.ExpectedError == "EINVAL" {
			res.Records++
		} else {
			res.Errors = append(res.Errors, fmt.Sprintf("line %d %s: rejected as EINVAL, expected %s", lineNo, rec.CaseID, rec.ExpectedError))
		}
	}
	if err := scanner.Err(); err != nil {
		res.Errors = append(res.Errors, err.Error())
	}
	if w != nil {
		if err := finalizeWriter(w, output, finalState, headRealtime); err != nil {
			res.Errors = append(res.Errors, err.Error())
		}
	}
	return res
}

func main() {
	dataset := flag.String("dataset", "", "dataset JSONL path")
	output := flag.String("output", "", "output journal path")
	rejectionMode := flag.Bool("rejection-mode", false, "process rejection corpus")
	finalState := flag.String("final-state", "online", "final journal state: online, offline, archived")
	compact := flag.Bool("compact", false, "write the systemd compact journal format")
	maxSizeBytes := flag.Uint64("max-size-bytes", 0, "systemd max-size value used for hash table sizing; zero uses the SDK default")
	flag.Parse()
	if *dataset == "" || *output == "" {
		fmt.Fprintln(os.Stderr, "usage: dataset_ingester --dataset PATH --output PATH [--rejection-mode] [--final-state online|offline|archived] [--compact] [--max-size-bytes BYTES]")
		os.Exit(2)
	}

	var res result
	if *rejectionMode {
		res = ingestRejections(*dataset, *output, *finalState, *compact, *maxSizeBytes)
	} else {
		res = ingestAccepted(*dataset, *output, *finalState, *compact, *maxSizeBytes)
	}
	encoded, _ := json.Marshal(res)
	fmt.Println(string(encoded))
	if len(res.Errors) != 0 {
		os.Exit(1)
	}
}
