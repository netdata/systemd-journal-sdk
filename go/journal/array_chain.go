package journal

type arrayHeaderReader interface {
	readOffsetArrayHeader(uint64) (offsetArrayHeader, uint64, error)
}

// Exceptional links need history, but only already visited headers are relevant.
func rejectArrayRevisit(reader arrayHeaderReader, head, current, next uint64, cycleErr error) error {
	var chain chainGuard
	for offset := head; offset != 0; {
		if offset == next {
			return cycleErr
		}
		if offset == current {
			return nil
		}
		header, _, err := reader.readOffsetArrayHeader(offset)
		if err != nil {
			return err
		}
		if !chain.advance(offset, header.nextArrayOffset, false) {
			return cycleErr
		}
		offset = header.nextArrayOffset
	}
	return cycleErr
}
