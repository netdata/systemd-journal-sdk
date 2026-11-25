//! Boot listing, as `journalctl --list-boots` reports it.

use super::*;
use journal_core::error::Result as CoreResult;
use journal_core::file::offset_array::Direction as ArrayDirection;
use std::collections::HashMap;

/// One boot's first and last entries within a reader.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BootRange {
    pub(crate) boot_id: [u8; 16],
    pub(crate) first: DirectoryEntryKey,
    pub(crate) last: DirectoryEntryKey,
}

impl BootRange {
    pub(crate) fn times(&self) -> BootTimes {
        BootTimes {
            boot_id: self.boot_id,
            first: self.first.realtime,
            last: self.last.realtime,
        }
    }
}

/// A boot's first and last realtime.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BootTimes {
    pub(crate) boot_id: [u8; 16],
    pub(crate) first: u64,
    pub(crate) last: u64,
}

impl FileReader {
    /// Lists the boots in this file, oldest first, as `journalctl
    /// --list-boots` does. It does not change matches or the read position.
    pub fn list_boots(&self) -> Result<Vec<BootInfo>> {
        if let Ok(ranges) = self.boot_ranges()
            && !ranges.is_empty()
        {
            return Ok(order_boots(ranges.iter().map(BootRange::times).collect()));
        }
        let mut boots = HashMap::new();
        self.scan_boot_times(&mut boots)?;
        Ok(order_boots(boots.into_values().collect()))
    }

    /// First and last entries of every boot that has `_BOOT_ID` DATA in this
    /// file, from the DATA objects' entry arrays. This is what a `_BOOT_ID`
    /// match with seek_head/seek_tail finds, without decoding entries or
    /// touching the caller's matches and position.
    pub(crate) fn boot_ranges(&self) -> Result<Vec<BootRange>> {
        let seqnum_id = self.header_snapshot.header.seqnum_id;
        self.inner
            .with_file(|file| with_reader_recovery(file, || file_boot_ranges(file, seqnum_id)))
    }

    /// Fallback for journals without `_BOOT_ID` DATA: reads every entry
    /// header and keeps each boot's lowest and highest realtime.
    pub(crate) fn scan_boot_times(&self, boots: &mut HashMap<[u8; 16], BootTimes>) -> Result<()> {
        let scanned = self
            .inner
            .with_file(|file| with_reader_recovery(file, || file_boot_times(file)))?;
        for times in scanned.into_values() {
            merge_boot_times(boots, times);
        }
        Ok(())
    }
}

/// Retries after the reader discovers a shorter readable prefix, as the
/// reader's own seeks do, so dirty archives list the boots the reader sees.
fn with_reader_recovery<T>(
    file: &JournalFile<Mmap>,
    mut run: impl FnMut() -> CoreResult<T>,
) -> Result<T> {
    loop {
        match run() {
            Err(err) if file.recover_reader_tail(&err)? => continue,
            result => return result.map_err(Into::into),
        }
    }
}

fn file_boot_ranges(file: &JournalFile<Mmap>, seqnum_id: [u8; 16]) -> CoreResult<Vec<BootRange>> {
    let mut data_offsets = Vec::new();
    let mut decompressed = Vec::new();
    for item in file.field_data_objects_with_offsets(b"_BOOT_ID")? {
        let (offset, data) = item?;
        let payload = if data.is_compressed() {
            decompressed.clear();
            let len = data.decompress(&mut decompressed)?;
            &decompressed[..len]
        } else {
            data.raw_payload()
        };
        let boot_id = payload
            .strip_prefix(b"_BOOT_ID=")
            .and_then(parse_canonical_boot_id);
        drop(data);
        if let Some(boot_id) = boot_id {
            data_offsets.push((offset, boot_id));
        }
    }

    let key_at = |offset: NonZeroU64| -> CoreResult<DirectoryEntryKey> {
        let entry = file.entry_ref(offset)?;
        Ok(DirectoryEntryKey {
            seqnum_id,
            seqnum: entry.header.seqnum,
            boot_id: entry.header.boot_id,
            monotonic: entry.header.monotonic,
            realtime: entry.header.realtime,
            xor_hash: entry.header.xor_hash,
        })
    };
    let mut ranges = Vec::with_capacity(data_offsets.len());
    for (offset, boot_id) in data_offsets {
        let first = file.data_object_directed_partition_point(
            offset,
            |_| Ok(false),
            ArrayDirection::Forward,
        )?;
        let last = file.data_object_directed_partition_point(
            offset,
            |_| Ok(true),
            ArrayDirection::Backward,
        )?;
        if let (Some(first), Some(last)) = (first, last) {
            ranges.push(BootRange {
                boot_id,
                first: key_at(first)?,
                last: key_at(last)?,
            });
        }
    }
    Ok(ranges)
}

fn file_boot_times(file: &JournalFile<Mmap>) -> CoreResult<HashMap<[u8; 16], BootTimes>> {
    let mut boots = HashMap::new();
    let mut offsets = Vec::new();
    file.entry_offsets(&mut offsets)?;
    for offset in offsets {
        let entry = file.entry_ref(offset)?;
        let (boot_id, realtime) = (entry.header.boot_id, entry.header.realtime);
        if boot_id != [0; 16] {
            merge_boot_times(
                &mut boots,
                BootTimes {
                    boot_id,
                    first: realtime,
                    last: realtime,
                },
            );
        }
    }
    Ok(boots)
}

pub(crate) fn merge_boot_times(boots: &mut HashMap<[u8; 16], BootTimes>, times: BootTimes) {
    boots
        .entry(times.boot_id)
        .and_modify(|boot| {
            boot.first = boot.first.min(times.first);
            boot.last = boot.last.max(times.last);
        })
        .or_insert(times);
}

/// Accepts only the 32 lowercase hex digits journald writes: journalctl
/// matches `_BOOT_ID=<id>` in that form, so other spellings never belong to a
/// boot it lists. The all-zero ID is not a boot.
pub(crate) fn parse_canonical_boot_id(value: &[u8]) -> Option<[u8; 16]> {
    if value.len() != 32 {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let mut id = [0u8; 16];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = (digit(value[2 * i])? << 4) | digit(value[2 * i + 1])?;
    }
    (id != [0; 16]).then_some(id)
}

/// Sorts by last entry, then first entry, then boot ID, and numbers the
/// newest boot 0.
pub(crate) fn order_boots(mut times: Vec<BootTimes>) -> Vec<BootInfo> {
    times.sort_by(|a, b| {
        a.last
            .cmp(&b.last)
            .then_with(|| a.first.cmp(&b.first))
            .then_with(|| a.boot_id.cmp(&b.boot_id))
    });
    let base = 1 - times.len() as i64;
    times
        .into_iter()
        .enumerate()
        .map(|(i, boot)| BootInfo {
            index: base + i as i64,
            boot_id: hex::encode(boot.boot_id),
            first_entry: boot.first as i64,
            last_entry: boot.last as i64,
        })
        .collect()
}
