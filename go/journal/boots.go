package journal

import (
	"bytes"
	"sort"
)

// bootRange is one boot's first and last entries within a reader.
type bootRange struct {
	bootID      UUID
	first, last directoryEntryKey
}

// BootListOptions selects how boots are listed.
type BootListOptions struct {
	// HeaderFallback lists the boot IDs in ENTRY headers when the journal has
	// no _BOOT_ID DATA at all, which the core writer can produce. journalctl
	// --list-boots reports no boots for such a journal.
	HeaderFallback bool
}

// bootRanges returns the first and last visible entries of every boot that
// has _BOOT_ID DATA in this file, from the DATA objects' entry postings, and
// whether the file has any _BOOT_ID DATA. This is what a _BOOT_ID match with
// SeekHead/SeekTail finds, without decoding entries or touching the caller's
// matches and position.
func (r *Reader) bootRanges() ([]bootRange, bool, error) {
	// A long-lived reader must describe the file as it is now, like Next does.
	if _, err := r.refreshEntryOffsets(); err != nil {
		return nil, false, err
	}
	if len(r.entryOffsets) == 0 {
		return nil, false, nil
	}
	lastVisible := r.entryOffsets[len(r.entryOffsets)-1]
	var ranges []bootRange
	hasData := false
	err := r.visitFieldDataObjects([]byte("_BOOT_ID"), func(offset uint64, _ dataHeader, payload []byte) error {
		value, ok := bytes.CutPrefix(payload, []byte("_BOOT_ID="))
		if !ok {
			return nil
		}
		hasData = true
		bootID, ok := parseCanonicalBootID(value)
		if !ok {
			return nil
		}
		posting, err := r.newFilterPostingCursor(offset)
		if err != nil {
			return err
		}
		if err := posting.reset(r, DirectionForward); err != nil {
			return err
		}
		firstOffset, err := posting.seek(r, 0, DirectionForward)
		if err != nil || firstOffset == 0 || firstOffset > lastVisible {
			return err
		}
		if err := posting.reset(r, DirectionBackward); err != nil {
			return err
		}
		lastOffset, err := posting.seek(r, lastVisible, DirectionBackward)
		if err != nil || lastOffset == 0 {
			return err
		}
		first, err := r.entryKeyAt(firstOffset)
		if err != nil {
			return err
		}
		last, err := r.entryKeyAt(lastOffset)
		if err != nil {
			return err
		}
		ranges = append(ranges, bootRange{bootID: bootID, first: first, last: last})
		return nil
	})
	return ranges, hasData, err
}

// parseCanonicalBootID accepts only the 32 lowercase hex digits journald
// writes: journalctl matches "_BOOT_ID=<id>" in that form, so other spellings
// never belong to a boot it lists. The all-zero ID is not a boot.
func parseCanonicalBootID(value []byte) (UUID, bool) {
	var id UUID
	if len(value) != 2*len(id) {
		return id, false
	}
	for i := range id {
		hi, okHi := lowerHexDigit(value[2*i])
		lo, okLo := lowerHexDigit(value[2*i+1])
		if !okHi || !okLo {
			return UUID{}, false
		}
		id[i] = hi<<4 | lo
	}
	return id, id != (UUID{})
}

func lowerHexDigit(c byte) (byte, bool) {
	switch {
	case c >= '0' && c <= '9':
		return c - '0', true
	case c >= 'a' && c <= 'f':
		return c - 'a' + 10, true
	}
	return 0, false
}

// bootTimes is a boot's first and last realtime.
type bootTimes struct {
	bootID      UUID
	first, last uint64
}

// scanBootTimes is the fallback for journals without _BOOT_ID DATA: it reads
// every visible entry header and keeps each boot's lowest and highest
// realtime.
func (r *Reader) scanBootTimes(boots map[UUID]*bootTimes) error {
	for _, offset := range r.entryOffsets {
		hdr, err := r.readEntryHeaderAt(offset)
		if err != nil {
			return err
		}
		if hdr.bootID == (UUID{}) {
			continue
		}
		if boot := boots[hdr.bootID]; boot == nil {
			boots[hdr.bootID] = &bootTimes{bootID: hdr.bootID, first: hdr.realtime, last: hdr.realtime}
		} else {
			boot.first = min(boot.first, hdr.realtime)
			boot.last = max(boot.last, hdr.realtime)
		}
	}
	return nil
}

// ListBoots lists the boots in this file, oldest first, as journalctl
// --list-boots does. It does not change matches or the read position.
func (r *Reader) ListBoots() ([]BootInfo, error) {
	return r.ListBootsWithOptions(BootListOptions{})
}

// ListBootsWithOptions is ListBoots with the given options.
func (r *Reader) ListBootsWithOptions(options BootListOptions) ([]BootInfo, error) {
	ranges, hasData, err := r.bootRanges()
	if err != nil {
		return nil, err
	}
	if hasData || !options.HeaderFallback {
		times := make([]bootTimes, 0, len(ranges))
		for _, boot := range ranges {
			times = append(times, bootTimes{bootID: boot.bootID, first: boot.first.realtime, last: boot.last.realtime})
		}
		return orderBoots(times), nil
	}
	boots := make(map[UUID]*bootTimes)
	if err := r.scanBootTimes(boots); err != nil {
		return nil, err
	}
	return orderBootMap(boots), nil
}

// ListBoots lists the boots across the directory's files, oldest first, as
// journalctl --list-boots does. A boot's first and last entries are the ones
// a _BOOT_ID match reaches from the head and from the tail of the merged
// directory order. It does not change matches or the read position.
func (dr *DirectoryReader) ListBoots() ([]BootInfo, error) {
	return dr.ListBootsWithOptions(BootListOptions{})
}

// ListBootsWithOptions is ListBoots with the given options. HeaderFallback
// applies only when no file in the directory has _BOOT_ID DATA.
func (dr *DirectoryReader) ListBootsWithOptions(options BootListOptions) ([]BootInfo, error) {
	merged := make(map[UUID]*bootRange)
	var order []UUID
	anyData := false
	for _, r := range dr.files {
		ranges, hasData, err := r.bootRanges()
		if err != nil {
			return nil, err
		}
		anyData = anyData || hasData
		for _, boot := range ranges {
			current := merged[boot.bootID]
			if current == nil {
				copied := boot
				merged[boot.bootID] = &copied
				order = append(order, boot.bootID)
				continue
			}
			if dr.compareEntryKeys(boot.first, current.first) < 0 {
				current.first = boot.first
			}
			if dr.compareEntryKeys(boot.last, current.last) > 0 {
				current.last = boot.last
			}
		}
	}
	if anyData || !options.HeaderFallback {
		times := make([]bootTimes, 0, len(order))
		for _, id := range order {
			boot := merged[id]
			times = append(times, bootTimes{bootID: id, first: boot.first.realtime, last: boot.last.realtime})
		}
		return orderBoots(times), nil
	}
	boots := make(map[UUID]*bootTimes)
	for _, r := range dr.files {
		if err := r.scanBootTimes(boots); err != nil {
			return nil, err
		}
	}
	return orderBootMap(boots), nil
}

func orderBootMap(boots map[UUID]*bootTimes) []BootInfo {
	times := make([]bootTimes, 0, len(boots))
	for _, boot := range boots {
		times = append(times, *boot)
	}
	return orderBoots(times)
}

// orderBoots sorts by last entry, then first entry, then boot ID, and numbers
// the newest boot 0.
func orderBoots(times []bootTimes) []BootInfo {
	sort.Slice(times, func(i, k int) bool {
		if times[i].last != times[k].last {
			return times[i].last < times[k].last
		}
		if times[i].first != times[k].first {
			return times[i].first < times[k].first
		}
		return bytes.Compare(times[i].bootID[:], times[k].bootID[:]) < 0
	})
	out := make([]BootInfo, len(times))
	base := 1 - len(times)
	for i, boot := range times {
		out[i] = BootInfo{
			Index:      int64(base + i),
			BootID:     boot.bootID.String(),
			FirstEntry: int64(boot.first),
			LastEntry:  int64(boot.last),
		}
	}
	return out
}
