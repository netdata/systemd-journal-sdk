//go:build windows

package journal

import (
	"os"
	"syscall"
)

func openWriterFile(path string, create bool, _ os.FileMode) (*os.File, error) {
	access := uint32(syscall.GENERIC_READ | syscall.GENERIC_WRITE)
	mode := uint32(syscall.OPEN_EXISTING)
	if create {
		mode = syscall.OPEN_ALWAYS
	}
	return openWindowsFile(path, access, mode)
}

func openReaderFile(path string) (*os.File, error) {
	return openWindowsFile(path, syscall.GENERIC_READ, syscall.OPEN_EXISTING)
}

func openWindowsFile(path string, access, mode uint32) (*os.File, error) {
	name, err := syscall.UTF16PtrFromString(path)
	if err != nil {
		return nil, err
	}
	handle, err := syscall.CreateFile(
		name,
		access,
		syscall.FILE_SHARE_READ|syscall.FILE_SHARE_WRITE|syscall.FILE_SHARE_DELETE,
		nil,
		mode,
		syscall.FILE_ATTRIBUTE_NORMAL,
		0,
	)
	if err != nil {
		return nil, err
	}
	f := os.NewFile(uintptr(handle), path)
	if f == nil {
		_ = syscall.CloseHandle(handle)
		return nil, syscall.EINVAL
	}
	return f, nil
}
