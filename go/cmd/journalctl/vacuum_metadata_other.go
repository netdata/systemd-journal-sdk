//go:build cgo && !linux && !darwin && !freebsd && !windows

package main

import "os"

const vacuumNoAtime = 0

func vacuumMetadataRealtime(_ os.FileInfo, realtime uint64) uint64 {
	return realtime
}

func vacuumCreationRealtime(_ string, realtime uint64) uint64 {
	return realtime
}
