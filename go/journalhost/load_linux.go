//go:build linux

package journalhost

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
	"golang.org/x/sys/unix"
)

// loadPlatform is the Linux loader. It reads machine ID, native boot ID,
// and CLOCK_MONOTONIC.
func loadPlatform(opts LoadOptions) (*Provider, *Diagnostics, error) {
	diag := &Diagnostics{
		MachineIDSource: "linux",
		MonotonicSource: "CLOCK_MONOTONIC",
	}
	machineID, machineIDSource, err := loadLinuxMachineID(opts)
	if err != nil {
		return nil, nil, fmt.Errorf("journalhost: machine id: %w", err)
	}
	diag.MachineIDSource = machineIDSource
	bootID, err := loadLinuxBootID(opts)
	if err != nil {
		reason := fmt.Sprintf("linux boot_id unavailable: %v", err)
		bootID, err = freshDegradedBootID(opts, reason)
		if err != nil {
			return nil, nil, fmt.Errorf("journalhost: boot id: %w", err)
		}
		diag.BootIDSource = BootIDSourceDegraded
		diag.DegradedReason = reason
	} else {
		diag.BootIDSource = BootIDSourceNative
	}
	monotonic := func() uint64 {
		var ts unix.Timespec
		_ = unix.ClockGettime(unix.CLOCK_MONOTONIC, &ts)
		return timespecUsec(ts)
	}
	return &Provider{
		machineID:       machineID,
		bootID:          bootID,
		monotonicSource: monotonic,
		monotonicLabel:  "CLOCK_MONOTONIC",
	}, diag, nil
}

func loadLinuxMachineID(opts LoadOptions) (journal.UUID, string, error) {
	return loadLinuxMachineIDFromRoot("/", opts.HostFilesystemPrefix)
}

func loadLinuxMachineIDFromRoot(root string, hostFilesystemPrefix string) (journal.UUID, string, error) {
	if hostFilesystemPrefix != "" {
		for _, candidate := range linuxMachineIDPaths(root, hostFilesystemPrefix, true) {
			id, err := readMachineIDFile(candidate.path)
			if err == nil {
				return id, candidate.source, nil
			}
			if os.IsNotExist(err) {
				continue
			}
			return journal.UUID{}, "", fmt.Errorf("host machine id: %s: %w", candidate.source, err)
		}
	}
	for _, candidate := range linuxMachineIDPaths(root, "", false) {
		id, err := readMachineIDFile(candidate.path)
		if err == nil {
			return id, candidate.source, nil
		}
	}
	return journal.UUID{}, "", fmt.Errorf("no machine id found")
}

func readMachineIDFile(path string) (journal.UUID, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return journal.UUID{}, err
	}
	text := strings.TrimSpace(string(data))
	id, err := parseUUIDText(text)
	if err != nil {
		return journal.UUID{}, err
	}
	if isZeroUUID(id) {
		return journal.UUID{}, fmt.Errorf("machine id is all zeros")
	}
	return id, nil
}

func loadLinuxBootID(opts LoadOptions) (journal.UUID, error) {
	return loadLinuxBootIDFromRoot("/", opts.HostFilesystemPrefix)
}

func loadLinuxBootIDFromRoot(root string, hostFilesystemPrefix string) (journal.UUID, error) {
	const rel = "proc/sys/kernel/random/boot_id"
	if hostFilesystemPrefix != "" {
		source := "linux:" + displayPrefixedPath(hostFilesystemPrefix, rel)
		id, err := readBootIDFile(filepath.Join(rootedPath(root, hostFilesystemPrefix), rel))
		if err == nil {
			return id, nil
		}
		if !os.IsNotExist(err) {
			return journal.UUID{}, fmt.Errorf("host boot id: %s: %w", source, err)
		}
	}
	return readBootIDFile(filepath.Join(root, rel))
}

func readBootIDFile(path string) (journal.UUID, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return journal.UUID{}, err
	}
	return parseUUIDText(strings.TrimSpace(string(data)))
}

type machineIDPath struct {
	path   string
	source string
}

func linuxMachineIDPaths(root string, hostFilesystemPrefix string, hostOnly bool) []machineIDPath {
	relPaths := []string{"etc/machine-id", "var/lib/dbus/machine-id"}
	paths := make([]machineIDPath, 0, len(relPaths)*2)
	if hostFilesystemPrefix != "" {
		hostRoot := rootedPath(root, hostFilesystemPrefix)
		for _, rel := range relPaths {
			paths = append(paths, machineIDPath{
				path:   filepath.Join(hostRoot, rel),
				source: "linux:" + displayPrefixedPath(hostFilesystemPrefix, rel),
			})
		}
	}
	if hostOnly {
		return paths
	}
	for _, rel := range relPaths {
		paths = append(paths, machineIDPath{
			path:   filepath.Join(root, rel),
			source: "linux:/" + filepath.ToSlash(rel),
		})
	}
	return paths
}

func rootedPath(root string, path string) string {
	if filepath.IsAbs(path) {
		volume := filepath.VolumeName(path)
		trimmed := strings.TrimPrefix(path[len(volume):], string(os.PathSeparator))
		return filepath.Join(root, trimmed)
	}
	return filepath.Join(root, path)
}

func displayPrefixedPath(prefix string, rel string) string {
	prefix = filepath.ToSlash(prefix)
	prefix = strings.TrimRight(prefix, "/")
	if prefix == "" || prefix == "/" {
		return "/" + filepath.ToSlash(rel)
	}
	return prefix + "/" + filepath.ToSlash(rel)
}
