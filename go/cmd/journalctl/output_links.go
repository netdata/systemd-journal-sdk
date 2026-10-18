//go:build cgo

package main

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
)

func urlifyEnabled(colors outputColors) bool {
	if value, ok := os.LookupEnv("SYSTEMD_URLIFY"); ok {
		if enabled, err := parseBoolOption("SYSTEMD_URLIFY", value); err == nil {
			return enabled
		}
	}
	return colors.level != 0
}

func terminalLink(url, text string) string {
	if len(url) > 2000 {
		return text
	}
	for i := range len(url) {
		if url[i] < 32 || url[i] >= 127 {
			return text
		}
	}
	return "\x1b]8;;" + url + "\x1b\\" + text + "\x1b]8;;\x1b\\"
}

var outputHostname = sync.OnceValue(func() string {
	hostname, _ := os.Hostname()
	return hostname
})

func fileLink(value []byte, enabled bool) []byte {
	text := string(cStringBytes(value))
	if text == "" {
		return nil
	}
	if !enabled {
		return []byte(text)
	}
	path := text
	if !filepath.IsAbs(path) {
		cwd, err := os.Getwd()
		if err != nil {
			return nil
		}
		if !os.IsPathSeparator(cwd[len(cwd)-1]) {
			cwd += string(filepath.Separator)
		}
		path = cwd + path
	}
	hostname := outputHostname()
	if hostname == "" {
		return nil
	}
	return []byte(terminalLink("file://"+hostname+path, text))
}

func documentationLink(value []byte) string {
	url := string(cStringBytes(value))
	valid := false
	for _, prefix := range []string{"http://", "https://", "file:/", "info:", "man:"} {
		if strings.HasPrefix(url, prefix) && len(url) > len(prefix) {
			valid = true
			break
		}
	}
	if !valid || !isASCII([]byte(url)) {
		return ""
	}
	glyph := "[LNK]"
	unicode := ellipsisMarker == "…"
	if value, ok := os.LookupEnv("SYSTEMD_UTF8"); ok {
		if flag, err := parseBoolOption("SYSTEMD_UTF8", value); err == nil {
			unicode = flag
		}
	}
	if unicode {
		glyph = "[🡕]"
	}
	return terminalLink(url, glyph) + " "
}
