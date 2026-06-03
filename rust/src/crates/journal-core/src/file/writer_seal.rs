use super::mmap::MmapMut;
use super::object::*;
use super::writer::JournalWriter;
use crate::error::Result;
use crate::file::JournalFile;
use crate::seal::TAG_LENGTH;
use std::num::NonZeroU64;

impl JournalWriter {
    pub(super) fn ensure_first_tag(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
    ) -> Result<()> {
        if !self.first_tag_written && self.seal.is_some() {
            self.append_first_tag(journal_file)?;
            self.first_tag_written = true;
        }
        Ok(())
    }

    fn append_first_tag(&mut self, journal_file: &mut JournalFile<MmapMut>) -> Result<()> {
        self.hmac_put_header(journal_file)?;
        let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
        let (dht_offset, fht_offset) = {
            let header = journal_file.journal_header_ref();
            (
                header
                    .data_hash_table_offset
                    .map(|o| o.get() - object_header_size),
                header
                    .field_hash_table_offset
                    .map(|o| o.get() - object_header_size),
            )
        };
        // systemd journal-authenticate.c:478-487: field hash table first, then data hash table.
        if let Some(fht_offset) = fht_offset {
            self.hmac_put_hash_table_object(journal_file, NonZeroU64::new(fht_offset).unwrap())?;
        }
        if let Some(dht_offset) = dht_offset {
            self.hmac_put_hash_table_object(journal_file, NonZeroU64::new(dht_offset).unwrap())?;
        }
        self.append_tag(journal_file)
    }

    fn append_tag(&mut self, journal_file: &mut JournalFile<MmapMut>) -> Result<()> {
        let tag_offset = self.append_offset;

        // Increment n_tags before computing the HMAC, matching systemd's journal_file_tag_seqnum().
        let seqnum = {
            let header = journal_file.journal_header_mut();
            header.n_tags += 1;
            header.n_tags
        };

        let epoch = self.seal.as_ref().unwrap().epoch();

        let object_header_size = std::mem::size_of::<ObjectHeader>() as usize;
        let tag_meta_size = 16;
        let total_size = object_header_size + tag_meta_size + TAG_LENGTH;
        let aligned_size = (total_size + 7) & !7;
        let mut buf = vec![0u8; aligned_size];

        buf[0] = ObjectType::Tag as u8;
        buf[8..16].copy_from_slice(&(total_size as u64).to_le_bytes());

        buf[object_header_size..object_header_size + 8].copy_from_slice(&seqnum.to_le_bytes());
        buf[object_header_size + 8..object_header_size + 16].copy_from_slice(&epoch.to_le_bytes());

        if let Some(ref mut seal) = self.seal {
            seal.hmac_put_object_bytes(&buf, ObjectType::Tag, total_size as u64, false);
            let digest = seal.hmac_finalize();
            buf[object_header_size + 16..object_header_size + 16 + TAG_LENGTH]
                .copy_from_slice(&digest);
            seal.hmac_reset();
        }

        {
            let mut tag_guard = journal_file.tag_mut(tag_offset, true)?;
            tag_guard.header.seqnum = seqnum;
            tag_guard.header.epoch = epoch;
            let digest = &buf[object_header_size + 16..object_header_size + 16 + TAG_LENGTH];
            tag_guard.header.tag.copy_from_slice(digest);
        }

        self.object_added(journal_file, tag_offset, total_size as u64)
    }

    pub(super) fn maybe_append_tag(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        realtime: u64,
    ) -> Result<()> {
        if !self.seal_needs_evolution(realtime)? {
            return Ok(());
        }

        self.append_tag(journal_file)?;
        let Some(goal) = self.seal_goal_epoch(realtime)? else {
            return Ok(());
        };
        self.append_intermediate_tags(journal_file, goal)
    }

    fn seal_needs_evolution(&self, realtime: u64) -> Result<bool> {
        match self.seal {
            Some(ref seal) => seal.need_evolve(realtime),
            None => Ok(false),
        }
    }

    fn seal_goal_epoch(&self, realtime: u64) -> Result<Option<u64>> {
        match self.seal {
            Some(ref seal) => Ok(Some(seal.goal_epoch(realtime)?)),
            None => Ok(None),
        }
    }

    fn seal_epoch(&self) -> Option<u64> {
        self.seal.as_ref().map(|seal| seal.epoch())
    }

    fn append_intermediate_tags(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        goal: u64,
    ) -> Result<()> {
        while self.seal_epoch().is_some_and(|epoch| epoch < goal) {
            if let Some(ref mut seal) = self.seal {
                seal.evolve_state();
            }
            if self.seal_epoch().is_some_and(|epoch| epoch < goal) {
                self.append_tag(journal_file)?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn hmac_put_header(&mut self, journal_file: &JournalFile<MmapMut>) -> Result<()> {
        if self.seal.is_none() {
            return Ok(());
        }
        let header = journal_file.journal_header_ref();
        let bytes = zerocopy::IntoBytes::as_bytes(header);
        if let Some(ref mut seal) = self.seal {
            seal.hmac_put_header_ranges(bytes);
        }
        Ok(())
    }

    fn hmac_put_hash_table_object(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        offset: NonZeroU64,
    ) -> Result<()> {
        if self.seal.is_none() {
            return Ok(());
        }
        let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
        let bytes = journal_file.read_bytes_at(offset.get(), object_header_size)?;
        if let Some(ref mut seal) = self.seal {
            let typ = if bytes.is_empty() {
                ObjectType::Unused
            } else {
                match bytes[0] {
                    4 => ObjectType::DataHashTable,
                    5 => ObjectType::FieldHashTable,
                    _ => ObjectType::Unused,
                }
            };
            seal.hmac_put_object_bytes(&bytes, typ, object_header_size, false);
        }
        Ok(())
    }

    pub(super) fn hmac_put_object(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        offset: u64,
        object_type: ObjectType,
    ) -> Result<()> {
        if self.seal.is_none() {
            return Ok(());
        }
        let is_compact = Self::is_compact(journal_file);
        let offset_nz = NonZeroU64::new(offset).unwrap();
        let oh = journal_file.object_header_ref(offset_nz)?;
        let size = oh.size as usize;
        let bytes = journal_file.read_bytes_at(offset, size as u64)?;
        if let Some(ref mut seal) = self.seal {
            seal.hmac_put_object_bytes(&bytes, object_type, size as u64, is_compact);
        }
        Ok(())
    }
}
