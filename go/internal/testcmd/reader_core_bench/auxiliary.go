package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func readAuxiliary(cfg benchConfig) (counts, error) {
	if cfg.surface != "file" || len(cfg.inputs) != 1 || cfg.direction != "forward" || cfg.bounds != "snapshot" {
		return counts{}, errors.New("indexed/verification modes require one file, snapshot bounds and forward direction")
	}
	ctx := context.Background()
	var out counts
	if strings.HasPrefix(cfg.mode, "verify-") {
		if cfg.mmapStrategy != "auto" || cfg.windowSize != defaultWindowSize {
			return counts{}, errors.New("verification modes use public SDK defaults; select auto access and the default window size")
		}
		for range cfg.catalogRepetitions {
			var err error
			switch cfg.mode {
			case "verify-index":
				err = journal.VerifyIndex(ctx, cfg.inputs[0])
			case "verify-file":
				err = journal.VerifyFile(cfg.inputs[0])
			default:
				return counts{}, fmt.Errorf("invalid verification mode: %s", cfg.mode)
			}
			if err != nil {
				return counts{}, err
			}
			out.addRecordMarker(1)
		}
		out.extra = map[string]interface{}{"completed_verifications": out.records, "reader_options": "public SDK defaults"}
		return out, nil
	}
	if len(cfg.filterMatches) != 1 {
		return counts{}, errors.New("indexed modes require one --match FIELD=value declaration")
	}
	name, value, found := bytes.Cut(cfg.filterMatches[0], []byte{'='})
	if !found || len(name) == 0 {
		return counts{}, errors.New("indexed declaration must be FIELD=value")
	}
	opts := journal.IndexedSnapshotOptions{
		Reader:        cfg.options,
		CaptureFields: [][]byte{name},
		CaptureValues: []journal.Field{{Name: string(name), Value: value}},
	}
	var capturedEntries, capturedValueEntries uint64
	var capturedPresent bool
	digest := strings.HasSuffix(cfg.mode, "-digest")
	for range cfg.catalogRepetitions {
		snapshot, err := journal.OpenIndexedSnapshot(ctx, cfg.inputs[0], opts)
		if err != nil {
			return counts{}, err
		}
		capturedEntries = snapshot.EntryCount()
		captured, err := snapshot.CapturedValue(name, value)
		if err == nil {
			capturedValueEntries, capturedPresent = captured.EntryCount, captured.Present
			visit := func(entry *journal.SnapshotEntry) error {
				out.addRecordMarker(entry.Realtime)
				return entry.VisitPayloads(func(payload []byte) error {
					if digest {
						out.addFullPayload(payload)
					} else {
						out.addPayload(payload)
					}
					return nil
				})
			}
			switch strings.TrimSuffix(cfg.mode, "-digest") {
			case "indexed-open":
				out.addRecordMarker(capturedEntries)
			case "indexed-exact":
				err = snapshot.VisitMatch(ctx, name, value, visit)
			case "indexed-field":
				err = snapshot.VisitField(ctx, name, func(candidate []byte) (bool, error) { return bytes.Equal(candidate, value), nil }, visit)
			case "indexed-all":
				err = snapshot.VisitEntries(ctx, visit)
			default:
				err = fmt.Errorf("invalid indexed mode: %s", cfg.mode)
			}
		}
		if err = errors.Join(err, snapshot.Close()); err != nil {
			return counts{}, err
		}
	}
	out.extra = map[string]interface{}{
		"completed_opens":        cfg.catalogRepetitions,
		"captured_entries":       capturedEntries,
		"captured_value_entries": capturedValueEntries,
		"captured_value_present": capturedPresent,
	}
	return out, nil
}
