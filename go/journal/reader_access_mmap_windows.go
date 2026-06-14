//go:build windows

package journal

import (
	"fmt"
	"os"
	"runtime"
	"syscall"
	"unsafe"
)

var kernel32GetSystemInfo = kernel32.NewProc("GetSystemInfo")

type windowsSystemInfo struct {
	processorArchitecture uint16
	reserved              uint16
	pageSize              uint32
	minimumAppAddress     uintptr
	maximumAppAddress     uintptr
	activeProcessorMask   uintptr
	numberOfProcessors    uint32
	processorType         uint32
	allocationGranularity uint32
	processorLevel        uint16
	processorRevision     uint16
}

type mmapReaderBackend struct {
	handle      syscall.Handle
	granularity uint64
	mappedSize  uint64
}

func newMmapReaderBackend(file *os.File) (readerWindowBackend, error) {
	granularity, err := windowsAllocationGranularity()
	if err != nil {
		return nil, err
	}
	size, err := currentFileSize(file)
	if err != nil {
		return nil, err
	}
	handle, err := createWindowsReadOnlyFileMapping(file)
	if err != nil {
		return nil, err
	}
	return &mmapReaderBackend{handle: handle, granularity: granularity, mappedSize: size}, nil
}

func windowsAllocationGranularity() (uint64, error) {
	var info windowsSystemInfo
	kernel32GetSystemInfo.Call(uintptr(unsafe.Pointer(&info)))
	if info.allocationGranularity == 0 {
		return 0, fmt.Errorf("%w: invalid Windows allocation granularity", errInvalidJournal)
	}
	return uint64(info.allocationGranularity), nil
}

func (*mmapReaderBackend) mode() ReaderAccessMode {
	return ReaderAccessMmap
}

func (b *mmapReaderBackend) mapWindow(_ *os.File, base, size uint64) (*readerAccessWindow, error) {
	if size == 0 {
		return nil, fmt.Errorf("%w: reader window maps no bytes", errInvalidJournal)
	}
	alignedBase := (base / b.granularity) * b.granularity
	delta := base - alignedBase
	viewLen, ok := checkedAdd(delta, size)
	if !ok || viewLen > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("%w: mmap reader window too large", errInvalidJournal)
	}
	offsetHigh := uint32(alignedBase >> 32)
	offsetLow := uint32(alignedBase)
	addr, err := syscall.MapViewOfFile(
		b.handle,
		syscall.FILE_MAP_READ,
		offsetHigh,
		offsetLow,
		uintptr(viewLen),
	)
	if err != nil {
		return nil, err
	}
	logicalAddr := addr + uintptr(delta)
	data := unsafe.Slice((*byte)(unsafe.Pointer(logicalAddr)), int(size))
	return &readerAccessWindow{
		base:        base,
		size:        size,
		data:        data,
		mappedBytes: viewLen,
		viewAddr:    addr,
	}, nil
}

func (*mmapReaderBackend) closeWindow(window *readerAccessWindow) error {
	if window.viewAddr == 0 {
		return nil
	}
	err := syscall.UnmapViewOfFile(window.viewAddr)
	runtime.KeepAlive(window.data)
	window.viewAddr = 0
	window.data = nil
	return err
}

func (b *mmapReaderBackend) refreshFileSize(file *os.File, size uint64) error {
	if size <= b.mappedSize {
		return nil
	}
	handle, err := createWindowsReadOnlyFileMapping(file)
	if err != nil {
		return err
	}
	old := b.handle
	b.handle = handle
	b.mappedSize = size
	if old != 0 {
		return syscall.CloseHandle(old)
	}
	return nil
}

func (b *mmapReaderBackend) close() error {
	if b.handle == 0 {
		return nil
	}
	err := syscall.CloseHandle(b.handle)
	b.handle = 0
	return err
}

func currentFileSize(file *os.File) (uint64, error) {
	info, err := file.Stat()
	if err != nil {
		return 0, err
	}
	if info.Size() < 0 {
		return 0, fmt.Errorf("%w: negative reader file size", errInvalidJournal)
	}
	return uint64(info.Size()), nil
}

func createWindowsReadOnlyFileMapping(file *os.File) (syscall.Handle, error) {
	return syscall.CreateFileMapping(
		syscall.Handle(file.Fd()),
		nil,
		syscall.PAGE_READONLY,
		0,
		0,
		nil,
	)
}
