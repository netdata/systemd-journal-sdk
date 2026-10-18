//go:build cgo

package main

import (
	"os"
	"strings"
)

type matchRange struct{ start, end int }

type outputColors struct {
	level     int
	underline bool
}

const (
	ansiNormal    = "\x1b[0m"
	ansiHighlight = "\x1b[0;1;39m"
	ansiRed       = "\x1b[0;1;31m"
	ansiBlue      = "\x1b[0;34m"
	ansiGreen     = "\x1b[0;32m"
)

func detectOutputColors() outputColors {
	mode := os.Getenv("SYSTEMD_COLORS")
	level := 0
	switch mode {
	case "off":
		return outputColors{}
	case "16":
		level = 16
	case "256":
		level = 256
	case "24bit":
		level = 24
	default:
		force, err := parseBoolOption("SYSTEMD_COLORS", mode)
		if err == nil && !force {
			return outputColors{}
		}
		if err != nil || !force {
			_, noColor := os.LookupEnv("NO_COLOR")
			term, termSet := os.LookupEnv("TERM")
			if noColor || !termSet || term == "dumb" || os.Getpid() != 1 && !outputIsTerminal() {
				return outputColors{}
			}
		}
		switch mode {
		case "auto-16":
			level = 16
		case "auto-24bit":
			level = 24
		case "auto-256":
			level = 256
		default:
			level = 256
			if value := os.Getenv("COLORTERM"); value == "truecolor" || value == "24bit" {
				level = 24
			}
		}
	}
	return outputColors{level: level, underline: level != 0 && os.Getenv("TERM") != "linux"}
}

func (c outputColors) grey() string {
	if c.level == 16 {
		return "\x1b[0;90m"
	}
	if c.level == 24 {
		return "\x1b[0;38:5:245m"
	}
	return "\x1b[0;90m\x1b[0;38:5:245m"
}

func (c outputColors) log(priority int, audit bool) (on, off, highlight string) {
	if c.level == 0 {
		return "", "", ""
	}
	switch {
	case priority <= 3:
		on, highlight = ansiRed, ansiHighlight
	case priority == 4:
		on, highlight = "\x1b[0;1;33m", ansiHighlight
		if c.level != 16 {
			on = "\x1b[0;1;38:5:185m"
			if c.level == 256 {
				on = "\x1b[0;1;33m" + on
			}
		}
	case priority == 5:
		on, highlight = ansiHighlight, ansiRed
	case priority >= 7:
		on, highlight = c.grey(), ansiRed
	}
	if audit && on == "" {
		on = ansiBlue
	}
	if on != "" {
		off = ansiNormal
	}
	return
}

func (c outputColors) boot(id string) string {
	text := "-- Boot " + id + " --\n"
	if c.level == 0 {
		return text
	}
	return ansiHighlight + strings.TrimSuffix(text, "\n") + ansiNormal + "\n"
}

func (c outputColors) tableHeader(id string) string {
	if c.level == 0 || !c.underline {
		return "IDX " + id + strings.Repeat(" ", 33-len(id)) + "FIRST ENTRY                 LAST ENTRY\n"
	}
	on := ""
	if c.underline {
		on = "\x1b[4m"
	}
	fields := []string{"IDX", id + strings.Repeat(" ", 32-len(id)), "FIRST ENTRY                ", "LAST ENTRY                 "}
	return on + strings.Join(fields, ansiNormal+on+" "+ansiNormal+on) + ansiNormal + "\n"
}

// Color the existing JSON encoding only when requested; framing stays with its writer.
func colorJSON(data []byte) []byte {
	var out strings.Builder
	for i := 0; i < len(data); {
		start := i
		switch b := data[i]; {
		case b == '"':
			i++
			for i < len(data) && data[i] != '"' {
				if data[i] == '\\' {
					i++
				}
				i++
			}
			out.WriteByte('"')
			out.WriteString(ansiGreen)
			out.Write(data[start+1 : i])
			out.WriteString(ansiNormal)
			out.WriteByte('"')
			i++
		case b == '-' || b >= '0' && b <= '9' || b == 'n' || b == 't' || b == 'f':
			on := "\x1b[0;1;34m"
			if b == 'n' || b == 't' || b == 'f' {
				on = ansiHighlight
			}
			for i < len(data) && !strings.ContainsRune(",]} \t\r\n", rune(data[i])) {
				i++
			}
			out.WriteString(on)
			out.Write(data[start:i])
			out.WriteString(ansiNormal)
		default:
			out.WriteByte(b)
			i++
		}
	}
	return []byte(out.String())
}
