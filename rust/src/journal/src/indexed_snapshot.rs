//! Bounded native-index traversal. Capture requires caller-owned writer exclusion.
use super::*;
use journal_core::file::{HashTable, JournalHeader, JournalState};

/// Cooperative cancellation, checked between objects, chunks, and payloads.
/// A syscall or one decompression is not interruptible.
#[derive(Default)]
pub struct SnapshotControl<'a> {
    pub cancelled: Option<&'a dyn Fn() -> bool>,
}
impl SnapshotControl<'_> {
    pub(crate) fn check(&self) -> Result<()> {
        if self.cancelled.is_some_and(|f| f()) {
            Err(SdkError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Default)]
pub struct IndexedSnapshotOptions {
    pub reader: ReaderOptions,
    pub capture_fields: Vec<Vec<u8>>,
    /// Exact `(name, value)` pairs. Names and values are arbitrary bytes.
    pub capture_values: Vec<(Vec<u8>, Vec<u8>)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapturedValue {
    pub present: bool,
    pub entry_count: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct SnapshotMetadata {
    pub seqnum: u64,
    pub realtime: u64,
    pub monotonic: u64,
    pub boot_id: [u8; 16],
}

/// One callback-scoped entry. Payload bytes cannot outlive their visitor call.
pub struct SnapshotEntry<'a> {
    snapshot: &'a IndexedSnapshot,
    control: &'a SnapshotControl<'a>,
    metadata: SnapshotMetadata,
    offsets: Vec<NonZeroU64>,
}
impl SnapshotEntry<'_> {
    pub fn metadata(&self) -> SnapshotMetadata {
        self.metadata
    }
    pub fn visit_payloads(&mut self, mut visit: impl FnMut(&[u8]) -> Result<()>) -> Result<()> {
        let mut buffer = Vec::new();
        for &offset in &self.offsets {
            self.control.check()?;
            self.snapshot.check_object(offset, 1)?;
            let data = self.snapshot.file.data_ref(offset)?;
            if data.is_compressed() {
                buffer.clear();
                let len = data.decompress(&mut buffer)?;
                visit(&buffer[..len])?;
            } else {
                visit(data.raw_payload())?;
            }
        }
        Ok(())
    }
}

/// A single-consumer bounded view of a valid native journal index graph.
///
/// Open while appends, rotation and truncation are excluded by the caller.
/// After capture, supported append-only writers may resume. In-place mutation
/// and truncation are unsupported. Uncertain files require `verify_index` first.
/// This API never falls back to a row scan for an indexed operation. `.journal.zst`
/// uses whole-file staging and therefore has full-file decompression opening cost.
pub struct IndexedSnapshot {
    file: JournalFile<Mmap>,
    _staging: Option<SnapshotStaging>,
    header: JournalHeader,
    object_end: u64,
    entry_tail: u64,
    fields: HashMap<Vec<u8>, Option<NonZeroU64>>,
    values: HashMap<(Vec<u8>, Vec<u8>), CapturedValue>,
}
// Fields drop in declaration order: release maps/handles before deleting staging.
struct SnapshotStaging(PathBuf);
impl Drop for SnapshotStaging {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn corrupt(message: &str) -> SdkError {
    SdkError::VerificationError(message.into())
}
fn raw_name(name: &[u8]) -> Result<()> {
    if name.is_empty() || name.contains(&b'=') {
        Err(JournalError::InvalidField.into())
    } else {
        Ok(())
    }
}
fn number(offset: Option<NonZeroU64>) -> u64 {
    offset.map_or(0, NonZeroU64::get)
}

impl IndexedSnapshot {
    pub fn open(
        path: impl AsRef<Path>,
        mut options: IndexedSnapshotOptions,
        control: &SnapshotControl<'_>,
    ) -> Result<Self> {
        control.check()?;
        options.reader.bounds = ReaderBounds::Snapshot;
        let path = path.as_ref();
        let temp_path = if is_zst_file(path) {
            Some(decompress_zst_to_temp(path, "rust-indexed-snapshot")?)
        } else {
            None
        };
        let file = match open_journal_file(temp_path.as_deref().unwrap_or(path), options.reader) {
            Ok(file) => file,
            Err(err) => {
                if let Some(path) = temp_path {
                    let _ = std::fs::remove_file(path);
                }
                return Err(err);
            }
        };
        let header = *file.journal_header_ref();
        let mut snapshot = Self {
            file,
            _staging: temp_path.map(SnapshotStaging),
            header,
            object_end: 0,
            entry_tail: 0,
            fields: HashMap::new(),
            values: HashMap::new(),
        };
        control.check()?;
        snapshot.capture_object_bounds()?;
        snapshot.capture_entry_bounds(control)?;
        for field in options.capture_fields {
            raw_name(&field)?;
            let head = snapshot.find_field(&field, control)?;
            snapshot.fields.insert(field, head);
        }
        for (name, value) in options.capture_values {
            raw_name(&name)?;
            let captured = if let Some(offset) = snapshot.find_data(&name, &value, control, true)? {
                snapshot.capture_postings(offset, control)?
            } else {
                CapturedValue::default()
            };
            snapshot.values.insert((name, value), captured);
        }
        control.check()?;
        Ok(snapshot)
    }
    fn capture_object_bounds(&mut self) -> Result<()> {
        let header = &self.header;
        if header.tail_object_offset.is_none() != (header.n_objects == 0) {
            return Err(corrupt("object count and tail disagree"));
        }
        self.object_end = if let Some(tail) = header.tail_object_offset {
            if tail.get() < header.header_size || tail.get() % 8 != 0 {
                return Err(corrupt("invalid object tail"));
            }
            let object = self.file.object_header_ref(tail)?;
            tail.get()
                .checked_add(object.validated_size()?)
                .ok_or_else(|| corrupt("object end overflow"))?
        } else {
            header.header_size
        };
        if self.object_end > header.validated_arena_end(self.file.reader_file_size()?)? {
            return Err(corrupt("object tail exceeds declared arena"));
        }
        Ok(())
    }
    fn capture_entry_bounds(&mut self, control: &SnapshotControl<'_>) -> Result<()> {
        let header = &self.header;
        let (tail, last_array, last_count) =
            self.array_tail(header.entry_array_offset, header.n_entries, control)?;
        self.entry_tail = number(tail);
        if header.header_size >= 272 && header.tail_entry_offset != self.entry_tail {
            return Err(corrupt("tail entry hint disagrees with committed count"));
        }
        if header.tail_entry_array_hint_usable()
            && (header.tail_entry_array_offset as u64 != number(last_array)
                || header.tail_entry_array_n_entries as u64 != last_count)
        {
            return Err(corrupt("tail array hints disagree with committed count"));
        }
        if let Some(tail) = tail {
            self.check_object(tail, 3)?;
            let entry = self.file.entry_ref(tail)?;
            if entry.header.seqnum != header.tail_entry_seqnum
                || entry.header.realtime != header.tail_entry_realtime
                || (header.compatible_flags & 2 != 0
                    && (entry.header.monotonic != header.tail_entry_monotonic
                        || entry.header.boot_id != header.tail_entry_boot_id))
            {
                return Err(corrupt("tail metadata disagrees with committed entry"));
            }
        } else {
            header.validate_empty_entry_metadata()?;
        }
        Ok(())
    }
    fn capture_postings(
        &self,
        offset: NonZeroU64,
        control: &SnapshotControl<'_>,
    ) -> Result<CapturedValue> {
        let (first, array, count) = self.postings(offset)?;
        if count == 0 || count > self.header.n_entries {
            return Err(corrupt(
                "captured posting count exceeds committed population",
            ));
        }
        self.check_entry(first.ok_or_else(|| corrupt("missing inline posting"))?)?;
        let (last, last_array, used) = self.array_tail(array, count - 1, control)?;
        let data = self.file.data_ref(offset)?;
        if let Some((hint_offset, hint_count)) = data.tail_entry_array_hint() {
            if hint_offset as u64 != number(last_array) || hint_count as u64 != used {
                return Err(corrupt("DATA tail array hint disagrees with count"));
            }
        }
        drop(data);
        if let Some(last) = last {
            self.check_entry(last)?;
        }
        Ok(CapturedValue {
            present: true,
            entry_count: count,
        })
    }
    pub fn entry_count(&self) -> u64 {
        self.header.n_entries
    }
    /// Whether the captured header was archived. This remains fixed if the
    /// file is archived later; it does not certify index integrity.
    pub fn is_archived(&self) -> bool {
        self.header.state == JournalState::Archived as u8
    }
    pub fn captured_value(&self, name: &[u8], value: &[u8]) -> Result<CapturedValue> {
        self.values
            .get(&(name.to_vec(), value.to_vec()))
            .copied()
            .ok_or(SdkError::Unsupported("undeclared captured value"))
    }
    fn check_object(&self, offset: NonZeroU64, kind: u8) -> Result<()> {
        let n = offset.get();
        if n < self.header.header_size || n > number(self.header.tail_object_offset) || n % 8 != 0 {
            return Err(corrupt("object outside captured bounds"));
        }
        let object = self.file.object_header_ref(offset)?;
        if object.type_ != kind
            || n.checked_add(object.validated_size()?)
                .is_none_or(|end| end > self.object_end)
        {
            return Err(corrupt("invalid bounded object"));
        }
        Ok(())
    }
    fn check_entry(&self, offset: NonZeroU64) -> Result<()> {
        if offset.get() > self.entry_tail {
            return Err(corrupt("entry exceeds committed boundary"));
        }
        self.check_object(offset, 3)
    }
    // O(array nodes), not O(entries): only each node's terminal used slot is read.
    fn array_tail(
        &self,
        mut current: Option<NonZeroU64>,
        mut count: u64,
        control: &SnapshotControl<'_>,
    ) -> Result<(Option<NonZeroU64>, Option<NonZeroU64>, u64)> {
        if count == 0 {
            if current.is_some() {
                return Err(corrupt("array present for empty population"));
            }
            return Ok((None, None, 0));
        }
        let mut previous = 0;
        loop {
            control.check()?;
            let offset = current.ok_or_else(|| corrupt("array ended early"))?;
            if offset.get() <= previous {
                return Err(corrupt("array chain does not progress"));
            }
            self.check_object(offset, 6)?;
            let array = self.file.offset_array_ref(offset)?;
            let used = count.min(array.capacity() as u64);
            if used == 0 {
                return Err(corrupt("empty array"));
            }
            let last = array
                .items
                .get(used as usize - 1)
                .ok_or_else(|| corrupt("zero used array slot"))?;
            let next = array.header.next_offset_array;
            drop(array);
            self.check_object(last, 3)?;
            count -= used;
            if count == 0 {
                return Ok((Some(last), Some(offset), used));
            }
            previous = offset.get();
            current = next;
        }
    }
    fn find_field(&self, name: &[u8], control: &SnapshotControl<'_>) -> Result<Option<NonZeroU64>> {
        let hash = self.file.hash(name);
        let table = self
            .file
            .field_hash_table_ref()
            .ok_or(JournalError::MissingHashTable)?;
        if table.is_empty() {
            return Err(JournalError::MissingHashTable.into());
        }
        let mut current = table.hash_item_ref(hash).head_hash_offset;
        let mut previous = 0;
        while let Some(offset) = current {
            control.check()?;
            if offset.get() > number(self.header.tail_object_offset) {
                return Err(corrupt("FIELD capture exceeds object bounds"));
            }
            if offset.get() <= previous {
                return Err(corrupt("FIELD hash chain does not progress"));
            }
            self.check_object(offset, 2)?;
            let field = self.file.field_ref(offset)?;
            if field.header.next_hash_offset.is_some_and(|next| {
                next.get() <= offset.get() || next.get() > number(self.header.tail_object_offset)
            }) {
                return Err(corrupt("invalid captured FIELD hash link"));
            }
            if field.header.hash == hash && field.payload == name {
                let head = field.header.head_data_offset;
                if head.is_none() {
                    return Err(corrupt("FIELD has no DATA chain"));
                }
                drop(field);
                if let Some(head) = head {
                    self.check_object(head, 1)?;
                }
                return Ok(head);
            }
            previous = offset.get();
            current = field.header.next_hash_offset;
        }
        Ok(None)
    }
    fn find_data(
        &self,
        name: &[u8],
        value: &[u8],
        control: &SnapshotControl<'_>,
        capturing: bool,
    ) -> Result<Option<NonZeroU64>> {
        raw_name(name)?;
        let payload = journal_core::file::PayloadParts::structured(name, value);
        let hash = self.file.hash_parts(payload);
        let table = self
            .file
            .data_hash_table_ref()
            .ok_or(JournalError::MissingHashTable)?;
        if table.is_empty() {
            return Err(JournalError::MissingHashTable.into());
        }
        let mut current = table.hash_item_ref(hash).head_hash_offset;
        let mut previous = 0;
        let mut buffer = Vec::new();
        while let Some(offset) = current {
            control.check()?;
            if offset.get() > number(self.header.tail_object_offset) {
                if capturing {
                    return Err(corrupt("DATA capture exceeds object bounds"));
                }
                break;
            }
            if offset.get() <= previous {
                return Err(corrupt("DATA hash chain does not progress"));
            }
            self.check_object(offset, 1)?;
            let data = self.file.data_ref(offset)?;
            if capturing
                && data.header.next_hash_offset.is_some_and(|next| {
                    next.get() <= offset.get()
                        || next.get() > number(self.header.tail_object_offset)
                })
            {
                return Err(corrupt("invalid captured DATA hash link"));
            }
            if data.header.hash == hash {
                let bytes = if data.is_compressed() {
                    buffer.clear();
                    let len = data.decompress(&mut buffer)?;
                    &buffer[..len]
                } else {
                    data.raw_payload()
                };
                if payload.equals_slice(bytes) {
                    return Ok(Some(offset));
                }
            }
            previous = offset.get();
            current = data.header.next_hash_offset;
        }
        Ok(None)
    }
    fn postings(
        &self,
        offset: NonZeroU64,
    ) -> Result<(Option<NonZeroU64>, Option<NonZeroU64>, u64)> {
        self.check_object(offset, 1)?;
        let data = self.file.data_ref(offset)?;
        let count = number(data.header.n_entries);
        Ok((
            data.header.entry_offset,
            data.header.entry_array_offset,
            count,
        ))
    }
    fn emit(
        &self,
        offset: NonZeroU64,
        control: &SnapshotControl<'_>,
        visit: &mut impl FnMut(&mut SnapshotEntry<'_>) -> Result<()>,
    ) -> Result<()> {
        control.check()?;
        self.check_entry(offset)?;
        let (metadata, offsets) = {
            let entry = self.file.entry_ref(offset)?;
            let mut offsets = Vec::with_capacity(entry.items.len());
            entry.collect_offsets(&mut offsets)?;
            (
                SnapshotMetadata {
                    seqnum: entry.header.seqnum,
                    realtime: entry.header.realtime,
                    monotonic: entry.header.monotonic,
                    boot_id: entry.header.boot_id,
                },
                offsets,
            )
        };
        let mut entry = SnapshotEntry {
            snapshot: self,
            control,
            metadata,
            offsets,
        };
        visit(&mut entry)
    }
    // A live count can be newer than a copied pointer or slot. Writers publish
    // links before counts: refresh required zeros before treating them as damage.
    fn required_posting(
        &self,
        cached: Option<NonZeroU64>,
        position: u64,
        compact: bool,
    ) -> Result<NonZeroU64> {
        if let Some(offset) = cached {
            return Ok(offset);
        }
        let mut bytes = [0; 8];
        let size = if compact { 4 } else { 8 };
        self.file
            .read_fresh_bytes_at(position, &mut bytes[..size])?;
        NonZeroU64::new(u64::from_le_bytes(bytes))
            .ok_or_else(|| corrupt("missing required posting"))
    }
    fn posting_array_offset(
        &self,
        current: Option<NonZeroU64>,
        previous_array: u64,
        clip: bool,
    ) -> Result<NonZeroU64> {
        match current {
            Some(offset) => Ok(offset),
            None if clip && previous_array != 0 => {
                self.required_posting(None, previous_array + 16, false)
            }
            None => Err(corrupt("missing posting array")),
        }
    }
    fn posting_slot(
        &self,
        cached: Option<NonZeroU64>,
        array: NonZeroU64,
        index: usize,
        clip: bool,
    ) -> Result<NonZeroU64> {
        match cached {
            Some(entry) => Ok(entry),
            None if clip => {
                let compact = self.header.incompatible_flags & 16 != 0;
                let width = if compact { 4 } else { 8 };
                self.required_posting(None, array.get() + 24 + index as u64 * width, compact)
            }
            None => Err(corrupt("zero posting")),
        }
    }
    fn visit_array(
        &self,
        mut current: Option<NonZeroU64>,
        mut remaining: u64,
        mut last: u64,
        clip: bool,
        control: &SnapshotControl<'_>,
        visit: &mut impl FnMut(&mut SnapshotEntry<'_>) -> Result<()>,
    ) -> Result<()> {
        let mut previous_array = 0;
        let mut chunk = Vec::with_capacity(256);
        while remaining > 0 {
            control.check()?;
            let offset = self.posting_array_offset(current, previous_array, clip)?;
            if clip && offset.get() > number(self.header.tail_object_offset) {
                return Ok(());
            }
            if offset.get() <= previous_array {
                return Err(corrupt("array chain does not progress"));
            }
            self.check_object(offset, 6)?;
            let (capacity, next) = {
                let array = self.file.offset_array_ref(offset)?;
                (array.capacity(), array.header.next_offset_array)
            };
            if capacity == 0 {
                return Err(corrupt("empty posting array"));
            }
            let used = remaining.min(capacity as u64) as usize;
            for start in (0..used).step_by(256) {
                control.check()?;
                chunk.clear();
                {
                    let array = self.file.offset_array_ref(offset)?;
                    for index in start..used.min(start + 256) {
                        chunk.push(array.items.get(index));
                    }
                }
                for (index, &cached) in chunk.iter().enumerate() {
                    let entry = self.posting_slot(cached, offset, start + index, clip)?;
                    if entry.get() <= last {
                        return Err(corrupt("postings do not progress"));
                    }
                    if clip && entry.get() > self.entry_tail {
                        return Ok(());
                    }
                    self.emit(entry, control, visit)?;
                    last = entry.get();
                }
            }
            remaining -= used as u64;
            previous_array = offset.get();
            current = next;
        }
        Ok(())
    }
    fn visit_data(
        &self,
        offset: NonZeroU64,
        control: &SnapshotControl<'_>,
        visit: &mut impl FnMut(&mut SnapshotEntry<'_>) -> Result<()>,
    ) -> Result<()> {
        let (first, array, count) = self.postings(offset)?;
        if count == 0 {
            return Err(corrupt("DATA has no postings"));
        }
        let first = first.ok_or_else(|| corrupt("missing inline posting"))?;
        if first.get() > self.entry_tail {
            return Ok(());
        }
        self.emit(first, control, visit)?;
        let array = if count > 1 {
            Some(self.required_posting(array, offset.get() + 48, false)?)
        } else {
            array
        };
        self.visit_array(array, count - 1, first.get(), true, control, visit)
    }
    pub fn visit_match(
        &mut self,
        name: &[u8],
        value: &[u8],
        control: &SnapshotControl<'_>,
        mut visit: impl FnMut(&mut SnapshotEntry<'_>) -> Result<()>,
    ) -> Result<()> {
        control.check()?;
        if let Some(offset) = self.find_data(name, value, control, false)? {
            self.visit_data(offset, control, &mut visit)?;
        }
        Ok(())
    }
    /// Streams postings for every accepted value; multivalued rows may repeat.
    /// No union, deduplication, or sorting is performed.
    pub fn visit_field(
        &mut self,
        name: &[u8],
        control: &SnapshotControl<'_>,
        mut accept: impl FnMut(&[u8]) -> Result<bool>,
        mut visit: impl FnMut(&mut SnapshotEntry<'_>) -> Result<()>,
    ) -> Result<()> {
        control.check()?;
        let mut current = *self
            .fields
            .get(name)
            .ok_or(SdkError::Unsupported("undeclared captured field"))?;
        let mut previous = u64::MAX;
        let mut buffer = Vec::new();
        while let Some(offset) = current {
            control.check()?;
            if offset.get() >= previous {
                return Err(corrupt("FIELD DATA chain does not progress"));
            }
            self.check_object(offset, 1)?;
            let (next, accepted) = {
                let data = self.file.data_ref(offset)?;
                let bytes = if data.is_compressed() {
                    buffer.clear();
                    let len = data.decompress(&mut buffer)?;
                    &buffer[..len]
                } else {
                    data.raw_payload()
                };
                let value = bytes
                    .strip_prefix(name)
                    .and_then(|rest| rest.strip_prefix(b"="))
                    .ok_or_else(|| corrupt("FIELD chain payload mismatch"))?;
                (data.header.next_field_offset, accept(value)?)
            };
            if accepted {
                self.visit_data(offset, control, &mut visit)?;
            }
            previous = offset.get();
            current = next;
        }
        Ok(())
    }
    /// Lazily streams all captured entries, with O(entries) work and bounded chunks.
    pub fn visit_entries(
        &mut self,
        control: &SnapshotControl<'_>,
        mut visit: impl FnMut(&mut SnapshotEntry<'_>) -> Result<()>,
    ) -> Result<()> {
        control.check()?;
        self.visit_array(
            self.header.entry_array_offset,
            self.header.n_entries,
            0,
            false,
            control,
            &mut visit,
        )
    }
}
