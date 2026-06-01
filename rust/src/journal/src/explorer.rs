use super::{
    Direction, DirectoryEntryKey, DirectoryReader, FileReader, Result, SdkError,
    collect_entry_data_offsets, collect_offsets_from_entry_items, format_cursor_from_key,
    split_raw_payload,
};
use journal_core::file::{HashableObject, JournalFile, Mmap};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerFilterKind {
    In,
    NotIn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerFilter {
    pub field: Vec<u8>,
    pub values: Vec<Vec<u8>>,
    pub kind: ExplorerFilterKind,
}

impl ExplorerFilter {
    pub fn field_in(field: impl Into<Vec<u8>>, values: Vec<Vec<u8>>) -> Self {
        Self {
            field: field.into(),
            values,
            kind: ExplorerFilterKind::In,
        }
    }

    pub fn field_not_in(field: impl Into<Vec<u8>>, values: Vec<Vec<u8>>) -> Self {
        Self {
            field: field.into(),
            values,
            kind: ExplorerFilterKind::NotIn,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerDisplay {
    None,
    All,
    Fields(Vec<Vec<u8>>),
}

impl Default for ExplorerDisplay {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Clone)]
pub struct ExplorerQuery {
    pub filters: Vec<ExplorerFilter>,
    pub facets: Vec<Vec<u8>>,
    pub full_text: Option<Vec<u8>>,
    pub display: ExplorerDisplay,
    pub limit: Option<usize>,
    pub direction: Direction,
    pub since_realtime_usec: Option<u64>,
    pub until_realtime_usec: Option<u64>,
}

impl Default for ExplorerQuery {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            facets: Vec::new(),
            full_text: None,
            display: ExplorerDisplay::All,
            limit: Some(100),
            direction: Direction::Forward,
            since_realtime_usec: None,
            until_realtime_usec: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExplorerUniqueQuery {
    pub field: Vec<u8>,
    pub filters: Vec<ExplorerFilter>,
    pub limit: Option<usize>,
    pub skip: usize,
    pub include_counts: bool,
    pub since_realtime_usec: Option<u64>,
    pub until_realtime_usec: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExplorerQueryCounters {
    pub entry_offsets_indexed: u64,
    pub filter_data_objects_examined: u64,
    pub candidate_entries: u64,
    pub candidate_data_refs_visited: u64,
    pub data_refs_reported: u64,
    pub payloads_materialized: u64,
    pub payloads_decompressed: u64,
    pub facet_values_materialized: u64,
    pub fts_payloads_scanned: u64,
    pub display_rows_expanded: u64,
    pub constrained_facet_counts: u64,
    pub field_linkage_hits: u64,
    pub field_linkage_fallbacks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerRow {
    pub realtime: u64,
    pub seqnum: u64,
    pub cursor: String,
    pub fields: Vec<(Vec<u8>, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerFacetValue {
    pub value: Vec<u8>,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerFacet {
    pub field: Vec<u8>,
    pub values: Vec<ExplorerFacetValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerQueryResult {
    pub rows: Vec<ExplorerRow>,
    pub facets: Vec<ExplorerFacet>,
    pub total_candidates: u64,
    pub counters: ExplorerQueryCounters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerUniqueValue {
    pub value: Vec<u8>,
    pub count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerUniqueResult {
    pub field: Vec<u8>,
    pub values: Vec<ExplorerUniqueValue>,
    pub total_values_considered: u64,
    pub counters: ExplorerQueryCounters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryDataRef {
    pub offset: u64,
    pub compressed: bool,
    pub payload_len: u64,
}

impl FileReader {
    pub fn visit_entry_data_refs<F>(&mut self, mut visitor: F) -> Result<()>
    where
        F: FnMut(EntryDataRef) -> Result<()>,
    {
        self.invalidate_entry_data_state();
        let inner = &mut self.inner;
        let data_offsets = &mut self.data_offsets;
        let data_offsets_entry = &mut self.data_offsets_entry;
        inner.with_mut(|fields| {
            let entry_offset = fields.reader.get_entry_offset()?;
            if *data_offsets_entry != Some(entry_offset) {
                collect_entry_data_offsets(fields.file, entry_offset, data_offsets)?;
                *data_offsets_entry = Some(entry_offset);
            }

            for data_offset in data_offsets.iter().copied() {
                let data = fields.file.data_ref(data_offset)?;
                visitor(EntryDataRef {
                    offset: data_offset.get(),
                    compressed: data.is_compressed(),
                    payload_len: data.raw_payload().len() as u64,
                })?;
            }

            Ok(())
        })
    }

    pub fn field_data_offsets(&mut self, field: &[u8]) -> Result<Vec<u64>> {
        self.invalidate_entry_data_state();
        self.inner.with_file(|file| {
            let mut offsets = Vec::new();
            file.field_data_offsets(field, &mut offsets)?;
            Ok(offsets.into_iter().map(NonZeroU64::get).collect())
        })
    }

    pub fn explorer_query(&mut self, query: &ExplorerQuery) -> Result<ExplorerQueryResult> {
        self.invalidate_entry_data_state();
        self.inner
            .with_file(|file| execute_file_query(file, query, None))
    }

    pub fn explorer_unique(&mut self, query: &ExplorerUniqueQuery) -> Result<ExplorerUniqueResult> {
        self.invalidate_entry_data_state();
        self.inner
            .with_file(|file| execute_file_unique(file, query))
    }
}

impl DirectoryReader {
    pub fn explorer_query(&mut self, query: &ExplorerQuery) -> Result<ExplorerQueryResult> {
        let mut combined = ExplorerQueryResult {
            rows: Vec::new(),
            facets: Vec::new(),
            total_candidates: 0,
            counters: ExplorerQueryCounters::default(),
        };
        let mut facet_maps: HashMap<Vec<u8>, HashMap<Vec<u8>, u64>> = HashMap::new();

        for reader in &mut self.files {
            let result = reader.explorer_query(query)?;
            merge_counters(&mut combined.counters, &result.counters);
            combined.total_candidates = combined
                .total_candidates
                .saturating_add(result.total_candidates);
            combined.rows.extend(result.rows);
            for facet in result.facets {
                let values = facet_maps.entry(facet.field).or_default();
                for value in facet.values {
                    *values.entry(value.value).or_default() += value.count;
                }
            }
        }

        sort_rows(&mut combined.rows, query.direction);
        if let Some(limit) = query.limit {
            combined.rows.truncate(limit);
        }
        combined.facets = facet_maps_to_vec(facet_maps);
        Ok(combined)
    }

    pub fn explorer_unique(&mut self, query: &ExplorerUniqueQuery) -> Result<ExplorerUniqueResult> {
        let mut value_counts: HashMap<Vec<u8>, u64> = HashMap::new();
        let mut counters = ExplorerQueryCounters::default();
        let mut considered = 0u64;

        for reader in &mut self.files {
            let mut per_file_query = query.clone();
            per_file_query.limit = None;
            per_file_query.skip = 0;
            let result = reader.explorer_unique(&per_file_query)?;
            merge_counters(&mut counters, &result.counters);
            considered = considered.saturating_add(result.total_values_considered);
            for value in result.values {
                *value_counts.entry(value.value).or_default() += value.count.unwrap_or(0);
            }
        }

        let mut values: Vec<_> = value_counts
            .into_iter()
            .map(|(value, count)| ExplorerUniqueValue {
                value,
                count: query.include_counts.then_some(count),
            })
            .collect();
        values.sort_by(|a, b| a.value.cmp(&b.value));
        let start = query.skip.min(values.len());
        let end = query
            .limit
            .map(|limit| start.saturating_add(limit).min(values.len()))
            .unwrap_or(values.len());
        values = values[start..end].to_vec();

        Ok(ExplorerUniqueResult {
            field: query.field.clone(),
            values,
            total_values_considered: considered,
            counters,
        })
    }
}

fn execute_file_query(
    file: &JournalFile<Mmap>,
    query: &ExplorerQuery,
    file_label: Option<&[u8]>,
) -> Result<ExplorerQueryResult> {
    let mut counters = ExplorerQueryCounters::default();
    let all_entry_offsets = all_entry_offsets(file)?;
    counters.entry_offsets_indexed = all_entry_offsets.len() as u64;
    let candidate_set =
        build_candidate_set(file, &all_entry_offsets, &query.filters, &mut counters)?;
    let constrained_facets = constrained_positive_facets(query);
    let constrained_facets_complete = query.full_text.is_none()
        && !query.facets.is_empty()
        && constrained_facets.len() == query.facets.len()
        && constrained_facets_cover_candidate_values(file, &constrained_facets, &candidate_set)?;
    let no_scan_path =
        query.full_text.is_none() && (query.facets.is_empty() || constrained_facets_complete);

    let mut facet_maps = HashMap::new();
    if no_scan_path && !query.facets.is_empty() {
        facet_maps = constrained_facet_counts(
            file,
            query,
            &candidate_set,
            &all_entry_offsets,
            &mut counters,
        )?;
    }
    let exact_total_candidates = (query.full_text.is_none()
        && query.since_realtime_usec.is_none()
        && query.until_realtime_usec.is_none())
    .then(|| {
        candidate_set
            .as_ref()
            .map_or(all_entry_offsets.len() as u64, |candidate| {
                candidate.len() as u64
            })
    });

    let mut facet_data = if no_scan_path {
        FieldDataMap::default()
    } else {
        build_field_data_map(file, &query.facets, &mut counters)?
    };
    let display_data = match &query.display {
        ExplorerDisplay::Fields(fields) => build_field_data_map(file, fields, &mut counters)?,
        ExplorerDisplay::None | ExplorerDisplay::All => FieldDataMap::default(),
    };

    let mut rows = Vec::new();
    let mut total_candidates = exact_total_candidates.unwrap_or(0);
    let mut data_offsets = Vec::new();
    let mut decompressed = Vec::new();
    let ordered = ordered_offsets(&all_entry_offsets, query.direction);
    let needs_data_offsets = query.full_text.is_some()
        || (!no_scan_path && !query.facets.is_empty())
        || !matches!(query.display, ExplorerDisplay::None);

    for entry_offset in ordered {
        if no_scan_path
            && exact_total_candidates.is_some()
            && query.limit.is_some_and(|limit| rows.len() >= limit)
        {
            break;
        }
        if !candidate_contains(&candidate_set, entry_offset) {
            continue;
        }
        let (realtime, seqnum, monotonic, boot_id, xor_hash) = {
            let entry = file.entry_ref(entry_offset)?;
            if needs_data_offsets {
                collect_offsets_from_entry_items(&entry.items, &mut data_offsets);
            } else {
                data_offsets.clear();
            }
            (
                entry.header.realtime,
                entry.header.seqnum,
                entry.header.monotonic,
                entry.header.boot_id,
                entry.header.xor_hash,
            )
        };
        if !time_matches(
            query.since_realtime_usec,
            query.until_realtime_usec,
            realtime,
        ) {
            continue;
        }
        if exact_total_candidates.is_none() {
            total_candidates = total_candidates.saturating_add(1);
        }
        counters.candidate_entries = counters.candidate_entries.saturating_add(1);

        let mut fts_match = true;
        if let Some(needle) = &query.full_text {
            fts_match = entry_matches_full_text(
                file,
                &data_offsets,
                needle,
                &mut decompressed,
                &mut counters,
            )?;
        }
        if !fts_match {
            continue;
        }

        if !no_scan_path && !query.facets.is_empty() {
            counters.candidate_data_refs_visited = counters
                .candidate_data_refs_visited
                .saturating_add(data_offsets.len() as u64);
            aggregate_facets(
                file,
                &data_offsets,
                &mut facet_data,
                &mut facet_maps,
                &mut decompressed,
                &mut counters,
            )?;
        }

        if query.limit.map_or(true, |limit| rows.len() < limit) {
            let fields = materialize_display_fields(
                file,
                &data_offsets,
                &query.display,
                &display_data,
                &mut decompressed,
                &mut counters,
            )?;
            if !matches!(query.display, ExplorerDisplay::None) {
                counters.display_rows_expanded = counters.display_rows_expanded.saturating_add(1);
            }
            rows.push(ExplorerRow {
                realtime,
                seqnum,
                cursor: format_cursor_from_key(DirectoryEntryKey {
                    seqnum_id: file.journal_header_ref().seqnum_id,
                    seqnum,
                    boot_id,
                    monotonic,
                    realtime,
                    xor_hash,
                }),
                fields: with_file_label(fields, file_label),
            });
        }
    }

    Ok(ExplorerQueryResult {
        rows,
        facets: facet_maps_to_vec(facet_maps),
        total_candidates,
        counters,
    })
}

fn execute_file_unique(
    file: &JournalFile<Mmap>,
    query: &ExplorerUniqueQuery,
) -> Result<ExplorerUniqueResult> {
    let mut counters = ExplorerQueryCounters::default();
    let all_entry_offsets = all_entry_offsets(file)?;
    counters.entry_offsets_indexed = all_entry_offsets.len() as u64;
    let candidate_set =
        build_candidate_set(file, &all_entry_offsets, &query.filters, &mut counters)?;
    let mut target_offsets = Vec::new();
    file.field_data_offsets(&query.field, &mut target_offsets)?;
    counters.field_linkage_hits = counters
        .field_linkage_hits
        .saturating_add(target_offsets.len() as u64);

    let mut values = Vec::new();
    let mut decompressed = Vec::new();
    let mut postings = Vec::new();
    let mut considered = 0u64;

    for data_offset in target_offsets {
        considered = considered.saturating_add(1);
        postings.clear();
        collect_data_entry_offsets(file, data_offset, &mut postings)?;
        let mut count = 0u64;
        for entry_offset in &postings {
            if !candidate_contains(&candidate_set, *entry_offset) {
                continue;
            }
            let entry = file.entry_ref(*entry_offset)?;
            if time_matches(
                query.since_realtime_usec,
                query.until_realtime_usec,
                entry.header.realtime,
            ) {
                count = count.saturating_add(1);
            }
        }
        if count == 0 {
            continue;
        }
        let value = materialize_known_field_value(
            file,
            data_offset,
            &query.field,
            &mut decompressed,
            &mut counters,
        )?;
        values.push(ExplorerUniqueValue {
            value,
            count: query.include_counts.then_some(count),
        });
    }

    values.sort_by(|a, b| a.value.cmp(&b.value));
    let start = query.skip.min(values.len());
    let end = query
        .limit
        .map(|limit| start.saturating_add(limit).min(values.len()))
        .unwrap_or(values.len());
    values = values[start..end].to_vec();
    Ok(ExplorerUniqueResult {
        field: query.field.clone(),
        values,
        total_values_considered: considered,
        counters,
    })
}

fn all_entry_offsets(file: &JournalFile<Mmap>) -> Result<Vec<NonZeroU64>> {
    let mut offsets = Vec::new();
    file.entry_offsets(&mut offsets)?;
    Ok(offsets)
}

fn build_candidate_set(
    file: &JournalFile<Mmap>,
    all_entry_offsets: &[NonZeroU64],
    filters: &[ExplorerFilter],
    counters: &mut ExplorerQueryCounters,
) -> Result<Option<HashSet<NonZeroU64>>> {
    let mut candidate: Option<HashSet<NonZeroU64>> = None;

    for filter in filters {
        match filter.kind {
            ExplorerFilterKind::In => {
                let value_offsets = filter_value_entry_offsets(file, filter, counters)?;
                candidate = Some(match candidate.take() {
                    Some(existing) => existing.intersection(&value_offsets).copied().collect(),
                    None => value_offsets,
                });
            }
            ExplorerFilterKind::NotIn => {
                let excluded = filter_value_entry_offsets(file, filter, counters)?;
                let mut base = candidate.take().unwrap_or_else(|| {
                    all_entry_offsets
                        .iter()
                        .copied()
                        .collect::<HashSet<NonZeroU64>>()
                });
                for offset in excluded {
                    base.remove(&offset);
                }
                candidate = Some(base);
            }
        }
    }

    Ok(candidate)
}

fn filter_value_entry_offsets(
    file: &JournalFile<Mmap>,
    filter: &ExplorerFilter,
    counters: &mut ExplorerQueryCounters,
) -> Result<HashSet<NonZeroU64>> {
    let mut out = HashSet::new();
    let mut postings = Vec::new();
    for value in &filter.values {
        let payload = payload_for(&filter.field, value);
        let hash = file.hash(&payload);
        let Some(data_offset) = file.find_data_offset(hash, &payload)? else {
            continue;
        };
        counters.filter_data_objects_examined =
            counters.filter_data_objects_examined.saturating_add(1);
        postings.clear();
        collect_data_entry_offsets(file, data_offset, &mut postings)?;
        out.extend(postings.iter().copied());
    }
    Ok(out)
}

fn collect_data_entry_offsets(
    file: &JournalFile<Mmap>,
    data_offset: NonZeroU64,
    offsets: &mut Vec<NonZeroU64>,
) -> Result<()> {
    offsets.clear();
    let cursor = {
        let data = file.data_ref(data_offset)?;
        data.inlined_cursor()
    };
    if let Some(cursor) = cursor {
        cursor.collect_offsets(file, offsets)?;
    }
    Ok(())
}

fn candidate_contains(candidate: &Option<HashSet<NonZeroU64>>, offset: NonZeroU64) -> bool {
    candidate
        .as_ref()
        .map_or(true, |candidate| candidate.contains(&offset))
}

fn ordered_offsets(offsets: &[NonZeroU64], direction: Direction) -> Vec<NonZeroU64> {
    let mut out = offsets.to_vec();
    if direction == Direction::Backward {
        out.reverse();
    }
    out
}

fn time_matches(since: Option<u64>, until: Option<u64>, realtime: u64) -> bool {
    if since.is_some_and(|since| realtime < since) {
        return false;
    }
    if until.is_some_and(|until| realtime >= until) {
        return false;
    }
    true
}

fn entry_matches_full_text(
    file: &JournalFile<Mmap>,
    data_offsets: &[NonZeroU64],
    needle: &[u8],
    decompressed: &mut Vec<u8>,
    counters: &mut ExplorerQueryCounters,
) -> Result<bool> {
    if needle.is_empty() {
        return Ok(true);
    }
    counters.candidate_data_refs_visited = counters
        .candidate_data_refs_visited
        .saturating_add(data_offsets.len() as u64);
    for data_offset in data_offsets.iter().copied() {
        let payload = materialize_payload(file, data_offset, decompressed, counters)?;
        counters.fts_payloads_scanned = counters.fts_payloads_scanned.saturating_add(1);
        if contains_bytes(&payload, needle) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Default)]
struct FieldDataMap {
    fields: Vec<Vec<u8>>,
    offsets: HashMap<NonZeroU64, usize>,
}

fn build_field_data_map(
    file: &JournalFile<Mmap>,
    fields: &[Vec<u8>],
    counters: &mut ExplorerQueryCounters,
) -> Result<FieldDataMap> {
    let mut map = FieldDataMap::default();
    let mut unique = Vec::<Vec<u8>>::new();
    for field in fields {
        if !unique.iter().any(|existing| existing == field) {
            unique.push(field.clone());
        }
    }

    for field in unique {
        let index = map.fields.len();
        let mut offsets = Vec::new();
        file.field_data_offsets(&field, &mut offsets)?;
        counters.field_linkage_hits = counters
            .field_linkage_hits
            .saturating_add(offsets.len() as u64);
        for offset in offsets {
            map.offsets.insert(offset, index);
        }
        map.fields.push(field);
    }

    Ok(map)
}

fn aggregate_facets(
    file: &JournalFile<Mmap>,
    data_offsets: &[NonZeroU64],
    facet_data: &mut FieldDataMap,
    facet_maps: &mut HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>,
    decompressed: &mut Vec<u8>,
    counters: &mut ExplorerQueryCounters,
) -> Result<()> {
    for data_offset in data_offsets {
        let Some(field_index) = facet_data.offsets.get(data_offset).copied() else {
            continue;
        };
        let field = &facet_data.fields[field_index];
        let value =
            materialize_known_field_value(file, *data_offset, field, decompressed, counters)?;
        counters.facet_values_materialized = counters.facet_values_materialized.saturating_add(1);
        *facet_maps
            .entry(field.clone())
            .or_default()
            .entry(value)
            .or_default() += 1;
    }
    Ok(())
}

fn materialize_display_fields(
    file: &JournalFile<Mmap>,
    data_offsets: &[NonZeroU64],
    display: &ExplorerDisplay,
    display_data: &FieldDataMap,
    decompressed: &mut Vec<u8>,
    counters: &mut ExplorerQueryCounters,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    match display {
        ExplorerDisplay::None => Ok(Vec::new()),
        ExplorerDisplay::All => {
            let mut fields = Vec::new();
            for data_offset in data_offsets.iter().copied() {
                let payload = materialize_payload(file, data_offset, decompressed, counters)?;
                let Some(raw) = split_raw_payload(&payload) else {
                    counters.field_linkage_fallbacks =
                        counters.field_linkage_fallbacks.saturating_add(1);
                    continue;
                };
                fields.push((raw.name.to_vec(), raw.value.to_vec()));
            }
            Ok(fields)
        }
        ExplorerDisplay::Fields(_) => {
            let mut fields = Vec::new();
            for data_offset in data_offsets.iter().copied() {
                let Some(field_index) = display_data.offsets.get(&data_offset).copied() else {
                    continue;
                };
                let field = &display_data.fields[field_index];
                let value = materialize_known_field_value(
                    file,
                    data_offset,
                    field,
                    decompressed,
                    counters,
                )?;
                fields.push((field.clone(), value));
            }
            Ok(fields)
        }
    }
}

fn materialize_known_field_value(
    file: &JournalFile<Mmap>,
    data_offset: NonZeroU64,
    field: &[u8],
    decompressed: &mut Vec<u8>,
    counters: &mut ExplorerQueryCounters,
) -> Result<Vec<u8>> {
    let payload = materialize_payload(file, data_offset, decompressed, counters)?;
    if payload.len() > field.len()
        && &payload[..field.len()] == field
        && payload.get(field.len()) == Some(&b'=')
    {
        return Ok(payload[field.len() + 1..].to_vec());
    }

    counters.field_linkage_fallbacks = counters.field_linkage_fallbacks.saturating_add(1);
    let Some(raw) = split_raw_payload(&payload) else {
        return Err(SdkError::VerificationError(
            "DATA payload has no FIELD=value separator".to_string(),
        ));
    };
    if raw.name == field {
        Ok(raw.value.to_vec())
    } else {
        Err(SdkError::VerificationError(
            "FIELD linkage DATA payload mismatch".to_string(),
        ))
    }
}

fn materialize_payload(
    file: &JournalFile<Mmap>,
    data_offset: NonZeroU64,
    decompressed: &mut Vec<u8>,
    counters: &mut ExplorerQueryCounters,
) -> Result<Vec<u8>> {
    let data = file.data_ref(data_offset)?;
    counters.payloads_materialized = counters.payloads_materialized.saturating_add(1);
    if data.is_compressed() {
        counters.payloads_decompressed = counters.payloads_decompressed.saturating_add(1);
        decompressed.clear();
        let len = data.decompress(decompressed)?;
        return Ok(decompressed[..len].to_vec());
    }
    Ok(data.raw_payload().to_vec())
}

fn constrained_positive_facets(query: &ExplorerQuery) -> HashMap<Vec<u8>, Vec<Vec<u8>>> {
    let mut positive = HashMap::new();
    for filter in &query.filters {
        if filter.kind == ExplorerFilterKind::In && !filter.values.is_empty() {
            positive.insert(filter.field.clone(), filter.values.clone());
        }
    }
    let mut constrained = HashMap::new();
    for facet in &query.facets {
        if let Some(values) = positive.get(facet) {
            constrained.insert(facet.clone(), values.clone());
        }
    }
    constrained
}

fn constrained_facet_counts(
    file: &JournalFile<Mmap>,
    query: &ExplorerQuery,
    candidate_set: &Option<HashSet<NonZeroU64>>,
    all_entry_offsets: &[NonZeroU64],
    counters: &mut ExplorerQueryCounters,
) -> Result<HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>> {
    let constrained = constrained_positive_facets(query);
    let mut maps = HashMap::new();
    let all_entries: Option<HashSet<NonZeroU64>> = candidate_set
        .is_none()
        .then(|| all_entry_offsets.iter().copied().collect());
    let effective_candidates = candidate_set.as_ref().or(all_entries.as_ref());

    for (field, values) in constrained {
        for value in values {
            let filter = ExplorerFilter::field_in(field.clone(), vec![value.clone()]);
            let offsets = filter_value_entry_offsets(file, &filter, counters)?;
            let mut count = 0u64;
            for entry_offset in offsets {
                let in_candidates = effective_candidates
                    .map_or(true, |candidate| candidate.contains(&entry_offset));
                if !in_candidates {
                    continue;
                }
                let entry = file.entry_ref(entry_offset)?;
                if time_matches(
                    query.since_realtime_usec,
                    query.until_realtime_usec,
                    entry.header.realtime,
                ) {
                    count = count.saturating_add(1);
                }
            }
            counters.constrained_facet_counts = counters.constrained_facet_counts.saturating_add(1);
            maps.entry(field.clone())
                .or_insert_with(HashMap::new)
                .insert(value, count);
        }
    }

    Ok(maps)
}

fn constrained_facets_cover_candidate_values(
    file: &JournalFile<Mmap>,
    constrained: &HashMap<Vec<u8>, Vec<Vec<u8>>>,
    candidate_set: &Option<HashSet<NonZeroU64>>,
) -> Result<bool> {
    for (field, selected_values) in constrained {
        let mut selected_offsets = HashSet::new();
        for value in selected_values {
            let payload = payload_for(field, value);
            let hash = file.hash(&payload);
            if let Some(data_offset) = file.find_data_offset(hash, &payload)? {
                selected_offsets.insert(data_offset);
            }
        }

        let mut field_offsets = Vec::new();
        file.field_data_offsets(field, &mut field_offsets)?;
        let mut postings = Vec::new();
        for data_offset in field_offsets {
            if selected_offsets.contains(&data_offset) {
                continue;
            }
            postings.clear();
            collect_data_entry_offsets(file, data_offset, &mut postings)?;
            if candidate_set
                .as_ref()
                .map_or(!postings.is_empty(), |candidate| {
                    postings
                        .iter()
                        .any(|entry_offset| candidate.contains(entry_offset))
                })
            {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

fn facet_maps_to_vec(facet_maps: HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>) -> Vec<ExplorerFacet> {
    let mut facets: Vec<_> = facet_maps
        .into_iter()
        .map(|(field, values)| {
            let mut values: Vec<_> = values
                .into_iter()
                .map(|(value, count)| ExplorerFacetValue { value, count })
                .collect();
            values.sort_by(|a, b| a.value.cmp(&b.value));
            ExplorerFacet { field, values }
        })
        .collect();
    facets.sort_by(|a, b| a.field.cmp(&b.field));
    facets
}

fn payload_for(field: &[u8], value: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(field.len() + 1 + value.len());
    payload.extend_from_slice(field);
    payload.push(b'=');
    payload.extend_from_slice(value);
    payload
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn sort_rows(rows: &mut [ExplorerRow], direction: Direction) {
    rows.sort_by(|a, b| match direction {
        Direction::Forward => (a.realtime, a.seqnum).cmp(&(b.realtime, b.seqnum)),
        Direction::Backward => (b.realtime, b.seqnum).cmp(&(a.realtime, a.seqnum)),
    });
}

fn with_file_label(
    mut fields: Vec<(Vec<u8>, Vec<u8>)>,
    file_label: Option<&[u8]>,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    if let Some(label) = file_label {
        fields.push((b"__SDK_FILE".to_vec(), label.to_vec()));
    }
    fields
}

fn merge_counters(dst: &mut ExplorerQueryCounters, src: &ExplorerQueryCounters) {
    dst.entry_offsets_indexed += src.entry_offsets_indexed;
    dst.filter_data_objects_examined += src.filter_data_objects_examined;
    dst.candidate_entries += src.candidate_entries;
    dst.candidate_data_refs_visited += src.candidate_data_refs_visited;
    dst.data_refs_reported += src.data_refs_reported;
    dst.payloads_materialized += src.payloads_materialized;
    dst.payloads_decompressed += src.payloads_decompressed;
    dst.facet_values_materialized += src.facet_values_materialized;
    dst.fts_payloads_scanned += src.fts_payloads_scanned;
    dst.display_rows_expanded += src.display_rows_expanded;
    dst.constrained_facet_counts += src.constrained_facet_counts;
    dst.field_linkage_hits += src.field_linkage_hits;
    dst.field_linkage_fallbacks += src.field_linkage_fallbacks;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Compression, ReaderOptions};
    use journal_core::file::{
        EntryField, EntryWriteOptions, FieldNamePolicy, JournalFileOptions, JournalWriter, MmapMut,
    };
    use journal_core::repository::File as RepoFile;
    use std::path::Path;

    fn test_uuid(seed: u8) -> uuid::Uuid {
        uuid::Uuid::from_bytes([seed; 16])
    }

    fn create_writer(
        path: &Path,
        compression: Option<Compression>,
    ) -> (JournalFile<MmapMut>, JournalWriter) {
        std::fs::create_dir_all(path.parent().expect("journal parent")).expect("create parent");
        let repo_file = RepoFile::from_path(path).expect("parse repo file");
        let options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));
        let options = if let Some(compression) = compression {
            options
                .with_compression(compression)
                .with_compress_threshold(8)
        } else {
            options
        };
        let mut file = JournalFile::<MmapMut>::create(&repo_file, options).expect("create journal");
        let writer = if let Some(compression) = compression {
            JournalWriter::new_with_compression(&mut file, 1, test_uuid(4), compression, 8)
                .expect("create compressed writer")
        } else {
            JournalWriter::new(&mut file, 1, test_uuid(4)).expect("create writer")
        };
        (file, writer)
    }

    fn write_explorer_test_journal(path: &Path, compression: Option<Compression>) {
        let (mut file, mut writer) = create_writer(path, compression);
        writer
            .add_entry(
                &mut file,
                &[
                    b"SERVICE=api".as_slice(),
                    b"LEVEL=i".as_slice(),
                    b"MESSAGE=alpha repeated repeated repeated".as_slice(),
                    b"USER=alice".as_slice(),
                ],
                1_000,
                1,
            )
            .expect("write first");
        writer
            .add_entry_fields_with_options(
                &mut file,
                [
                    EntryField::raw(b"SERVICE=api"),
                    EntryField::raw(b"LEVEL=e"),
                    EntryField::raw(b"MESSAGE=beta repeated repeated repeated"),
                    EntryField::structured(b"BIN", b"\x00\xff"),
                    EntryField::raw(b"USER=bob"),
                ],
                2_000,
                2,
                EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Journald),
            )
            .expect("write second");
        writer
            .add_entry(
                &mut file,
                &[
                    b"SERVICE=db".as_slice(),
                    b"LEVEL=i".as_slice(),
                    b"MESSAGE=gamma repeated repeated repeated".as_slice(),
                    b"USER=carol".as_slice(),
                ],
                3_000,
                3,
            )
            .expect("write third");
        file.sync().expect("sync journal");
    }

    #[test]
    fn explorer_filter_without_facets_skips_row_payloads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journals/system.journal");
        write_explorer_test_journal(&path, None);

        let mut reader =
            FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open reader");
        let result = reader
            .explorer_query(&ExplorerQuery {
                filters: vec![ExplorerFilter::field_in(
                    b"SERVICE".to_vec(),
                    vec![b"api".to_vec()],
                )],
                display: ExplorerDisplay::None,
                limit: Some(10),
                ..ExplorerQuery::default()
            })
            .expect("query");

        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.total_candidates, 2);
        assert_eq!(result.counters.payloads_materialized, 0);
        assert_eq!(result.counters.candidate_data_refs_visited, 0);
    }

    #[test]
    fn explorer_selected_facet_materializes_only_facet_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journals/system.journal");
        write_explorer_test_journal(&path, None);

        let mut reader =
            FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open reader");
        let result = reader
            .explorer_query(&ExplorerQuery {
                facets: vec![b"LEVEL".to_vec()],
                display: ExplorerDisplay::None,
                limit: Some(0),
                ..ExplorerQuery::default()
            })
            .expect("query");

        assert_eq!(result.rows.len(), 0);
        assert_eq!(result.facets[0].field, b"LEVEL");
        assert_eq!(
            result.facets[0].values,
            vec![
                ExplorerFacetValue {
                    value: b"e".to_vec(),
                    count: 1
                },
                ExplorerFacetValue {
                    value: b"i".to_vec(),
                    count: 2
                }
            ]
        );
        assert_eq!(result.counters.facet_values_materialized, 3);
        assert_eq!(result.counters.payloads_materialized, 3);
    }

    #[test]
    fn explorer_constrained_facet_falls_back_for_repeated_field_extra_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journals/system.journal");
        let (mut file, mut writer) = create_writer(&path, None);
        writer
            .add_entry(
                &mut file,
                &[b"TAG=a".as_slice(), b"TAG=b".as_slice()],
                1_000,
                1,
            )
            .expect("write first");
        writer
            .add_entry(&mut file, &[b"TAG=a".as_slice()], 2_000, 2)
            .expect("write second");
        file.sync().expect("sync journal");

        let mut reader =
            FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open reader");
        let result = reader
            .explorer_query(&ExplorerQuery {
                filters: vec![ExplorerFilter::field_in(
                    b"TAG".to_vec(),
                    vec![b"a".to_vec()],
                )],
                facets: vec![b"TAG".to_vec()],
                display: ExplorerDisplay::None,
                limit: Some(10),
                ..ExplorerQuery::default()
            })
            .expect("query");

        assert_eq!(result.counters.candidate_data_refs_visited, 3);
        assert_eq!(
            result.facets[0].values,
            vec![
                ExplorerFacetValue {
                    value: b"a".to_vec(),
                    count: 2
                },
                ExplorerFacetValue {
                    value: b"b".to_vec(),
                    count: 1
                }
            ]
        );
    }

    #[test]
    fn explorer_skips_compressed_irrelevant_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journals/system.journal");
        write_explorer_test_journal(&path, Some(Compression::Zstd));

        let mut reader =
            FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open reader");
        let result = reader
            .explorer_query(&ExplorerQuery {
                facets: vec![b"LEVEL".to_vec()],
                display: ExplorerDisplay::None,
                limit: Some(0),
                ..ExplorerQuery::default()
            })
            .expect("query");

        assert_eq!(result.counters.facet_values_materialized, 3);
        assert_eq!(result.counters.payloads_decompressed, 0);
    }

    #[test]
    fn explorer_filtered_unique_uses_target_field_chain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journals/system.journal");
        write_explorer_test_journal(&path, None);

        let mut reader =
            FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open reader");
        let result = reader
            .explorer_unique(&ExplorerUniqueQuery {
                field: b"USER".to_vec(),
                filters: vec![ExplorerFilter::field_in(
                    b"SERVICE".to_vec(),
                    vec![b"api".to_vec()],
                )],
                limit: None,
                skip: 0,
                include_counts: true,
                since_realtime_usec: None,
                until_realtime_usec: None,
            })
            .expect("unique");

        assert_eq!(
            result.values,
            vec![
                ExplorerUniqueValue {
                    value: b"alice".to_vec(),
                    count: Some(1)
                },
                ExplorerUniqueValue {
                    value: b"bob".to_vec(),
                    count: Some(1)
                }
            ]
        );
        assert_eq!(result.counters.payloads_materialized, 2);
    }

    #[test]
    fn explorer_filtered_unique_sorts_before_pagination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journals/system.journal");
        write_explorer_test_journal(&path, None);

        let mut reader =
            FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open reader");
        let result = reader
            .explorer_unique(&ExplorerUniqueQuery {
                field: b"USER".to_vec(),
                filters: Vec::new(),
                limit: Some(1),
                skip: 1,
                include_counts: false,
                since_realtime_usec: None,
                until_realtime_usec: None,
            })
            .expect("unique");

        assert_eq!(
            result.values,
            vec![ExplorerUniqueValue {
                value: b"bob".to_vec(),
                count: None
            }]
        );
    }

    #[test]
    fn explorer_data_refs_report_offsets_without_payload_materialization() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journals/system.journal");
        write_explorer_test_journal(&path, None);

        let mut reader =
            FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open reader");
        assert!(reader.next().expect("next"));
        let mut refs = Vec::new();
        reader
            .visit_entry_data_refs(|data_ref| {
                refs.push(data_ref);
                Ok(())
            })
            .expect("visit refs");

        assert_eq!(refs.len(), 4);
        assert!(refs.iter().all(|data_ref| data_ref.offset > 0));
    }
}
