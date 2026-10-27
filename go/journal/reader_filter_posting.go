package journal

// Posting cursors retain scalar node metadata, never mapped slices or a copy of
// every matching offset. The inline posting makes the first healthy step lazy.
type filterPostingCursor struct {
	dataOffset uint64
	declared   uint64
	count      uint64
	inline     uint64
	head       uint64
	nodes      []filterPostingNode
	highest    uint64
	node       int
	slot       uint64
	value      uint64
}

type filterPostingNode struct {
	offset    uint64
	next      uint64
	remaining uint64
	length    uint64
}

func (r *Reader) newFilterPostingCursor(offset uint64) (filterPostingCursor, error) {
	posting, err := r.buildFilterPostingCursor(offset)
	if err != nil && filterTailFailure(err) {
		changed, recoveryErr := r.refreshFilterRecovery()
		if recoveryErr != nil {
			return filterPostingCursor{}, recoveryErr
		}
		if changed {
			return r.buildFilterPostingCursor(offset)
		}
	}
	return posting, err
}

func (r *Reader) buildFilterPostingCursor(offset uint64) (filterPostingCursor, error) {
	header, err := r.readDataHeaderAt(offset)
	if err != nil {
		return filterPostingCursor{}, err
	}
	p := filterPostingCursor{dataOffset: offset, declared: header.nEntries, count: header.nEntries, inline: header.entryOffset, head: header.entryArrayOffset, node: -1, value: header.entryOffset}
	if (p.declared == 0) != (p.inline == 0) {
		return filterPostingCursor{}, readerTailError{p.inline}
	}
	if p.declared == 0 {
		return p, nil
	}
	if r.recovery.Incomplete() {
		p.count, err = r.recoveredFilterPostingCount(header)
		if err != nil {
			return filterPostingCursor{}, err
		}
	}
	if p.count > 1 && p.head == 0 {
		return filterPostingCursor{}, readerTailError{0}
	}
	return p, nil
}

func (r *Reader) recoveredFilterPostingCount(header dataHeader) (uint64, error) {
	if len(r.entryOffsets) == 0 || header.nEntries == 0 {
		return 0, nil
	}
	last := r.entryOffsets[len(r.entryOffsets)-1]
	if header.entryOffset > last {
		return 0, nil
	}
	previous, count := header.entryOffset, uint64(1)
	remaining := header.nEntries - 1
	highest := header.entryArrayOffset
	for offset := header.entryArrayOffset; remaining > 0; {
		if offset == 0 {
			if previous == last {
				return count, nil
			}
			return 0, errCorruptObject
		}
		array, capacity, err := r.readOffsetArrayHeader(offset)
		if err != nil {
			if _, tail := readerTailFailure(err); tail && previous == last {
				return count, nil
			}
			return 0, err
		}
		wanted := minUint64(remaining, capacity)
		start := offset + offsetArrayObjectHeaderSize
		available := uint64(0)
		if start <= r.fileSize {
			available = minUint64(wanted, (r.fileSize-start)/r.offsetArrayItemSize())
		}
		for i := uint64(0); i < available; i++ {
			entry, err := r.filterPostingSlot(offset, i)
			if err != nil {
				return 0, err
			}
			if entry == 0 {
				if previous == last {
					return count, nil
				}
				return 0, errCorruptObject
			}
			if entry%8 != 0 || entry <= previous {
				return 0, errCorruptObject
			}
			if entry > last {
				return count, nil
			}
			previous = entry
			count++
		}
		if available < wanted {
			if previous == last {
				return count, nil
			}
			return 0, readerBoundsError{start, wanted * r.offsetArrayItemSize(), r.fileSize}
		}
		remaining -= wanted
		if remaining > 0 && array.nextArrayOffset != 0 {
			if array.nextArrayOffset <= highest {
				if err := rejectArrayRevisit(r, header.entryArrayOffset, offset, array.nextArrayOffset, errCorruptObject); err != nil {
					return 0, err
				}
			} else {
				highest = array.nextArrayOffset
			}
		}
		offset = array.nextArrayOffset
	}
	return count, nil
}

func (r *Reader) filterPostingSlot(offset, index uint64) (uint64, error) {
	size := r.offsetArrayItemSize()
	position, ok := checkedAdd(offset, offsetArrayObjectHeaderSize)
	if !ok || index > (^uint64(0)-position)/size {
		return 0, errCorruptObject
	}
	buf, err := r.readSlice(position+index*size, size)
	if err != nil {
		return 0, err
	}
	return entryOffsetArrayItem(buf, size), nil
}

func (p *filterPostingCursor) appendNode(r *Reader) error {
	remaining, offset := p.count-1, p.head
	if len(p.nodes) != 0 {
		last := p.nodes[len(p.nodes)-1]
		remaining = last.remaining - last.length
		offset = last.next
		if remaining > 0 && offset <= p.highest {
			for _, node := range p.nodes {
				if node.offset == offset {
					return errCorruptObject
				}
			}
		}
	}
	if remaining == 0 || offset == 0 {
		return readerTailError{offset}
	}
	array, capacity, err := r.readOffsetArrayHeader(offset)
	if err != nil {
		return err
	}
	if !r.recovery.Incomplete() && (offset > r.fileSize || array.object.size > r.fileSize-offset) {
		return readerBoundsError{offset, array.object.size, r.fileSize}
	}
	p.nodes = append(p.nodes, filterPostingNode{offset: offset, next: array.nextArrayOffset, remaining: remaining, length: minUint64(remaining, capacity)})
	p.highest = max(p.highest, offset)
	return nil
}

func (p *filterPostingCursor) reset(r *Reader, direction Direction) error {
	p.node, p.slot, p.value = -1, 0, p.inline
	if p.count < 2 || direction == DirectionForward {
		return nil
	}
	for len(p.nodes) == 0 || p.nodes[len(p.nodes)-1].remaining > p.nodes[len(p.nodes)-1].length {
		if err := p.appendNode(r); err != nil {
			return err
		}
	}
	index := len(p.nodes) - 1
	slot := p.nodes[index].length - 1
	p.node, p.slot, p.value = index, slot, 0
	return nil
}

func (p *filterPostingCursor) readSlot(r *Reader, node int, slot uint64) (uint64, error) {
	value, err := r.filterPostingSlot(p.nodes[node].offset, slot)
	if err != nil {
		return 0, err
	}
	if value == 0 {
		return 0, readerTailError{0}
	}
	return value, nil
}

func (p *filterPostingCursor) seek(r *Reader, needle uint64, direction Direction) (uint64, error) {
	if p.count == 0 {
		return 0, nil
	}
	if p.value == 0 {
		if p.node == -2 {
			if err := p.reset(r, DirectionBackward); err != nil {
				return 0, err
			}
		}
		if p.value == 0 {
			value, err := p.readSlot(r, p.node, p.slot)
			if err != nil {
				return 0, err
			}
			p.value = value
		}
	}
	for {
		if direction == DirectionForward && p.value >= needle || direction == DirectionBackward && p.value <= needle {
			return p.value, nil
		}
		node, slot := p.node, p.slot
		if direction == DirectionForward {
			if node < 0 {
				if p.count == 1 {
					return 0, nil
				}
				node, slot = 0, 0
			} else if slot+1 < p.nodes[node].length {
				slot++
			} else if p.nodes[node].remaining > p.nodes[node].length {
				node, slot = node+1, 0
			} else {
				return 0, nil
			}
			if node == len(p.nodes) {
				if err := p.appendNode(r); err != nil {
					return 0, err
				}
			}
		} else if node < 0 {
			return 0, nil
		} else if slot > 0 {
			slot--
		} else if node > 0 {
			node--
			slot = p.nodes[node].length - 1
		} else {
			p.node, p.slot, p.value = -1, 0, p.inline
			continue
		}
		value, err := p.readSlot(r, node, slot)
		if err != nil {
			return 0, err
		}
		p.node, p.slot, p.value = node, slot, value
	}
}
