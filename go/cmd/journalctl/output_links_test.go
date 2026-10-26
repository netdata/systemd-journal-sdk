//go:build cgo

package main

import (
	"path/filepath"
	"runtime"
	"testing"
)

func TestWindowsFileURL(t *testing.T) {
	for _, tc := range []struct{ path, want string }{
		{`C:\logs\example.conf`, "file://host/C:/logs/example.conf"},
		{`\\server\share\é #%.conf`, "file://server/share/%C3%A9%20%23%25.conf"},
		{"C:/logs/a?b", "file://host/C:/logs/a%3Fb"},
		{`\\?\C:\logs`, ""}, {`\\.\pipe\name`, ""}, {"relative", ""}, {"//server", ""},
	} {
		if got := windowsFileURL(tc.path, "host"); got != tc.want {
			t.Errorf("%q: got %q want %q", tc.path, got, tc.want)
		}
	}
}

func TestRelativeWindowsFileLink(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("native Windows path resolution")
	}
	path := "relative #.conf"
	absolute, err := filepath.Abs(path)
	if err != nil {
		t.Fatal(err)
	}
	want := terminalLink(windowsFileURL(absolute, outputHostname()), path)
	if got := string(fileLink([]byte(path), true)); got != want {
		t.Fatalf("got %q want %q", got, want)
	}
	if got := string(fileLink([]byte(path), false)); got != path {
		t.Fatalf("plain got %q", got)
	}
}
