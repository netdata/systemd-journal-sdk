use super::chain_guard::ChainGuard;
use crate::error::{JournalError, Result};
use std::num::NonZeroU64;

// Exceptional links need history, but only already visited headers matter.
#[cold]
#[inline(never)]
pub(super) fn reject_array_revisit(
    head: NonZeroU64,
    current: NonZeroU64,
    target: NonZeroU64,
    mut next_of: impl FnMut(NonZeroU64) -> Result<Option<NonZeroU64>>,
) -> Result<()> {
    let mut at = head;
    let mut guard = ChainGuard::default();
    loop {
        if at == target {
            return Err(JournalError::CyclicObjectChain);
        }
        if at == current {
            return Ok(());
        }
        let next = next_of(at)?;
        guard.advance(at, next, false)?;
        at = next.ok_or(JournalError::InvalidOffsetArrayOffset)?;
    }
}
