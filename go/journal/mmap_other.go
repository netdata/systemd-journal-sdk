//go:build !unix

package journal

import (
	"fmt"
	"os"
)

type mappedArena struct {
	file *os.File
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
	if size > uint64(int64(^uint64(0)>>1)) {
		return fmt.Errorf("%w: mapped arena too large", errInvalidJournal)
	}
	if err := a.file.Truncate(int64(size)); err != nil {
		return err
	}
	a.size = size
	return nil
}

func (a *mappedArena) checkBounds(offset, size uint64) error {
	end, ok := checkedAdd(offset, size)
	if !ok || end > a.size {
		return fmt.Errorf("%w: mapped arena access out of bounds", errInvalidJournal)
	}
	return nil
}

func (a *mappedArena) directBytesAt(offset, size uint64) ([]byte, bool, error) {
	if err := a.checkBounds(offset, size); err != nil {
		return nil, false, err
	}
	return nil, false, nil
}

func (a *mappedArena) readAt(dst []byte, offset uint64) error {
	if err := a.checkBounds(offset, uint64(len(dst))); err != nil {
		return err
	}
	_, err := a.file.ReadAt(dst, int64(offset))
	return err
}

func (a *mappedArena) writeAt(offset uint64, src []byte) error {
	if err := a.checkBounds(offset, uint64(len(src))); err != nil {
		return err
	}
	_, err := a.file.WriteAt(src, int64(offset))
	return err
}

func (a *mappedArena) sync() error {
	return a.file.Sync()
}

func (a *mappedArena) close() error {
	a.size = 0
	return nil
}
