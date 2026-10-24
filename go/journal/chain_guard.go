package journal

// chainGuard accepts acyclic links in either order without additional reads.
// Normal journal ordering needs no cycle-tracking updates.
type chainGuard struct {
	checkpoint uint64
	steps      uint64
}

func (g *chainGuard) advance(current, next uint64, descending bool) bool {
	if next == 0 {
		return true
	}
	if g.steps == 0 && ((descending && next < current) || (!descending && next > current)) {
		return true
	}
	return g.advanceUnordered(current, next)
}

func (g *chainGuard) advanceUnordered(current, next uint64) bool {
	if g.steps == 0 {
		g.checkpoint = current
		g.steps = 1
	}
	if next == g.checkpoint || g.steps == ^uint64(0) {
		return false
	}
	g.steps++
	// Power-of-two checkpoints encode Brent's doubling schedule without distance.
	if g.steps&(g.steps-1) == 0 {
		g.checkpoint = next
	}
	return true
}
