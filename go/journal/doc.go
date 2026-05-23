// Package journal implements pure-Go systemd journal file writing.
//
// The writer implementation targets regular, uncompressed, keyed-hash journal
// files. It can create new single journal files, reopen files created by this
// package for appending, and manage a journal directory with rotation and
// retention through Log. Field values are byte slices, so callers can write
// binary journal values through Append.
package journal
