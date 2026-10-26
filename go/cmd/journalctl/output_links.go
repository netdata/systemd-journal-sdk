//go:build cgo

package main

import (
	"os"
	"path/filepath"
	"runtime"
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
	if runtime.GOOS == "windows" {
		path, err := filepath.Abs(text)
		if err != nil {
			return []byte(text)
		}
		url := windowsFileURL(path, outputHostname())
		if url == "" {
			return []byte(text)
		}
		return []byte(terminalLink(url, text))
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

// Windows paths need a URI authority and slash-separated, escaped path bytes.
func windowsFileURL(path, hostname string) string {
	path = strings.ReplaceAll(path, `\`, "/")
	if strings.HasPrefix(path, "//?/") || strings.HasPrefix(path, "//./") {
		return ""
	}
	authority := hostname
	if strings.HasPrefix(path, "//") {
		rest := path[2:]
		slash := strings.IndexByte(rest, '/')
		if slash <= 0 || slash == len(rest)-1 {
			return ""
		}
		authority, path = rest[:slash], rest[slash:]
	} else {
		if len(path) < 3 || path[1] != ':' || path[2] != '/' || !((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z')) {
			return ""
		}
		path = "/" + path
	}
	for i := range len(authority) {
		c := authority[i]
		if !((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '.' || c == '-') {
			return ""
		}
	}
	const hex = "0123456789ABCDEF"
	var url strings.Builder
	url.WriteString("file://")
	url.WriteString(authority)
	for i := range len(path) {
		c := path[i]
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || strings.ContainsRune("/-._~:", rune(c)) {
			url.WriteByte(c)
		} else {
			url.WriteByte('%')
			url.WriteByte(hex[c>>4])
			url.WriteByte(hex[c&15])
		}
	}
	return url.String()
}
