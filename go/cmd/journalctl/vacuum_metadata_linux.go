//go:build cgo && linux

package main

import (
	"encoding/binary"
	"os"
	"syscall"
	"time"

	"golang.org/x/sys/unix"
)

const vacuumNoAtime = unix.O_NOATIME

func vacuumMetadataRealtime(info os.FileInfo, realtime uint64) uint64 {
	if stat, ok := info.Sys().(*syscall.Stat_t); ok {
		realtime = earlierVacuumUsec(realtime, timeToUsec(time.Unix(int64(stat.Atim.Sec), int64(stat.Atim.Nsec))))
		realtime = earlierVacuumUsec(realtime, timeToUsec(time.Unix(int64(stat.Ctim.Sec), int64(stat.Ctim.Nsec))))
	}
	return realtime
}

func vacuumCreationRealtime(path string, realtime uint64) uint64 {
	var stat unix.Statx_t
	if err := unix.Statx(unix.AT_FDCWD, path, unix.AT_STATX_DONT_SYNC, unix.STATX_BTIME, &stat); err == nil && stat.Mask&unix.STATX_BTIME != 0 && stat.Btime.Sec != 0 {
		realtime = earlierVacuumUsec(realtime, timeToUsec(time.Unix(stat.Btime.Sec, int64(stat.Btime.Nsec))))
	}
	// Older journals may retain their original creation time in this xattr.
	var data [8]byte
	if n, err := unix.Getxattr(path, "user.crtime_usec", data[:]); err == nil && n == len(data) {
		realtime = earlierVacuumUsec(realtime, binary.LittleEndian.Uint64(data[:]))
	}
	return realtime
}
