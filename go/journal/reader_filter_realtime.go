package journal

func seekFilterRealtime(expr filterExpr, r *Reader, needle uint64, direction Direction) (uint64, error) {
	switch value := expr.(type) {
	case *matchExpr:
		return value.posting.seekRealtime(r, needle, direction)
	case falseExpr:
		return 0, nil
	case andExpr:
		initial := needle
		for i := range value {
			index := i
			if direction == DirectionBackward {
				index = len(value) - 1 - i
			}
			candidate, err := seekFilterRealtime(value[index], r, needle, direction)
			if err != nil || candidate == 0 {
				return 0, err
			}
			needle = candidate
		}
		if needle == initial {
			return needle, nil
		}
		return value.seek(r, needle, direction)
	case orExpr:
		var best uint64
		for _, child := range value {
			candidate, err := seekFilterRealtime(child, r, needle, direction)
			if err != nil {
				return 0, err
			}
			if candidate != 0 && (best == 0 || direction == DirectionForward && candidate < best || direction == DirectionBackward && candidate > best) {
				best = candidate
			}
		}
		return best, nil
	default:
		return 0, ErrMalformedFilter
	}
}

func (p *filterPostingCursor) seekRealtime(r *Reader, needle uint64, direction Direction) (uint64, error) {
	p.node, p.slot, p.value = -1, 0, p.inline
	if p.count == 0 {
		return 0, nil
	}
	if direction == DirectionForward && p.inline >= needle {
		return p.inline, nil
	}
	if direction == DirectionBackward && p.inline > needle {
		return 0, nil
	}
	if p.count == 1 {
		if direction == DirectionBackward {
			return p.inline, nil
		}
		return 0, nil
	}
	for index := 0; ; index++ {
		if index == len(p.nodes) {
			if err := p.appendNode(r); err != nil {
				return 0, err
			}
		}
		node := p.nodes[index]
		left, right := uint64(0), node.length
		for left < right {
			mid := left + (right-left)/2
			value, err := p.readSlot(r, index, mid)
			if err != nil {
				return 0, err
			}
			if value < needle || direction == DirectionBackward && value == needle {
				left = mid + 1
			} else {
				right = mid
			}
		}
		if direction == DirectionBackward && left == 0 {
			return p.value, nil
		}
		slot := left
		if direction == DirectionBackward || left == node.length {
			slot--
		}
		value, err := p.readSlot(r, index, slot)
		if err != nil {
			return 0, err
		}
		// Retain the last visited posting even when a sibling wins an OR query.
		p.node, p.slot, p.value = index, slot, value
		if left < node.length {
			return value, nil
		}
		if node.remaining == node.length {
			if direction == DirectionBackward {
				return value, nil
			}
			return 0, nil
		}
	}
}
