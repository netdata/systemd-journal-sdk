package journal

import (
	"errors"
	"io"
)

func filterTailFailure(err error) bool {
	_, eligible := readerTailFailure(err)
	return eligible || errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF)
}

// A new group sees current native indexes. Existing compiled leaves keep their
// captured population; refreshing visibility must not walk every ENTRY.
func (r *Reader) prepareFilterGroup() error {
	snapshot := r.refreshSnapshot()
	header, size, err := r.readCurrentHeader()
	if err != nil {
		r.restoreRefreshSnapshot(snapshot)
		return err
	}
	if r.options.Bounds == ReaderBoundsSnapshot || r.staged {
		size = minUint64(size, r.accessor.initialSize())
		r.accessor.restoreVisibleBounds(readerAccessorVisibleSnapshot{fileSize: size})
	}
	r.applyRefreshState(header, size)
	return nil
}

// Recovery must reread published bytes even when the file size and header
// counters are unchanged. Snapshot readers retain their original byte extent.
func (r *Reader) refreshFilterRecovery() (bool, error) {
	if !r.allowRecovery || r.accessor == nil {
		return false, nil
	}
	snapshot := r.refreshSnapshot()
	header, size, err := r.readCurrentHeader()
	if err != nil {
		r.restoreRefreshSnapshot(snapshot)
		return false, err
	}
	if r.options.Bounds == ReaderBoundsSnapshot || r.staged {
		size = minUint64(size, r.accessor.initialSize())
		r.accessor.restoreVisibleBounds(readerAccessorVisibleSnapshot{fileSize: size})
	}
	r.applyRefreshState(header, size)
	if err := r.loadEntryArray(); err != nil {
		r.restoreRefreshSnapshot(snapshot)
		return false, err
	}
	r.entryIndex = snapshot.index
	if snapshot.index >= 0 && snapshot.index >= len(snapshot.offsets) {
		r.entryIndex = len(r.entryOffsets)
	} else if snapshot.index >= 0 && snapshot.index >= len(r.entryOffsets) && r.filter != nil {
		r.filter.overrideOffset = snapshot.offsets[snapshot.index]
		r.entryIndex = -2
	}
	r.clampEntryIndex()
	changed := !sameRecoveryState(r.recovery, snapshot.recovery)
	if changed {
		if r.filter != nil {
			r.filter.stale = true
			r.filter.walkValid = false
		}
		if r.directoryOrderDirty != nil {
			*r.directoryOrderDirty = true
		}
	}
	return changed, nil
}

func (r *Reader) refreshFilterOffsets() error {
	if r.options.Bounds == ReaderBoundsSnapshot || r.staged {
		_, err := r.refreshFilterRecovery()
		return err
	}
	_, err := r.refreshEntryOffsets()
	return err
}
