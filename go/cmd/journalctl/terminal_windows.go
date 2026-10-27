//go:build cgo && windows

package main

import (
	"os"

	"golang.org/x/sys/windows"
)

func outputIsTerminal() bool {
	var mode uint32
	return windows.GetConsoleMode(windows.Handle(os.Stdout.Fd()), &mode) == nil &&
		windows.GetConsoleMode(windows.Handle(os.Stderr.Fd()), &mode) == nil
}

func stdoutColumns() (int, bool) {
	var info windows.ConsoleScreenBufferInfo
	if err := windows.GetConsoleScreenBufferInfo(windows.Handle(os.Stdout.Fd()), &info); err != nil {
		return 0, false
	}
	width := int(info.Window.Right) - int(info.Window.Left) + 1
	return width, width > 0
}
