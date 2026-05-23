// Package journal implements pure-Go systemd journal file writing.
//
// The first writer implementation targets regular, uncompressed, keyed-hash
// journal files. It can create new journal files and reopen files created by
// this package for appending. Field values are byte slices, so callers can
// write binary journal values through Append.
package journal
