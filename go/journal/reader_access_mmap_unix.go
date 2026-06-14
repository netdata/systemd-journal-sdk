//go:build unix

package journal

import (
	"fmt"
	"os"
	"syscall"
)

type mmapReaderBackend struct {
	alignment uint64
}

func newMmapReaderBackend(*os.File) (readerWindowBackend, error) {
	pageSize := os.Getpagesize()
	if pageSize <= 0 {
		return nil, fmt.Errorf("%w: invalid mmap page size", errInvalidJournal)
	}
	return &mmapReaderBackend{alignment: uint64(pageSize)}, nil
}

func (*mmapReaderBackend) mode() ReaderAccessMode {
	return ReaderAccessMmap
}

func (b *mmapReaderBackend) mapWindow(file *os.File, base, size uint64) (*readerAccessWindow, error) {
	if size == 0 {
		return nil, fmt.Errorf("%w: reader window maps no bytes", errInvalidJournal)
	}
	alignedBase := (base / b.alignment) * b.alignment
	delta := base - alignedBase
	viewLen, ok := checkedAdd(delta, size)
	if !ok || viewLen > uint64(int(^uint(0)>>1)) || alignedBase > uint64(int64(^uint64(0)>>1)) {
		return nil, fmt.Errorf("%w: mmap reader window too large", errInvalidJournal)
	}
	data, err := syscall.Mmap(
		int(file.Fd()),
		int64(alignedBase),
		int(viewLen),
		syscall.PROT_READ,
		syscall.MAP_SHARED,
	)
	if err != nil {
		return nil, err
	}
	logical := data[int(delta):int(delta+size)]
	return &readerAccessWindow{
		base:        base,
		size:        size,
		data:        logical,
		mappedData:  data,
		mappedBytes: uint64(len(data)),
	}, nil
}

func (*mmapReaderBackend) closeWindow(window *readerAccessWindow) error {
	if len(window.mappedData) == 0 {
		return nil
	}
	err := syscall.Munmap(window.mappedData)
	window.mappedData = nil
	window.data = nil
	return err
}

func (*mmapReaderBackend) refreshFileSize(*os.File, uint64) error {
	return nil
}

func (*mmapReaderBackend) close() error {
	return nil
}
