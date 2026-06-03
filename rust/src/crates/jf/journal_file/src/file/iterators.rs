use super::*;

/// Iterator that walks through all field objects in the field hash table
pub struct FieldIterator<'a, M: MemoryMap> {
    pub(super) journal: &'a JournalFile<M>,
    pub(super) field_hash_table: Option<FieldHashTable<&'a [u8]>>,
    pub(super) current_bucket_index: usize,
    pub(super) next_field_offset: Option<NonZeroU64>,
}

impl<M: MemoryMap> FieldIterator<'_, M> {
    /// Advances to the next non-empty bucket
    pub(super) fn advance_to_next_nonempty_bucket(&mut self) {
        // If we don't have a hash table, there's nothing to iterate
        let Some(hash_table) = &self.field_hash_table else {
            return;
        };

        let items = &hash_table.items;

        // Find the next non-empty bucket
        while self.current_bucket_index < items.len() {
            let bucket = items[self.current_bucket_index];
            if bucket.head_hash_offset.is_some() {
                self.next_field_offset = bucket.head_hash_offset;
                return;
            }
            self.current_bucket_index += 1;
        }

        // No more non-empty buckets
        self.next_field_offset = None;
    }
}

impl<'a, M: MemoryMap> Iterator for FieldIterator<'a, M> {
    type Item = Result<ValueGuard<'a, FieldObject<&'a [u8]>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let offset = self.next_field_offset?;

        match self.journal.field_ref(offset) {
            Ok(field_guard) => {
                // Get the next field offset before we return the guard
                self.next_field_offset = field_guard.header.next_hash_offset;

                // If we've reached the end of the chain, move to the next bucket
                if self.next_field_offset.is_none() {
                    self.current_bucket_index += 1;
                    self.advance_to_next_nonempty_bucket();
                }

                Some(Ok(field_guard))
            }
            Err(e) => {
                self.next_field_offset = None;
                Some(Err(e))
            }
        }
    }
}

/// Iterator that walks through all DATA objects for a specific field
pub struct FieldDataIterator<'a, M: MemoryMap> {
    pub(super) journal: &'a JournalFile<M>,
    pub(super) current_data_offset: Option<NonZeroU64>,
}

impl<'a, M: MemoryMap> Iterator for FieldDataIterator<'a, M> {
    type Item = Result<ValueGuard<'a, DataObject<&'a [u8]>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let data_offset = self.current_data_offset?;

        match self.journal.data_ref(data_offset) {
            Ok(data_guard) => {
                // Get the next data offset before we return the guard
                self.current_data_offset = data_guard.header.next_field_offset;
                Some(Ok(data_guard))
            }
            Err(e) => {
                self.current_data_offset = None;
                Some(Err(e))
            }
        }
    }
}

/// Iterator that walks through all DATA objects for a specific entry
pub struct EntryDataIterator<'a, M: MemoryMap> {
    pub(super) journal: &'a JournalFile<M>,
    pub(super) entry_offset: Option<NonZeroU64>,
    pub(super) current_index: usize,
    pub(super) total_items: usize,
}

impl<'a, M: MemoryMap> Iterator for EntryDataIterator<'a, M> {
    type Item = Result<ValueGuard<'a, DataObject<&'a [u8]>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry_offset = self.entry_offset?;

        // If we've reached the end of the data indices, return None
        if self.current_index >= self.total_items {
            return None;
        }

        // Get the entry object to access the data offset
        match self.journal.entry_ref(entry_offset) {
            Ok(entry_guard) => {
                let idx = self.current_index;
                self.current_index += 1;

                let data_offset = match &entry_guard.items {
                    EntryItemsType::Regular(items) => {
                        if idx >= items.len() {
                            return None;
                        }
                        items[idx].object_offset
                    }
                    EntryItemsType::Compact(items) => {
                        if idx >= items.len() {
                            return None;
                        }
                        items[idx].object_offset as u64
                    }
                };

                let data_offset = NonZeroU64::new(data_offset)?;

                // Drop the entry guard before obtaining the data object
                drop(entry_guard);

                // Try to get the data object
                match self.journal.data_ref(data_offset) {
                    Ok(data_guard) => Some(Ok(data_guard)),
                    Err(e) => Some(Err(e)),
                }
            }
            Err(e) => {
                // If we can't read the entry, return the error and stop iteration
                self.current_index = self.total_items;
                Some(Err(e))
            }
        }
    }
}
