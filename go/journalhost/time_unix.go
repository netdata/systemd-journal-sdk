//go:build linux || freebsd || darwin

package journalhost

import "golang.org/x/sys/unix"

func timespecUsec(ts unix.Timespec) uint64 {
	return uint64(ts.Sec)*1_000_000 + uint64(ts.Nsec)/1_000
}
