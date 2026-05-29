//go:build unix

package journal

import (
	"fmt"
	"os"
	"syscall"
	"unsafe"
)

type mappedArena struct {
	file *os.File
	data []byte
	size uint64
}

type readOnlyMapping struct {
	file *os.File
	data []byte
	size uint64
}

func checkedAdd(a, b uint64) (uint64, bool) {
	if a > ^uint64(0)-b {
		return 0, false
	}
	return a + b, true
}

func newMappedArena(file *os.File, size uint64) (*mappedArena, error) {
	arena := &mappedArena{file: file}
	if err := arena.remap(size); err != nil {
		return nil, err
	}
	return arena, nil
}

func (a *mappedArena) remap(size uint64) error {
	if size == a.size && len(a.data) > 0 {
		return nil
	}
	if size > uint64(int(^uint(0)>>1)) {
		return fmt.Errorf("%w: mapped arena too large", errInvalidJournal)
	}
	if len(a.data) > 0 {
		if err := syscall.Munmap(a.data); err != nil {
			return err
		}
		a.data = nil
		a.size = 0
	}
	if err := a.file.Truncate(int64(size)); err != nil {
		return err
	}
	if size == 0 {
		return nil
	}
	data, err := syscall.Mmap(
		int(a.file.Fd()),
		0,
		int(size),
		syscall.PROT_READ|syscall.PROT_WRITE,
		syscall.MAP_SHARED,
	)
	if err != nil {
		return err
	}
	a.data = data
	a.size = size
	return nil
}

func (a *mappedArena) bytesAt(offset, size uint64) ([]byte, error) {
	end, ok := checkedAdd(offset, size)
	if !ok || end > a.size || end > uint64(len(a.data)) {
		return nil, fmt.Errorf("%w: mapped arena access out of bounds", errInvalidJournal)
	}
	return a.data[int(offset):int(end)], nil
}

func (a *mappedArena) readAt(dst []byte, offset uint64) error {
	src, err := a.bytesAt(offset, uint64(len(dst)))
	if err != nil {
		return err
	}
	copy(dst, src)
	return nil
}

func (a *mappedArena) writeAt(offset uint64, src []byte) error {
	dst, err := a.bytesAt(offset, uint64(len(src)))
	if err != nil {
		return err
	}
	copy(dst, src)
	return nil
}

func (a *mappedArena) sync() error {
	if len(a.data) > 0 {
		_, _, errno := syscall.Syscall(
			syscall.SYS_MSYNC,
			uintptr(unsafe.Pointer(&a.data[0])),
			uintptr(len(a.data)),
			uintptr(syscall.MS_SYNC),
		)
		if errno != 0 {
			return errno
		}
	}
	return a.file.Sync()
}

func (a *mappedArena) close() error {
	if len(a.data) == 0 {
		a.size = 0
		return nil
	}
	err := syscall.Munmap(a.data)
	a.data = nil
	a.size = 0
	return err
}

func newReadOnlyMapping(file *os.File) (*readOnlyMapping, error) {
	m := &readOnlyMapping{file: file}
	if err := m.remap(); err != nil {
		return nil, err
	}
	return m, nil
}

func (m *readOnlyMapping) remap() error {
	info, err := m.file.Stat()
	if err != nil {
		return err
	}
	size := uint64(info.Size())
	if size == m.size && len(m.data) > 0 {
		return nil
	}
	if size > uint64(int(^uint(0)>>1)) {
		return fmt.Errorf("%w: mapped reader file too large", errInvalidJournal)
	}
	oldData := m.data
	oldSize := m.size
	m.data = nil
	m.size = 0
	if size == 0 {
		if len(oldData) > 0 {
			if err := syscall.Munmap(oldData); err != nil {
				m.data = oldData
				m.size = oldSize
				return err
			}
		}
		return nil
	}
	data, err := syscall.Mmap(
		int(m.file.Fd()),
		0,
		int(size),
		syscall.PROT_READ,
		syscall.MAP_SHARED,
	)
	if err != nil {
		m.data = oldData
		m.size = oldSize
		return err
	}
	if len(oldData) > 0 {
		if err := syscall.Munmap(oldData); err != nil {
			_ = syscall.Munmap(data)
			m.data = oldData
			m.size = oldSize
			return err
		}
	}
	m.data = data
	m.size = size
	return nil
}

func (m *readOnlyMapping) bytesAt(offset, size uint64) ([]byte, error) {
	end, ok := checkedAdd(offset, size)
	if !ok || end > m.size || end > uint64(len(m.data)) {
		return nil, fmt.Errorf("%w: mapped reader access out of bounds", errInvalidJournal)
	}
	return m.data[int(offset):int(end)], nil
}

func (m *readOnlyMapping) readAt(dst []byte, offset uint64) error {
	src, err := m.bytesAt(offset, uint64(len(dst)))
	if err != nil {
		return err
	}
	copy(dst, src)
	return nil
}

func (m *readOnlyMapping) close() error {
	if len(m.data) == 0 {
		m.size = 0
		return nil
	}
	err := syscall.Munmap(m.data)
	m.data = nil
	m.size = 0
	return err
}
