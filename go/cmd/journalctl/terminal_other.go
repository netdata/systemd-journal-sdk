//go:build cgo && !unix && !windows

package main

func outputIsTerminal() bool { return false }

func stdoutColumns() (int, bool) {
	return 0, false
}
