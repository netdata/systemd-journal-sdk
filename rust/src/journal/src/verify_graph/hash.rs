use super::*;

impl<'a> GraphVerifier<'a> {
    pub(super) fn hash(&self, payload: &[u8]) -> u64 {
        let keyed = self.header.incompatible_flags & INCOMPATIBLE_KEYED_HASH != 0;
        journal_hash_data(
            payload,
            keyed,
            if keyed {
                Some(&self.header.file_id)
            } else {
                None
            },
        )
    }
}
