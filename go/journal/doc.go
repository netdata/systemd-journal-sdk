// Package journal implements pure-Go systemd journal file reading and writing.
//
// The writer targets regular and compact keyed-hash journal files with optional
// DATA object compression. It can create new single journal files, reopen files
// created by this package for appending, and manage a journal directory with
// rotation and retention through Log. Writers accept structured binary-safe
// fields and raw complete KEY=value byte payloads. The reader handles regular
// and compact journal files, file-backed directory traversal, binary field
// values, cursors, matching, export/json formatting, and compressed DATA
// objects.
package journal
