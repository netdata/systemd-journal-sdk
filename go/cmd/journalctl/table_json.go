//go:build cgo

package main

import (
	"encoding/json"
	"io"
	"strings"
)

func writeTableJSON(out io.Writer, mode string, rows []map[string]any) error {
	var data []byte
	var err error
	if mode == "json-pretty" {
		data, err = json.MarshalIndent(rows, "", "\t")
	} else {
		data, err = json.Marshal(rows)
	}
	if err != nil {
		return err
	}
	if detectOutputColors().level != 0 {
		data = colorJSON(data)
	}
	if mode == "json-sse" {
		data = append(append([]byte("data: "), data...), '\n', '\n')
	} else if mode == "json-seq" {
		data = append(append([]byte{0x1e}, data...), '\n')
	} else {
		data = append(data, '\n')
	}
	_, err = out.Write(data)
	return err
}

func jsonTableMode(mode string) bool { return strings.HasPrefix(mode, "json") }

func tableTimestamp(value uint64) any {
	if value == ^uint64(0) {
		return nil
	}
	return value
}
