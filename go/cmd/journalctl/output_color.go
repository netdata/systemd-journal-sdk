//go:build cgo

package main

import (
	"bytes"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func entryPriority(entry *journal.Entry, first bool) int {
	value, _ := lastEntryValue(entry, "PRIORITY")
	if first {
		if values := entryValues(entry, "PRIORITY"); len(values) > 0 {
			value = values[0]
		}
	}
	if len(value) == 1 && value[0] >= '0' && value[0] <= '7' {
		return int(value[0] - '0')
	}
	return 6
}

func advanceHighlight(original matchRange, shift *matchRange, at, size int) {
	if at < original.start {
		shift.start += size
	}
	if at < original.end {
		shift.end += size
	}
}

func displayColoredMessage(entry *journal.Entry, options outputOptions, prefix, columns int, span matchRange) string {
	value, _ := lastEntryValue(entry, "MESSAGE")
	if !options.showAll {
		if start := bytes.IndexAny(value, "\t\r\x1b"); start >= 0 {
			value = stripShortControlsWithHighlight(value, start, &span)
		}
	}
	if options.truncateNewline {
		value = truncateAtNewline(value)
	}
	documentation := ""
	if options.urlify {
		if url, ok := shortField(entry, "DOCUMENTATION", options); ok {
			documentation = documentationLink(url)
		}
	}
	if !options.showAll && !journalTextPrintable(value) {
		return documentation + blobData(len(value))
	}
	if config, ok := lastEntryValue(entry, "CONFIG_FILE"); ok && bytes.HasPrefix(value, config) &&
		(len(value) == len(config) || value[len(config)] == ':' || value[len(config)] == ' ') &&
		(span.start == 0 || span.start > len(config)) {
		if linked := fileLink(config, options.urlify); linked != nil && len(linked) >= len(config) {
			shift := len(linked) - len(config)
			value = append(linked, value[len(config):]...)
			span.start += shift
			span.end += shift
		}
	}
	transport, _ := lastEntryValue(entry, "_TRANSPORT")
	on, off, highlight := options.colors.log(entryPriority(entry, false), string(cStringBytes(transport)) == "audit")
	var out strings.Builder
	out.WriteString(documentation)
	for start, line := 0, 0; start < len(value); line++ {
		end := bytes.IndexByte(value[start:], '\n')
		if end < 0 {
			end = len(value)
		} else {
			end += start
		}
		part := value[start:end]
		tail := line == 2 || end+1 >= printCharThreshold
		shorten := !options.showAll && !options.fullWidth && (prefix+len(part)+1 >= columns || tail)
		if line > 0 {
			out.WriteByte('\n')
		}
		if shorten && columns-prefix < 3 {
			out.WriteString("...")
		} else {
			if line > 0 {
				out.WriteString(strings.Repeat(" ", prefix))
			}
			out.WriteString(on)
			if !shorten {
				if start <= span.start && span.start < len(part) {
					out.Write(cStringBytes(part[:span.start]))
					out.WriteString(highlight)
					out.Write(cStringBytes(part[span.start:min(span.end, len(part))]))
					if len(part) > span.end {
						out.WriteString(on)
						out.Write(cStringBytes(part[span.end:]))
					}
				} else {
					out.Write(cStringBytes(part))
				}
			} else if columns-prefix > len(part)+3 {
				out.Write(cStringBytes(part))
				out.WriteString("...")
			} else {
				percent := 90
				if tail {
					percent = 100
				}
				out.Write(cStringBytes(ellipsizeLine(part, columns-prefix, percent)))
			}
			out.WriteString(off)
		}
		if shorten && tail {
			break
		}
		start = end + 1
	}
	return out.String()
}
