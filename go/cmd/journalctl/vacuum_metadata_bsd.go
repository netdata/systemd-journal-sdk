//go:build cgo && (darwin || freebsd)

package main

import (
	"os"
	"syscall"
	"time"
)

const vacuumNoAtime = 0

func vacuumMetadataRealtime(info os.FileInfo, realtime uint64) uint64 {
	if stat, ok := info.Sys().(*syscall.Stat_t); ok {
		for _, stamp := range [...]syscall.Timespec{stat.Atimespec, stat.Ctimespec, stat.Birthtimespec} {
			realtime = earlierVacuumUsec(realtime, timeToUsec(time.Unix(stamp.Sec, stamp.Nsec)))
		}
	}
	return realtime
}

func vacuumCreationRealtime(_ string, realtime uint64) uint64 {
	return realtime
}
