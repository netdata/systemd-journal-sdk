//go:build linux

package journal

func readHostBootID() (UUID, error) {
	return readUUIDFile("/proc/sys/kernel/random/boot_id")
}
