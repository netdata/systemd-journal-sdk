//go:build cgo && windows

package main

import (
	"os"
	"syscall"
	"time"
)

func vacuumMetadataRealtime(info os.FileInfo, realtime uint64) uint64 {
	if stat, ok := info.Sys().(*syscall.Win32FileAttributeData); ok {
		for _, stamp := range [...]syscall.Filetime{stat.LastAccessTime, stat.CreationTime} {
			realtime = earlierVacuumUsec(realtime, timeToUsec(time.Unix(0, stamp.Nanoseconds())))
		}
	}
	return realtime
}

func vacuumCreationRealtime(_ string, realtime uint64) uint64 {
	return realtime
}
