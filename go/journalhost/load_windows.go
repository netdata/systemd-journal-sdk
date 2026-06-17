//go:build windows

package journalhost

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"unsafe"

	"github.com/netdata/systemd-journal-sdk/go/journal"
	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/registry"
)

var (
	kernel32                       = windows.NewLazySystemDLL("kernel32.dll")
	procQueryUnbiasedInterruptTime = kernel32.NewProc("QueryUnbiasedInterruptTime")
	procGetTickCount64             = kernel32.NewProc("GetTickCount64")
)

// loadPlatform is the Windows loader. It reads MachineGuid from the registry,
// synthesizes a state-backed boot ID, and uses QueryUnbiasedInterruptTime for
// per-entry monotonic timestamps.
func loadPlatform(opts LoadOptions) (*Provider, *Diagnostics, error) {
	diag := &Diagnostics{MonotonicSource: "QueryUnbiasedInterruptTime"}
	machineID, err := loadWindowsMachineID()
	if err != nil {
		return nil, nil, fmt.Errorf("journalhost: machine id: %w", err)
	}
	bootID, path, degraded, derr := loadWindowsBootID(opts)
	if derr != nil && isZeroUUID(bootID) {
		fresh, ferr := loadHelperNewUUID(opts)()
		if ferr != nil {
			return nil, nil, fmt.Errorf("journalhost: boot id: %w", derr)
		}
		bootID = fresh
	}
	if derr != nil {
		degraded = true
	}
	diag.MachineIDSource = "windows:HKLM\\SOFTWARE\\Microsoft\\Cryptography\\MachineGuid"
	diag.BootIDSource = BootIDSourceStateBacked
	if degraded {
		diag.BootIDSource = BootIDSourceDegraded
		if derr != nil {
			diag.DegradedReason = derr.Error()
		}
	}
	diag.BootIDPath = path
	monotonic := func() uint64 {
		ticks, err := queryUnbiasedInterruptTime()
		if err != nil {
			return 0
		}
		return ticks / 10
	}
	return &Provider{
		machineID:       machineID,
		bootID:          bootID,
		diagnostics:     *diag,
		monotonicSource: monotonic,
		monotonicLabel:  "QueryUnbiasedInterruptTime",
	}, diag, nil
}

func loadWindowsMachineID() (journal.UUID, error) {
	key, err := registry.OpenKey(registry.LOCAL_MACHINE, `SOFTWARE\Microsoft\Cryptography`, registry.QUERY_VALUE)
	if err != nil {
		return journal.UUID{}, fmt.Errorf("open crypt key: %w", err)
	}
	defer key.Close()
	text, _, err := key.GetStringValue("MachineGuid")
	if err != nil {
		return journal.UUID{}, fmt.Errorf("query MachineGuid: %w", err)
	}
	text = strings.TrimSpace(text)
	if text == "" {
		return journal.UUID{}, fmt.Errorf("MachineGuid empty")
	}
	id, err := parseUUIDText(text)
	if err != nil {
		return journal.UUID{}, err
	}
	if isZeroUUID(id) {
		return journal.UUID{}, fmt.Errorf("MachineGuid is all zeros")
	}
	return id, nil
}

func loadWindowsBootID(opts LoadOptions) (journal.UUID, string, bool, error) {
	probe := stateBackedProbe{
		markerNow: func() uint64 {
			return getTickCount64() * 1_000
		},
		realtimeNow:     loadHelperRealtimeNow(opts),
		defaultStateDir: defaultWindowsStateDir(),
		defaultFileName: "bootid.state",
	}
	return loadStateBackedBootID(opts, probe)
}

func defaultWindowsStateDir() string {
	if v := os.Getenv("LOCALAPPDATA"); v != "" {
		return filepath.Join(v, "systemd-journal-sdk")
	}
	if v := os.Getenv("APPDATA"); v != "" {
		return filepath.Join(v, "systemd-journal-sdk")
	}
	return "."
}

func queryUnbiasedInterruptTime() (uint64, error) {
	var ticks uint64
	r1, _, e1 := procQueryUnbiasedInterruptTime.Call(uintptr(unsafe.Pointer(&ticks)))
	if r1 == 0 {
		if e1 != windows.ERROR_SUCCESS {
			return 0, e1
		}
		return 0, fmt.Errorf("QueryUnbiasedInterruptTime failed")
	}
	return ticks, nil
}

func getTickCount64() uint64 {
	r1, _, _ := procGetTickCount64.Call()
	return uint64(r1)
}

func acquirePlatformStateLock(lockPath string) (func() error, error) {
	pathPtr, err := windows.UTF16PtrFromString(lockPath)
	if err != nil {
		return nil, err
	}
	handle, err := windows.CreateFile(
		pathPtr,
		windows.GENERIC_READ|windows.GENERIC_WRITE,
		windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE,
		nil,
		windows.OPEN_ALWAYS,
		windows.FILE_ATTRIBUTE_NORMAL,
		0,
	)
	if err != nil {
		return nil, fmt.Errorf("create lock file: %w", err)
	}
	var overlapped windows.Overlapped
	if err := windows.LockFileEx(handle, windows.LOCKFILE_EXCLUSIVE_LOCK, 0, 1, 0, &overlapped); err != nil {
		_ = windows.CloseHandle(handle)
		return nil, fmt.Errorf("lock: %w", err)
	}
	unlock := func() error {
		err1 := windows.UnlockFileEx(handle, 0, 1, 0, &overlapped)
		err2 := windows.CloseHandle(handle)
		if err1 != nil {
			return err1
		}
		return err2
	}
	return unlock, nil
}

func fsyncDirectoryBestEffort(_ string) error {
	return nil
}
