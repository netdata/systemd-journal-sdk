//go:build cgo && unix

package main

/*
#include <unistd.h>
static int journal_output_isatty(void) { return isatty(STDOUT_FILENO) && isatty(STDERR_FILENO); }
*/
import "C"

import (
	"os"

	"golang.org/x/sys/unix"
)

func outputIsTerminal() bool { return C.journal_output_isatty() == 1 }

func stdoutColumns() (int, bool) {
	size, err := unix.IoctlGetWinsize(int(os.Stdout.Fd()), unix.TIOCGWINSZ)
	if err != nil || size.Col == 0 {
		return 0, false
	}
	return int(size.Col), true
}
