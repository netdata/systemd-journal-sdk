//go:build cgo

package main

import (
	_ "embed"
	"encoding/json"
	"io"
)

//go:embed options.json
var cliOptionsJSON []byte

func printCLIIntrospection(out io.Writer, mode string) error {
	var options any
	if err := json.Unmarshal(cliOptionsJSON, &options); err != nil {
		return err
	}
	value := map[string]any{
		"mediaType": "application/vnd.io.systemd.cli-introspection-0",
		"commands": []any{map[string]any{
			"names": []string{"journalctl"}, "project": "systemd-journal-sdk",
			"version": "Go rewrite", "features": []string{"PCRE2 " + pcreVersion()},
			"abstract": []string{"Query file-backed journals."}, "options": options,
		}},
	}
	var data []byte
	var err error
	if mode == "json-pretty" {
		data, err = json.MarshalIndent(value, "", "\t")
	} else {
		data, err = json.Marshal(value)
	}
	if err != nil {
		return err
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
