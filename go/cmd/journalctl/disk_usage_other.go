//go:build !unix

package main

import "os"

func allocatedBytes(info os.FileInfo) uint64 {
	return uint64(info.Size())
}
