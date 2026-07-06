use super::mmap::MemoryMap;
use crate::error::Result;
use crate::file::{
    EntryItemsType,
    cursor::{JournalCursor, Location},
    file::{EntryDataIterator, FieldDataIterator, FieldIterator, JournalFile},
    filter::{FilterExpr, JournalFilter, LogicalOp},
    object::{DataObject, FieldObject},
    offset_array::Direction,
    value_guard::ValueGuard,
};
use std::num::NonZeroU64;

pub struct JournalReader<'a, M: MemoryMap> {
    cursor: JournalCursor,

    filter: Option<JournalFilter>,
    field_iterator: Option<FieldIterator<'a, M>>,
    field_data_iterator: Option<FieldDataIterator<'a, M>>,
    entry_data_iterator: Option<EntryDataIterator<'a, M>>,

    field_guard: Option<ValueGuard<'a, FieldObject<&'a [u8]>>>,
    data_guard: Option<ValueGuard<'a, DataObject<&'a [u8]>>>,
    raw_payload_guard: Option<ValueGuard<'a, &'a [u8]>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::{JournalFileOptions, JournalWriter, MmapMut};
    use tempfile::TempDir;

    fn test_uuid(seed: u8) -> uuid::Uuid {
        uuid::Uuid::from_bytes([seed; 16])
    }

    fn create_test_journal() -> (TempDir, JournalFile<MmapMut>) {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        let payloads = [b"MESSAGE=test".as_slice(), b"PRIORITY=6".as_slice()];
        writer
            .add_entry(&mut journal_file, &payloads, 1_000_000, 100)
            .expect("write entry");

        (dir, journal_file)
    }

    #[test]
    fn build_filter_returns_expr_and_consumes_pending_filter() {
        let (_dir, journal_file) = create_test_journal();
        let mut reader = JournalReader::<MmapMut>::default();
        reader.add_match(b"MESSAGE=test");

        let expr = reader
            .build_filter(&journal_file)
            .expect("build filter")
            .expect("resolved filter expr");

        assert!(!matches!(expr, FilterExpr::None));
        assert!(reader.filter.is_none(), "pending filter should be consumed");
        assert!(
            reader
                .build_filter(&journal_file)
                .expect("second build")
                .is_none()
        );
    }

    #[test]
    fn build_filter_failure_keeps_pending_filter() {
        let (_dir, journal_file) = create_test_journal();
        let mut reader = JournalReader::<MmapMut>::default();
        reader.filter = Some(JournalFilter::default());

        assert!(reader.build_filter(&journal_file).is_err());
        assert!(
            reader.filter.is_some(),
            "pending filter should remain after build failure"
        );
        assert!(reader.build_filter(&journal_file).is_err());
    }
}

impl<M: MemoryMap> std::fmt::Debug for JournalReader<'_, M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JournalReader")
            // .field("cursor", &self.cursor)
            .field("field_guard", &self.field_guard)
            .field("data_guard", &self.data_guard)
            .finish()
    }
}

impl<M: MemoryMap> Default for JournalReader<'_, M> {
    fn default() -> Self {
        Self {
            cursor: JournalCursor::new(),
            filter: None,
            field_iterator: None,
            field_data_iterator: None,
            entry_data_iterator: None,
            field_guard: None,
            data_guard: None,
            raw_payload_guard: None,
        }
    }
}

impl<'a, M: MemoryMap> JournalReader<'a, M> {
    pub fn dump(&self, _journal_file: &'a JournalFile<M>) -> Result<String> {
        if let Some(_filter_expr) = self.cursor.filter_expr.as_ref() {
            Ok(String::from("filter expr active"))
        } else {
            Ok(String::from("no filter expr"))
        }
    }

    pub fn set_location(&mut self, location: Location) {
        self.cursor.set_location(location)
    }

    pub fn step(&mut self, journal_file: &'a JournalFile<M>, direction: Direction) -> Result<bool> {
        self.drop_guards();

        if let Some(filter) = self.filter.as_mut() {
            let filter_expr = filter.build(journal_file)?;
            self.cursor.set_filter(filter_expr);
            self.filter = None;
        }

        self.cursor.step(journal_file, direction)
    }

    /// Build the pending filter expression (if any) and return it.
    ///
    /// After `add_match` / `add_disjunction` calls, the filter is stored inside
    /// the reader in an unresolved form.  This method resolves it against
    /// `journal_file`'s hash table and returns the resulting [`FilterExpr`].
    /// On success, the internal pending filter is consumed; subsequent calls
    /// return `Ok(None)` until new matches are added. If resolution fails, the
    /// pending filter remains installed so the caller can retry or fall back.
    ///
    /// This is useful when the caller wants to drive iteration through
    /// [`JournalCursor`] directly rather than through [`JournalReader::step`].
    /// The returned filter is not installed on the reader cursor; callers that
    /// need cursor-based iteration should set it on their own cursor.
    pub fn build_filter(&mut self, journal_file: &JournalFile<M>) -> Result<Option<FilterExpr>> {
        self.drop_guards();
        if let Some(filter) = self.filter.as_mut() {
            let expr = filter.build(journal_file)?;
            self.filter = None;
            Ok(Some(expr))
        } else {
            Ok(None)
        }
    }

    pub fn add_match(&mut self, data: &[u8]) {
        self.filter.get_or_insert_default().add_match(data);
    }

    pub fn add_conjunction(&mut self, journal_file: &'a JournalFile<M>) -> Result<()> {
        self.filter
            .get_or_insert_default()
            .set_operation(journal_file, LogicalOp::Conjunction)
    }

    pub fn add_disjunction(&mut self, journal_file: &'a JournalFile<M>) -> Result<()> {
        self.filter
            .get_or_insert_default()
            .set_operation(journal_file, LogicalOp::Disjunction)
    }

    pub fn flush_matches(&mut self) {
        self.cursor.clear_filter();
        self.filter = None;
    }

    pub fn get_realtime_usec(&self, journal_file: &'a JournalFile<M>) -> Result<u64> {
        let entry_offset = self.cursor.position()?;
        let entry_object = journal_file.entry_ref(entry_offset)?;
        Ok(entry_object.header.realtime)
    }

    pub fn get_seqnum(&self, journal_file: &'a JournalFile<M>) -> Result<(u64, [u8; 16])> {
        let entry_offset = self.cursor.position()?;
        let entry_object = journal_file.entry_ref(entry_offset)?;
        Ok((
            entry_object.header.seqnum,
            journal_file.journal_header_ref().seqnum_id,
        ))
    }

    pub fn get_entry_offset(&self) -> Result<NonZeroU64> {
        self.cursor.position()
    }

    fn drop_guards(&mut self) {
        self.field_guard.take();
        self.data_guard.take();
        self.raw_payload_guard.take();
    }

    #[doc(hidden)]
    pub fn release_object_guards(&mut self) {
        self.drop_guards();
    }

    pub fn fields_restart(&mut self) {
        self.drop_guards();
        self.field_iterator = None;
    }

    pub fn fields_enumerate(
        &mut self,
        journal_file: &'a JournalFile<M>,
    ) -> Result<Option<&ValueGuard<'_, FieldObject<&'a [u8]>>>> {
        self.drop_guards();

        if self.field_iterator.is_none() {
            self.field_iterator = Some(journal_file.fields());
        }

        if let Some(iter) = &mut self.field_iterator {
            self.field_guard = iter.next().transpose()?;
            Ok(self.field_guard.as_ref())
        } else {
            Ok(None)
        }
    }

    pub fn field_data_query_unique(
        &mut self,
        journal_file: &'a JournalFile<M>,
        field_name: &[u8],
    ) -> Result<()> {
        self.drop_guards();

        self.field_data_iterator = Some(journal_file.field_data_objects(field_name)?);
        Ok(())
    }

    pub fn field_data_restart(&mut self) {
        self.drop_guards();
        if let Some(iter) = &mut self.field_data_iterator {
            iter.restart();
        }
    }

    pub fn field_data_clear(&mut self) {
        self.drop_guards();
        self.field_data_iterator = None;
    }

    pub fn field_data_enumerate(
        &mut self,
        _: &'a JournalFile<M>,
    ) -> Result<Option<&ValueGuard<'_, DataObject<&'a [u8]>>>> {
        self.drop_guards();

        if let Some(iter) = &mut self.field_data_iterator {
            self.data_guard = iter.next().transpose()?;
            Ok(self.data_guard.as_ref())
        } else {
            Ok(None)
        }
    }

    pub fn entry_data_restart(&mut self) {
        self.drop_guards();
        self.entry_data_iterator = None;
    }

    pub fn entry_data_enumerate(
        &mut self,
        journal_file: &'a JournalFile<M>,
    ) -> Result<Option<&ValueGuard<'_, DataObject<&'a [u8]>>>> {
        self.drop_guards();

        if self.entry_data_iterator.is_none() {
            let entry_offset = self.cursor.position()?;
            self.entry_data_iterator = Some(journal_file.entry_data_objects(entry_offset)?);
        }

        if let Some(iter) = &mut self.entry_data_iterator {
            self.data_guard = iter.next().transpose()?;
            Ok(self.data_guard.as_ref())
        } else {
            Ok(None)
        }
    }

    pub fn data_object_at(
        &mut self,
        journal_file: &'a JournalFile<M>,
        data_offset: NonZeroU64,
    ) -> Result<&ValueGuard<'_, DataObject<&'a [u8]>>> {
        self.drop_guards();
        self.data_guard = Some(journal_file.data_ref(data_offset)?);
        Ok(self.data_guard.as_ref().expect("data guard is present"))
    }

    #[doc(hidden)]
    pub fn raw_data_payload_at(
        &mut self,
        journal_file: &'a JournalFile<M>,
        context: crate::file::file::DataPayloadReadContext,
        info: crate::file::file::DataPayloadObjectInfo,
        data_offset: NonZeroU64,
    ) -> Result<&[u8]> {
        self.drop_guards();
        let guard = journal_file.raw_data_payload_ref_with_info(context, data_offset, info)?;
        self.raw_payload_guard = Some(guard);
        Ok(**self
            .raw_payload_guard
            .as_ref()
            .expect("raw payload guard is present"))
    }

    pub fn entry_data_offsets(
        &self,
        journal_file: &'a JournalFile<M>,
        data_offsets: &mut Vec<NonZeroU64>,
    ) -> Result<()> {
        let entry_offset = self.cursor.position()?;
        let entry_guard = journal_file.entry_ref(entry_offset)?;

        match &entry_guard.items {
            EntryItemsType::Regular(items) => {
                for item in items.iter() {
                    if let Some(offset) = NonZeroU64::new(item.object_offset) {
                        data_offsets.push(offset);
                    }
                }
            }
            EntryItemsType::Compact(items) => {
                for item in items.iter() {
                    if let Some(offset) = NonZeroU64::new(item.object_offset as u64) {
                        data_offsets.push(offset);
                    }
                }
            }
        }

        Ok(())
    }
}
