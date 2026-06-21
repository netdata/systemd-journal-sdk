//go:build unix

package main

import (
	"os"
	"syscall"
)

func allocatedBytes(info os.FileInfo) uint64 {
	if stat, ok := info.Sys().(*syscall.Stat_t); ok {
		return uint64(stat.Blocks) * 512
	}
	return uint64(info.Size())
}
