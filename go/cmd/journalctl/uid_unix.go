//go:build unix

package main

import (
	"os"
	"strconv"
)

func currentUIDString() (string, bool) {
	return strconv.Itoa(os.Getuid()), true
}
