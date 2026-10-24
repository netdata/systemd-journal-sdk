use crate::error::{JournalError, Result};
use std::num::NonZeroU64;

/// Detects cycles without reads or allocations, retaining acyclic link orders.
#[derive(Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub(super) struct ChainGuard {
    checkpoint: u64,
    steps: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_small_graphs_match_a_visited_set() {
        for size in 1usize..=5 {
            for encoded in 0..(size + 1).pow(size as u32) {
                let (mut links, mut code) = ([0usize; 6], encoded);
                for link in &mut links[1..=size] {
                    *link = code % (size + 1);
                    code /= size + 1;
                }
                for start in 1..=size {
                    let (mut seen, mut at) = (0u8, start);
                    while at != 0 && seen & (1 << at) == 0 {
                        seen |= 1 << at;
                        at = links[at];
                    }
                    let want_cycle = at != 0;
                    for descending in [false, true] {
                        let mut guard = ChainGuard::default();
                        for _ in 0..2 {
                            guard.reset();
                            let (mut at, mut cycle) = (start, false);
                            for _ in 0..4 * size + 4 {
                                let Some(current) = NonZeroU64::new(at as u64) else {
                                    break;
                                };
                                let next = links[at];
                                if guard
                                    .advance(current, NonZeroU64::new(next as u64), descending)
                                    .is_err()
                                {
                                    cycle = true;
                                    break;
                                }
                                at = next;
                            }
                            assert_eq!(cycle, want_cycle, "{links:?}/{start}/{descending}");
                            assert!(cycle || at == 0);
                        }
                    }
                }
            }
        }
    }
}

impl ChainGuard {
    pub(super) fn reset(&mut self) {
        self.steps = 0;
    }

    #[inline]
    pub(super) fn advance(
        &mut self,
        current: NonZeroU64,
        next: Option<NonZeroU64>,
        descending: bool,
    ) -> Result<()> {
        let (current, next) = (current.get(), next.map_or(0, NonZeroU64::get));
        let ordered = if descending {
            next < current
        } else {
            next > current
        };
        if (self.steps == 0) & ordered {
            return Ok(());
        }
        if next == 0 {
            return Ok(());
        }
        self.advance_unordered(current, next)
    }

    #[cold]
    #[inline(never)]
    fn advance_unordered(&mut self, current: u64, next: u64) -> Result<()> {
        if self.steps == 0 {
            self.checkpoint = current;
            self.steps = 1;
        }
        if next == self.checkpoint || self.steps == u64::MAX {
            return Err(JournalError::CyclicObjectChain);
        }
        self.steps += 1;
        if self.steps.is_power_of_two() {
            self.checkpoint = next;
        }
        Ok(())
    }
}
