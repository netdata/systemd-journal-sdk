//! Forward Secure Sealing support for journal writers.
//!
//! Implements file-format sealing with deterministic synthetic keys,
//! matching systemd v260.1 HMAC byte ranges and tag object layout.
//!
//! HMAC byte ranges follow systemd journal-authenticate.c exactly:
//! - Header: signature..state, file_id..tail_entry_boot_id, seqnum_id..arena_size,
//!   data_hash_table_offset..tail_object_offset
//! - DATA: object header + hash + stored payload
//! - FIELD: object header + hash + payload
//! - ENTRY: object header + seqnum..end
//! - HASH_TABLE / ENTRY_ARRAY: object header only
//! - TAG: object header + seqnum + epoch (not the tag value itself)

use crate::error::{JournalError, Result};
use crate::file::ObjectType;
use crate::fss::{
    RECOMMENDED_SECPAR, RECOMMENDED_SEEDLEN, evolve, gen_mk, gen_state0, get_epoch, get_key,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const TAG_LENGTH: usize = 256 / 8;

/// Type alias for HMAC-SHA256.
type HmacSha256 = Hmac<Sha256>;

/// Configures Forward Secure Sealing for a journal writer.
#[derive(Debug, Clone)]
pub struct SealOptions {
    pub seed: [u8; RECOMMENDED_SEEDLEN],
    pub interval_usec: u64,
    pub start_usec: u64,
}

impl SealOptions {
    pub fn new(seed: [u8; RECOMMENDED_SEEDLEN], interval_usec: u64, start_usec: u64) -> Self {
        Self {
            seed,
            interval_usec,
            start_usec,
        }
    }
}

/// Per-writer FSS+HMAC state.
pub struct SealState {
    fsprg_state: Vec<u8>,
    #[allow(dead_code)]
    msk: Vec<u8>,
    #[allow(dead_code)]
    seed: [u8; RECOMMENDED_SEEDLEN],
    interval: u64,
    start: u64,
    hmac: Option<HmacSha256>,
    hmac_running: bool,
}

impl SealState {
    pub fn new(opts: &SealOptions) -> Result<Self> {
        let (msk, mpk) = gen_mk(&opts.seed, RECOMMENDED_SECPAR);
        let state0 = gen_state0(&mpk, &opts.seed);
        Ok(Self {
            fsprg_state: state0,
            msk,
            seed: opts.seed,
            interval: opts.interval_usec,
            start: opts.start_usec,
            hmac: None,
            hmac_running: false,
        })
    }

    pub fn epoch(&self) -> u64 {
        get_epoch(&self.fsprg_state)
    }

    pub fn goal_epoch(&self, realtime: u64) -> Result<u64> {
        if self.start == 0 || self.interval == 0 {
            return Err(JournalError::FssVerificationError);
        }
        if realtime < self.start {
            return Err(JournalError::FssVerificationError);
        }
        Ok((realtime - self.start) / self.interval)
    }

    pub fn need_evolve(&self, realtime: u64) -> Result<bool> {
        let goal = self.goal_epoch(realtime)?;
        let epoch = self.epoch();
        if epoch > goal {
            return Err(JournalError::FssVerificationError);
        }
        Ok(epoch != goal)
    }

    pub fn hmac_start(&mut self) {
        if self.hmac_running {
            return;
        }
        let key = get_key(&self.fsprg_state, TAG_LENGTH, 0);
        self.hmac = Some(HmacSha256::new_from_slice(&key).expect("HMAC accepts any key length"));
        self.hmac_running = true;
    }

    pub fn hmac_write(&mut self, data: &[u8]) {
        self.hmac_start();
        if let Some(ref mut hmac) = self.hmac {
            hmac.update(data);
        }
    }

    pub fn hmac_reset(&mut self) {
        self.hmac_running = false;
        self.hmac = None;
    }

    pub fn hmac_finalize(&mut self) -> [u8; TAG_LENGTH] {
        let result = self
            .hmac
            .take()
            .expect("hmac_finalize called without active HMAC")
            .finalize()
            .into_bytes();
        self.hmac_running = false;
        let mut out = [0u8; TAG_LENGTH];
        out.copy_from_slice(&result);
        out
    }

    /// HMAC the immutable header byte ranges (systemd journal-authenticate.c:329-354).
    ///
    /// Four ranges from the serialized on-disk header:
    /// - signature through just before state (bytes 0..16)
    /// - file_id through just before tail_entry_boot_id (bytes 24..56)
    /// - seqnum_id through just before arena_size (bytes 72..96)
    /// - data_hash_table_offset through just before tail_object_offset (bytes 104..136)
    pub fn hmac_put_header_ranges(&mut self, header_bytes: &[u8]) {
        self.hmac_start();
        // signature + compatible_flags + incompatible_flags; state is excluded.
        self.hmac_write(&header_bytes[0..16]);

        // file_id + machine_id; tail_entry_boot_id is excluded.
        self.hmac_write(&header_bytes[24..56]);

        // seqnum_id + header_size; arena_size is excluded.
        self.hmac_write(&header_bytes[72..96]);

        // data/field hash-table offsets and sizes; tail_object_offset is excluded.
        self.hmac_write(&header_bytes[104..136]);
    }

    /// HMAC an object's immutable bytes based on its type.
    ///
    /// `object_bytes` must contain the full serialized on-disk object.
    /// `object_size` is the actual object size from the object header (not the buffer length).
    /// `is_compact` must be true for journals with HEADER_INCOMPATIBLE_COMPACT.
    ///
    /// Byte ranges follow systemd journal-authenticate.c:267-327:
    /// - All objects: object header (up to payload)
    /// - DATA: + hash + stored payload (offset 64 regular, 72 compact)
    /// - FIELD: + hash + payload
    /// - ENTRY: + seqnum through end
    /// - HASH_TABLE / ENTRY_ARRAY: nothing beyond header
    /// - TAG: + seqnum + epoch (not the tag value)
    pub fn hmac_put_object_bytes(
        &mut self,
        object_bytes: &[u8],
        typ: ObjectType,
        object_size: u64,
        is_compact: bool,
    ) {
        self.hmac_start();

        let object_header_size: u64 = 16; // type(1) + flags(1) + reserved(6) + size(8)

        // Object header is always HMAC'd for all types
        self.hmac_write(&object_bytes[..object_header_size as usize]);

        match typ {
            ObjectType::Data => {
                // systemd journal-authenticate.c:293-294:
                //   hmac(data.hash) + hmac(payload from payload_offset to end)
                // Regular payload offset = offsetof(Object, data.regular.payload) = 64
                // Compact payload offset = offsetof(Object, data.compact.payload) = 72
                let hash_offset = object_header_size as usize;
                self.hmac_write(&object_bytes[hash_offset..hash_offset + 8]);

                let payload_offset: u64 = if is_compact { 72 } else { 64 };
                if object_size > payload_offset {
                    let payload_size = (object_size - payload_offset) as usize;
                    let start = payload_offset as usize;
                    if start + payload_size <= object_bytes.len() {
                        self.hmac_write(&object_bytes[start..start + payload_size]);
                    }
                }
            }
            ObjectType::Field => {
                // hash (8 bytes at offset 16) + payload
                let hash_offset = object_header_size as usize;
                self.hmac_write(&object_bytes[hash_offset..hash_offset + 8]);

                // FieldObjectHeader: object_header(16) + hash(8) + next_hash_offset(8) +
                //   head_data_offset(8) = 40
                // Payload starts at offset 40
                let payload_offset: u64 = 40;
                if object_size > payload_offset {
                    let payload_size = (object_size - payload_offset) as usize;
                    let start = payload_offset as usize;
                    if start + payload_size <= object_bytes.len() {
                        self.hmac_write(&object_bytes[start..start + payload_size]);
                    }
                }
            }
            ObjectType::Entry => {
                // Everything from seqnum onward
                // EntryObjectHeader: object_header(16) + seqnum(8) + xor_hash(8) +
                //   boot_id(16) + monotonic(8) + realtime(8) + n_items(8) = 72
                // But systemd HMACs from &o->entry.seqnum, which is at offset 16 (after object header)
                // Length: object_size - object_header_size
                if object_size > object_header_size {
                    let rest_size = (object_size - object_header_size) as usize;
                    let start = object_header_size as usize;
                    if start + rest_size <= object_bytes.len() {
                        self.hmac_write(&object_bytes[start..start + rest_size]);
                    }
                }
            }
            ObjectType::DataHashTable | ObjectType::FieldHashTable | ObjectType::EntryArray => {
                // Nothing beyond object header: everything is mutable
            }
            ObjectType::Tag => {
                // seqnum (8 bytes) + epoch (8 bytes), not the tag value itself
                // TagObjectHeader: object_header(16) + seqnum(8) + epoch(8) + tag(32) = 64
                // seqnum is at offset 16, epoch at offset 24
                let seqnum_offset = object_header_size as usize;
                self.hmac_write(&object_bytes[seqnum_offset..seqnum_offset + 8]);
                self.hmac_write(&object_bytes[seqnum_offset + 8..seqnum_offset + 16]);
            }
            ObjectType::Unused => {}
        }
    }

    pub fn evolve_state(&mut self) {
        self.fsprg_state = evolve(&self.fsprg_state);
    }
}
