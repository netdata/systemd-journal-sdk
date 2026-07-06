use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

const DIRECTORY_UNIQUE_CACHE_CAPACITY: usize = 8;

pub struct DirectoryReader {
    files: Vec<FileReader>,
    index: usize,
    pending_realtime_seek: Option<u64>,
    realtime_seek_bound: Option<(u64, Direction)>,
    candidates: Vec<Option<DirectoryCandidate>>,
    current_key: Option<DirectoryEntryKey>,
    direction: Option<Direction>,
    boot_newest: HashMap<[u8; 16], DirectoryBootNewest>,
    unique_cache: HashMap<DirectoryUniqueCacheKey, DirectoryUniqueCacheEntry>,
    unique_cache_order: VecDeque<DirectoryUniqueCacheKey>,
    unique_state: Option<DirectoryUniqueState>,
    #[cfg(test)]
    unique_cache_builds: usize,
    pub(super) non_overlapping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DirectoryUniqueCacheKey {
    field_name: String,
    files: Vec<DirectoryUniqueFileSignature>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DirectoryUniqueFileSignature {
    file_id: [u8; 16],
    n_objects: u64,
    n_entries: u64,
    n_data: u64,
    n_fields: u64,
    head_entry_seqnum: u64,
    tail_entry_seqnum: u64,
    head_entry_realtime: u64,
    tail_entry_realtime: u64,
    tail_entry_monotonic: u64,
    tail_entry_boot_id: [u8; 16],
}

#[derive(Debug, Clone)]
struct DirectoryUniqueCacheEntry {
    payloads: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct DirectoryUniqueState {
    key: DirectoryUniqueCacheKey,
    index: usize,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryCandidate {
    reader_index: usize,
    key: DirectoryEntryKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DirectoryEntryKey {
    pub(super) seqnum_id: [u8; 16],
    pub(super) seqnum: u64,
    pub(super) boot_id: [u8; 16],
    pub(super) monotonic: u64,
    pub(super) realtime: u64,
    pub(super) xor_hash: u64,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryBootNewest {
    machine_id: [u8; 16],
    monotonic: u64,
    realtime: u64,
}

impl DirectoryReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_options(path, ReaderOptions::default())
    }

    pub fn open_with_options(path: impl AsRef<Path>, options: ReaderOptions) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(SdkError::InvalidPath(format!(
                "not a directory: {}",
                path.display()
            )));
        }

        let mut files = Vec::new();
        for file_path in collect_journal_files(path)? {
            if let Ok(reader) = FileReader::open_with_options(&file_path, options) {
                files.push(reader);
            }
        }

        Self::from_readers(files, true)
    }

    pub fn open_files<I, P>(paths: I) -> Result<Self>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        Self::open_files_with_options(paths, ReaderOptions::default())
    }

    pub fn open_files_with_options<I, P>(paths: I, options: ReaderOptions) -> Result<Self>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut files = Vec::new();
        for path in paths {
            let path = path.as_ref();
            if !path.is_file() || !is_journal_file_name(path) {
                return Err(SdkError::InvalidPath(format!(
                    "not a journal file: {}",
                    path.display()
                )));
            }
            files.push(FileReader::open_with_options(path, options)?);
        }

        Self::from_readers(files, false)
    }

    fn from_readers(mut files: Vec<FileReader>, allow_empty: bool) -> Result<Self> {
        if files.is_empty() && !allow_empty {
            return Err(SdkError::InvalidPath(
                "no readable journal files".to_string(),
            ));
        }

        files.sort_by_key(FileReader::header_realtime_start);
        let boot_newest = build_directory_boot_newest(&files);
        let non_overlapping = directory_files_non_overlapping(&files);
        let candidates = vec![None; files.len()];
        Ok(Self {
            files,
            index: usize::MAX,
            pending_realtime_seek: None,
            realtime_seek_bound: None,
            candidates,
            current_key: None,
            direction: None,
            boot_newest,
            unique_cache: HashMap::new(),
            unique_cache_order: VecDeque::new(),
            unique_state: None,
            #[cfg(test)]
            unique_cache_builds: 0,
            non_overlapping,
        })
    }

    pub fn seek_head(&mut self) {
        self.pending_realtime_seek = None;
        self.realtime_seek_bound = None;
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.reset_candidates();
        for reader in &mut self.files {
            reader.seek_head();
        }
    }

    pub fn seek_tail(&mut self) {
        self.pending_realtime_seek = None;
        self.realtime_seek_bound = None;
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.reset_candidates();
        for reader in &mut self.files {
            reader.seek_tail();
        }
    }

    pub fn seek_realtime(&mut self, usec: u64) {
        self.pending_realtime_seek = Some(usec);
        self.realtime_seek_bound = None;
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.reset_candidates();
    }

    pub fn next(&mut self) -> Result<bool> {
        self.step_merged(Direction::Forward)
    }

    pub fn previous(&mut self) -> Result<bool> {
        self.step_merged(Direction::Backward)
    }

    fn step_merged(&mut self, direction: Direction) -> Result<bool> {
        if self.can_step_sequential(direction) {
            return self.step_sequential(direction);
        }

        self.prepare_merge_direction(direction);

        let mut best: Option<DirectoryCandidate> = None;
        for idx in 0..self.files.len() {
            self.fill_candidate(idx, direction)?;
            let Some(candidate) = self.candidates[idx] else {
                continue;
            };
            let replace = match best {
                None => true,
                Some(current) => {
                    let cmp = self.compare_entry_keys(candidate.key, current.key);
                    (direction == Direction::Forward && cmp < 0)
                        || (direction == Direction::Backward && cmp > 0)
                }
            };
            if replace {
                best = Some(candidate);
            }
        }

        let Some(best) = best else {
            self.index = usize::MAX;
            self.realtime_seek_bound = None;
            return Ok(false);
        };

        self.index = best.reader_index;
        self.current_key = Some(best.key);
        self.candidates[best.reader_index] = None;
        self.realtime_seek_bound = None;
        Ok(true)
    }

    fn prepare_merge_direction(&mut self, direction: Direction) {
        if let Some(usec) = self.pending_realtime_seek.take() {
            for reader in &mut self.files {
                reader.seek_realtime(usec);
            }
            self.reset_candidates();
            self.realtime_seek_bound = Some((usec, direction));
            self.direction = Some(direction);
            return;
        }

        if self.direction == Some(direction) {
            return;
        }

        if let Some(current) = self.current_key {
            for reader in &mut self.files {
                reader.seek_realtime(current.realtime);
            }
        } else if direction == Direction::Forward {
            for reader in &mut self.files {
                reader.seek_head();
            }
        } else {
            for reader in &mut self.files {
                reader.seek_tail();
            }
        }

        self.reset_candidates();
        self.direction = Some(direction);
    }

    fn fill_candidate(&mut self, reader_index: usize, direction: Direction) -> Result<()> {
        if self.candidates[reader_index].is_some() {
            return Ok(());
        }

        loop {
            if !self.advance_candidate_reader(reader_index, direction)? {
                return Ok(());
            }
            let key = self.files[reader_index].current_directory_entry_key()?;
            if !self.candidate_matches_realtime_bound(key) {
                continue;
            }
            if !self.candidate_is_after_current(key, direction) {
                continue;
            }

            self.candidates[reader_index] = Some(DirectoryCandidate { reader_index, key });
            return Ok(());
        }
    }

    fn advance_candidate_reader(
        &mut self,
        reader_index: usize,
        direction: Direction,
    ) -> Result<bool> {
        match direction {
            Direction::Forward => self.files[reader_index].next(),
            Direction::Backward => self.files[reader_index].previous(),
        }
    }

    fn candidate_matches_realtime_bound(&self, key: DirectoryEntryKey) -> bool {
        let Some((usec, seek_direction)) = self.realtime_seek_bound else {
            return true;
        };
        match seek_direction {
            Direction::Forward => key.realtime >= usec,
            Direction::Backward => key.realtime <= usec,
        }
    }

    fn candidate_is_after_current(&self, key: DirectoryEntryKey, direction: Direction) -> bool {
        let Some(current) = self.current_key else {
            return true;
        };
        let cmp = self.compare_entry_keys(key, current);
        match direction {
            Direction::Forward => cmp > 0,
            Direction::Backward => cmp < 0,
        }
    }

    fn compare_entry_keys(&self, a: DirectoryEntryKey, b: DirectoryEntryKey) -> i8 {
        if a == b {
            return 0;
        }

        if a.seqnum_id == b.seqnum_id {
            let cmp = cmp_u64(a.seqnum, b.seqnum);
            if cmp != 0 {
                return cmp;
            }
        }

        if a.boot_id == b.boot_id {
            let cmp = cmp_u64(a.monotonic, b.monotonic);
            if cmp != 0 {
                return cmp;
            }
        } else {
            let cmp = self.compare_boot_ids(a.boot_id, b.boot_id);
            if cmp != 0 {
                return cmp;
            }
        }

        let cmp = cmp_u64(a.realtime, b.realtime);
        if cmp != 0 {
            return cmp;
        }
        cmp_u64(a.xor_hash, b.xor_hash)
    }

    fn compare_boot_ids(&self, a: [u8; 16], b: [u8; 16]) -> i8 {
        let Some(a_newest) = self.boot_newest.get(&a) else {
            return 0;
        };
        let Some(b_newest) = self.boot_newest.get(&b) else {
            return 0;
        };
        if a_newest.machine_id != b_newest.machine_id {
            return 0;
        }
        cmp_u64(a_newest.realtime, b_newest.realtime)
    }

    fn reset_candidates(&mut self) {
        if self.candidates.len() != self.files.len() {
            self.candidates = vec![None; self.files.len()];
            return;
        }
        for candidate in &mut self.candidates {
            *candidate = None;
        }
    }

    pub fn get_entry(&mut self) -> Result<Entry> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_entry()
    }

    pub fn visit_entry_payloads<F>(&mut self, visitor: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].visit_entry_payloads(visitor)
    }

    pub fn clear_entry_data_state(&mut self) {
        if self.index < self.files.len() {
            self.files[self.index].clear_entry_data_state();
        }
    }

    pub fn entry_data_restart(&mut self) -> Result<()> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].entry_data_restart()
    }

    pub fn enumerate_entry_payload(&mut self) -> Result<Option<&[u8]>> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].enumerate_entry_payload()
    }

    pub fn collect_entry_payloads(&mut self, payloads: &mut Vec<Vec<u8>>) -> Result<()> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].collect_entry_payloads(payloads)
    }

    pub fn get_entry_payload(&mut self, field: &[u8]) -> Result<Option<Vec<u8>>> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_entry_payload(field)
    }

    pub fn get_realtime_usec(&self) -> Result<u64> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_realtime_usec()
    }

    pub fn get_seqnum(&self) -> Result<(u64, [u8; 16])> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        if let Some(key) = self.current_key {
            return Ok((key.seqnum, key.seqnum_id));
        }
        self.files[self.index].get_seqnum()
    }

    pub fn get_monotonic_usec(&self) -> Result<(u64, [u8; 16])> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        if let Some(key) = self.current_key {
            return Ok((key.monotonic, key.boot_id));
        }
        self.files[self.index].get_monotonic_usec()
    }

    pub fn get_cursor(&self) -> Result<String> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_cursor()
    }

    pub fn test_cursor(&self, cursor: &str) -> Result<bool> {
        if self.index >= self.files.len() {
            return Ok(false);
        }
        self.files[self.index].test_cursor(cursor)
    }

    pub fn seek_cursor(&mut self, cursor: &str) -> Result<()> {
        let want = parse::parse_cursor_location(cursor, true)
            .map_err(|err| SdkError::InvalidCursor(err.to_string()))?;
        if want.realtime_set {
            self.seek_realtime(want.realtime);
        } else {
            self.seek_head();
        }
        while self.next()? {
            let current_cursor = self.get_cursor()?;
            let got = parse::parse_cursor_location(&current_cursor, false)
                .map_err(|err| SdkError::InvalidCursor(err.to_string()))?;
            if parse::cursor_location_at_or_after(&got, &want) {
                return Ok(());
            }
        }
        self.seek_tail();
        Ok(())
    }

    pub fn enumerate_fields(&mut self) -> Result<Vec<String>> {
        let mut fields = HashSet::new();
        for reader in &mut self.files {
            for field in reader.enumerate_fields()? {
                fields.insert(field);
            }
        }
        let mut out: Vec<_> = fields.into_iter().collect();
        out.sort();
        Ok(out)
    }

    pub fn query_unique(&mut self, field_name: &str) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        self.visit_unique_values(field_name, |value| {
            out.push(value.to_vec());
            Ok(())
        })?;
        Ok(out)
    }

    pub fn visit_unique_values<F>(&mut self, field_name: &str, mut visitor: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        let key = self.ensure_unique_cache(field_name)?;
        let Some(entry) = self.unique_cache.get(&key) else {
            return Err(SdkError::VerificationError(
                "directory unique cache entry disappeared".to_string(),
            ));
        };
        for payload in &entry.payloads {
            let value = strip_cached_unique_payload(field_name, payload)?;
            visitor(value)?;
        }
        Ok(())
    }

    pub fn query_unique_state(&mut self, field_name: &str) -> Result<()> {
        self.clear_unique_state();
        let key = self.ensure_unique_cache(field_name)?;
        self.unique_state = Some(DirectoryUniqueState { key, index: 0 });
        Ok(())
    }

    pub fn restart_unique_state(&mut self) {
        if let Some(state) = &mut self.unique_state {
            state.index = 0;
        }
    }

    pub fn clear_unique_state(&mut self) {
        self.unique_state = None;
        for reader in &mut self.files {
            reader.clear_unique_state();
        }
    }

    pub fn enumerate_unique_payload(&mut self) -> Result<Option<Vec<u8>>> {
        let Some(state) = &mut self.unique_state else {
            return Ok(None);
        };
        let Some(entry) = self.unique_cache.get(&state.key) else {
            return Err(SdkError::VerificationError(
                "directory unique state references a missing cache entry".to_string(),
            ));
        };
        if state.index >= entry.payloads.len() {
            return Ok(None);
        }
        let payload = entry.payloads[state.index].clone();
        state.index += 1;
        Ok(Some(payload))
    }

    fn ensure_unique_cache(&mut self, field_name: &str) -> Result<DirectoryUniqueCacheKey> {
        let key = self.unique_cache_key(field_name);
        if self.unique_cache.contains_key(&key) {
            self.touch_unique_cache_key(&key);
            return Ok(key);
        }

        let mut payloads = self.build_unique_cache_payloads(field_name)?;
        let refreshed_key = self.unique_cache_key(field_name);
        if refreshed_key != key {
            payloads = self.build_unique_cache_payloads(field_name)?;
        }
        let final_key = if refreshed_key == key {
            key
        } else {
            refreshed_key
        };
        self.unique_cache
            .insert(final_key.clone(), DirectoryUniqueCacheEntry { payloads });
        self.touch_unique_cache_key(&final_key);
        self.enforce_unique_cache_capacity();
        #[cfg(test)]
        {
            self.unique_cache_builds += 1;
        }
        Ok(final_key)
    }

    fn touch_unique_cache_key(&mut self, key: &DirectoryUniqueCacheKey) {
        if let Some(index) = self
            .unique_cache_order
            .iter()
            .position(|existing| existing == key)
        {
            self.unique_cache_order.remove(index);
        }
        self.unique_cache_order.push_back(key.clone());
    }

    fn enforce_unique_cache_capacity(&mut self) {
        let active_key = self.unique_state.as_ref().map(|state| state.key.clone());
        let mut skipped_active = false;
        while self.unique_cache_order.len() > DIRECTORY_UNIQUE_CACHE_CAPACITY {
            let Some(evicted) = self.unique_cache_order.pop_front() else {
                break;
            };
            if active_key.as_ref() == Some(&evicted) {
                if skipped_active {
                    self.unique_cache_order.push_front(evicted);
                    break;
                }
                self.unique_cache_order.push_back(evicted);
                skipped_active = true;
                continue;
            }
            self.unique_cache.remove(&evicted);
        }
    }

    fn build_unique_cache_payloads(&mut self, field_name: &str) -> Result<Vec<Vec<u8>>> {
        let mut seen = HashSet::new();
        let mut payloads = Vec::new();
        for reader in &mut self.files {
            reader.visit_unique_values(field_name, |value| {
                if seen.insert(value.to_vec()) {
                    payloads.push(cached_unique_payload(field_name, value));
                }
                Ok(())
            })?;
        }
        Ok(payloads)
    }

    fn unique_cache_key(&self, field_name: &str) -> DirectoryUniqueCacheKey {
        DirectoryUniqueCacheKey {
            field_name: field_name.to_string(),
            files: self
                .files
                .iter()
                .map(|reader| {
                    let header = reader.header();
                    DirectoryUniqueFileSignature {
                        file_id: header.file_id,
                        n_objects: header.n_objects,
                        n_entries: header.n_entries,
                        n_data: header.n_data,
                        n_fields: header.n_fields,
                        head_entry_seqnum: header.head_entry_seqnum,
                        tail_entry_seqnum: header.tail_entry_seqnum,
                        head_entry_realtime: header.head_entry_realtime,
                        tail_entry_realtime: header.tail_entry_realtime,
                        tail_entry_monotonic: header.tail_entry_monotonic,
                        tail_entry_boot_id: header.tail_entry_boot_id,
                    }
                })
                .collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn unique_cache_builds_for_tests(&self) -> usize {
        self.unique_cache_builds
    }

    pub fn list_boots(&self) -> Vec<BootInfo> {
        let mut boots: HashMap<String, (i64, i64)> = HashMap::new();
        for reader in &self.files {
            let header = reader.cached_header().header;
            let boot_id = hex::encode(header.tail_entry_boot_id);
            let first = header.head_entry_realtime as i64;
            let last = header.tail_entry_realtime as i64;
            boots
                .entry(boot_id)
                .and_modify(|range| {
                    range.0 = range.0.min(first);
                    range.1 = range.1.max(last);
                })
                .or_insert((first, last));
        }

        let mut out: Vec<_> = boots
            .into_iter()
            .map(|(boot_id, (first_entry, last_entry))| BootInfo {
                index: 0,
                boot_id,
                first_entry,
                last_entry,
            })
            .collect();
        out.sort_by_key(|boot| boot.first_entry);
        let base = 1 - out.len() as i64;
        for (idx, boot) in out.iter_mut().enumerate() {
            boot.index = base + idx as i64;
        }
        out
    }

    pub fn add_match(&mut self, data: &[u8]) {
        for reader in &mut self.files {
            reader.add_match(data);
        }
        self.reset_merge_state();
    }

    pub fn add_conjunction(&mut self) -> Result<()> {
        for reader in &mut self.files {
            reader.add_conjunction()?;
        }
        self.reset_merge_state();
        Ok(())
    }

    pub fn add_disjunction(&mut self) -> Result<()> {
        for reader in &mut self.files {
            reader.add_disjunction()?;
        }
        self.reset_merge_state();
        Ok(())
    }

    pub fn flush_matches(&mut self) {
        for reader in &mut self.files {
            reader.flush_matches();
        }
        self.reset_merge_state();
    }

    fn reset_merge_state(&mut self) {
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.realtime_seek_bound = None;
        self.reset_candidates();
    }

    fn can_step_sequential(&self, direction: Direction) -> bool {
        if !self.non_overlapping || self.pending_realtime_seek.is_some() {
            return false;
        }
        if self.direction.is_some_and(|current| current != direction) && self.current_key.is_some()
        {
            return false;
        }
        true
    }

    fn step_sequential(&mut self, direction: Direction) -> Result<bool> {
        if self.files.is_empty() {
            self.clear_current_directory_entry();
            return Ok(false);
        }

        if self.direction != Some(direction) {
            self.reset_sequential_direction(direction);
        }

        match direction {
            Direction::Forward => self.step_sequential_forward(),
            Direction::Backward => self.step_sequential_backward(),
        }
    }

    fn reset_sequential_direction(&mut self, direction: Direction) {
        match direction {
            Direction::Forward => {
                for reader in &mut self.files {
                    reader.seek_head();
                }
                self.index = 0;
            }
            Direction::Backward => {
                for reader in &mut self.files {
                    reader.seek_tail();
                }
                self.index = self.files.len() - 1;
            }
        }
        self.reset_candidates();
        self.current_key = None;
        self.realtime_seek_bound = None;
        self.direction = Some(direction);
    }

    fn step_sequential_forward(&mut self) -> Result<bool> {
        if self.index == usize::MAX {
            self.index = 0;
        }
        while self.index < self.files.len() {
            if self.files[self.index].next()? {
                self.current_key = Some(self.files[self.index].current_directory_entry_key()?);
                return Ok(true);
            }
            self.index += 1;
        }
        self.finish_sequential_end()
    }

    fn step_sequential_backward(&mut self) -> Result<bool> {
        if self.index >= self.files.len() {
            self.index = self.files.len() - 1;
        }
        loop {
            if self.files[self.index].previous()? {
                self.current_key = Some(self.files[self.index].current_directory_entry_key()?);
                return Ok(true);
            }
            if self.index == 0 {
                break;
            }
            self.index -= 1;
        }
        self.finish_sequential_end()
    }

    fn finish_sequential_end(&mut self) -> Result<bool> {
        self.clear_current_directory_entry();
        Ok(false)
    }

    fn clear_current_directory_entry(&mut self) {
        self.index = usize::MAX;
        self.current_key = None;
    }
}

pub(super) fn is_journal_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".journal")
                || name.ends_with(".journal~")
                || name.ends_with(".journal.zst")
                || name.ends_with(".journal~.zst")
        })
}

fn collect_journal_files(path: &Path) -> Result<Vec<PathBuf>> {
    let entries: Vec<_> = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    let mut files = Vec::new();

    for entry in &entries {
        let file_path = entry.path();
        if file_path.is_file() && is_journal_file_name(&file_path) {
            files.push(file_path);
        }
    }

    for entry in &entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_journal_subdir_name(&name) {
            continue;
        }
        let child_path = entry.path();
        if !child_path.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&child_path) else {
            continue;
        };
        for child in children.flatten() {
            let file_path = child.path();
            if file_path.is_file() && is_journal_file_name(&file_path) {
                files.push(file_path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn cached_unique_payload(field_name: &str, value: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(field_name.len() + 1 + value.len());
    payload.extend_from_slice(field_name.as_bytes());
    payload.push(b'=');
    payload.extend_from_slice(value);
    payload
}

fn strip_cached_unique_payload<'a>(field_name: &str, payload: &'a [u8]) -> Result<&'a [u8]> {
    payload
        .strip_prefix(field_name.as_bytes())
        .and_then(|rest| rest.strip_prefix(b"="))
        .ok_or_else(|| {
            SdkError::VerificationError(
                "directory unique cache payload does not match requested field".to_string(),
            )
        })
}

fn is_journal_subdir_name(name: &str) -> bool {
    if name.contains('.') {
        return false;
    }
    id128_string_valid(name)
}

fn id128_string_valid(s: &str) -> bool {
    match s.len() {
        32 => s.bytes().all(|byte| byte.is_ascii_hexdigit()),
        36 => s.bytes().enumerate().all(|(idx, byte)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        }),
        _ => false,
    }
}

fn build_directory_boot_newest(files: &[FileReader]) -> HashMap<[u8; 16], DirectoryBootNewest> {
    let mut newest: HashMap<[u8; 16], DirectoryBootNewest> = HashMap::new();
    for reader in files {
        let header = reader.cached_header();
        if header.header.tail_entry_boot_id == [0; 16] {
            continue;
        }
        let replace = match newest.get(&header.header.tail_entry_boot_id) {
            None => true,
            Some(current) => header.header.tail_entry_monotonic > current.monotonic,
        };
        if replace {
            newest.insert(
                header.header.tail_entry_boot_id,
                DirectoryBootNewest {
                    machine_id: header.header.machine_id,
                    monotonic: header.header.tail_entry_monotonic,
                    realtime: header.header.tail_entry_realtime,
                },
            );
        }
    }
    newest
}

fn directory_files_non_overlapping(files: &[FileReader]) -> bool {
    if files.is_empty() {
        return false;
    }

    for pair in files.windows(2) {
        let previous = pair[0].cached_header().header;
        let next = pair[1].cached_header().header;
        if previous.seqnum_id != next.seqnum_id
            || previous.tail_entry_seqnum == 0
            || next.head_entry_seqnum == 0
            || previous.tail_entry_seqnum >= next.head_entry_seqnum
            || previous.tail_entry_realtime == 0
            || next.head_entry_realtime == 0
            || previous.tail_entry_realtime >= next.head_entry_realtime
        {
            return false;
        }
    }

    true
}

fn cmp_u64(a: u64, b: u64) -> i8 {
    match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}
