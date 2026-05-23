// Package journal implements pure-Go systemd journal file reading and writing.
//
// The writer targets regular, uncompressed, keyed-hash journal files. It can
// create new single journal files, reopen files created by this package for
// appending, and manage a journal directory with rotation and retention through
// Log. The reader handles regular non-compact journal files, file-backed
// directory traversal, binary field values, cursors, matching, export/json
// formatting, and pure-Go zstd decompression for supported fixtures.
package journal
