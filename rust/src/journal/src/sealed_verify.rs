use super::reader_helpers::verify_journal_file_strict;
use super::*;
use hmac::{Hmac, Mac};
use journal_core::fss::{RECOMMENDED_SECPAR, gen_mk, gen_state0, get_key, seek};
use journal_core::seal::TAG_LENGTH;
use sha2::Sha256;
use verify_graph::ByteSource;

struct JournalFileVerifySource<'a> {
    file: &'a JournalFile<Mmap>,
    len: u64,
}

impl<'a> JournalFileVerifySource<'a> {
    fn new(file: &'a JournalFile<Mmap>) -> Result<Self> {
        Ok(Self {
            file,
            len: file.reader_file_size()?,
        })
    }
}

impl ByteSource for JournalFileVerifySource<'_> {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_vec(&self, offset: u64, size: u64) -> std::result::Result<Vec<u8>, String> {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| format!("slice {offset}..+{size} overflows"))?;
        if end > self.len {
            return Err(format!("slice {offset}..{end} exceeds file bounds"));
        }
        self.file
            .read_unaligned_bytes_at(offset, size)
            .map_err(|err| err.to_string())
    }
}

/// Validate the structural integrity of a journal file.
///
/// Opens the file (decompressing `.zst` if needed), validates the header,
/// and walks all entries and their referenced data objects.
/// Any parse or decompression error is reported as an `SdkError` with
/// a message containing "corrupt" so callers can detect verification failures.
///
/// For sealed journals, this validates structure only; use `verify_file_with_key`
/// when TAG/HMAC verification is required.
pub fn verify_file(path: impl AsRef<Path>) -> Result<()> {
    verify_file_with_options(path, ReaderOptions::snapshot())
}

pub(crate) fn verify_file_with_options(
    path: impl AsRef<Path>,
    options: ReaderOptions,
) -> Result<()> {
    let path = path.as_ref();
    let reader = FileReader::open_with_options(path, verify_reader_options(options))
        .map_err(|err| SdkError::VerificationError(format!("open/decompression failed: {err}")))?;
    verify_reader_object_graph(&reader)?;
    reader.inner.with_file(verify_journal_file_strict)
}

/// Validate the integrity of a journal file with a verification key.
///
/// For sealed files, parses the key and validates TAG/HMAC chains.
/// For unsealed files, behaves like `verify_file`.
pub fn verify_file_with_key(path: impl AsRef<Path>, verification_key: &str) -> Result<()> {
    verify_file_with_key_options(path, verification_key, ReaderOptions::snapshot())
}

pub(crate) fn verify_file_with_key_options(
    path: impl AsRef<Path>,
    verification_key: &str,
    options: ReaderOptions,
) -> Result<()> {
    let path = path.as_ref();
    let reader = FileReader::open_with_options(path, verify_reader_options(options))
        .map_err(|err| SdkError::VerificationError(format!("open/decompression failed: {err}")))?;

    reader.inner.with_file(|file| {
        let source = JournalFileVerifySource::new(file)?;
        if source.len() < HEADER_MIN_SIZE {
            return Err(SdkError::VerificationError("file too small".into()));
        }
        verify_graph::verify_object_graph_source(&source)
            .map_err(|err| SdkError::VerificationError(format!("corrupt object graph: {err}")))?;

        let compatible_flags = read_u32_for_verify(&source, 8, "compatible_flags")?;
        let incompatible_flags = read_u32_for_verify(&source, 12, "incompatible_flags")?;
        let sealed = (compatible_flags & 1) != 0;
        if sealed {
            let (seed, start_usec, interval_usec) = parse_verification_key(verification_key)
                .map_err(|e| {
                    SdkError::VerificationError(format!("invalid verification key: {e}"))
                })?;
            verify_sealed(
                &source,
                compatible_flags,
                incompatible_flags,
                seed,
                start_usec,
                interval_usec,
            )?;
        }
        Ok(())
    })?;
    reader.inner.with_file(verify_journal_file_strict)
}

fn verify_reader_options(mut options: ReaderOptions) -> ReaderOptions {
    options.bounds = ReaderBounds::Snapshot;
    options
}

fn verify_reader_object_graph(reader: &FileReader) -> Result<()> {
    reader.inner.with_file(|file| {
        let source = JournalFileVerifySource::new(file)?;
        verify_graph::verify_object_graph_source(&source)
            .map_err(|err| SdkError::VerificationError(format!("corrupt object graph: {err}")))
    })
}

fn parse_verification_key(key: &str) -> std::result::Result<([u8; 12], u64, u64), String> {
    let bytes = key.as_bytes();
    let (seed, slash_offset) = parse_verification_seed(bytes)?;
    if slash_offset >= bytes.len() || bytes[slash_offset] != b'/' {
        return Err("missing / separator".into());
    }
    let (start_usec, dash_offset) = parse_verification_hex_value(bytes, slash_offset + 1, "start")?;
    if dash_offset >= bytes.len() || bytes[dash_offset] != b'-' {
        return Err("bad start hex".into());
    }
    let (interval_usec, end_offset) =
        parse_verification_hex_value(bytes, dash_offset + 1, "interval")?;
    if end_offset != bytes.len() {
        return Err("trailing data".into());
    }
    if interval_usec == 0 {
        return Err("zero interval".into());
    }
    Ok((seed, start_usec, interval_usec))
}

fn parse_verification_seed(bytes: &[u8]) -> std::result::Result<([u8; 12], usize), String> {
    let mut seed = [0u8; 12];
    let mut i = 0;
    for c in 0..12 {
        let (next, val) = parse_verification_seed_byte(bytes, i)?;
        seed[c] = val;
        i = next;
    }
    Ok((seed, i))
}

fn parse_verification_seed_byte(
    bytes: &[u8],
    start: usize,
) -> std::result::Result<(usize, u8), String> {
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    if i + 2 > bytes.len() {
        return Err("seed too short".into());
    }
    let val = u8::from_str_radix(std::str::from_utf8(&bytes[i..i + 2]).unwrap_or("xx"), 16)
        .map_err(|_| "bad seed hex".to_string())?;
    Ok((i + 2, val))
}

fn parse_verification_hex_value(
    bytes: &[u8],
    start: usize,
    label: &str,
) -> std::result::Result<(u64, usize), String> {
    let (next, ok) = consume_hex(bytes, start);
    if !ok {
        return Err(format!("bad {label} hex"));
    }
    let value = u64::from_str_radix(std::str::from_utf8(&bytes[start..next]).unwrap_or("0"), 16)
        .map_err(|_| format!("bad {label} hex"))?;
    Ok((value, next))
}

fn consume_hex(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
        i += 1;
    }
    (i, i > start)
}

pub(super) fn align8(v: u64) -> u64 {
    v.checked_add(7).map(|value| value & !7).unwrap_or(0)
}

fn read_verify_bytes(
    source: &dyn ByteSource,
    offset: u64,
    len: u64,
    label: &str,
) -> Result<Vec<u8>> {
    let label_text = label.to_owned();
    let end = offset.checked_add(len).ok_or_else(|| {
        SdkError::VerificationError(format!(
            "{} read at offset {} overflows",
            label_text, offset
        ))
    })?;
    if end > source.len() {
        return Err(SdkError::VerificationError(format!(
            "{} read at offset {} exceeds file bounds",
            label_text, offset
        )));
    }
    source
        .read_vec(offset, len)
        .map_err(|err| SdkError::VerificationError(format!("{label_text}: {err}")))
}

fn read_u32_for_verify(source: &dyn ByteSource, offset: u64, label: &str) -> Result<u32> {
    let bytes = read_verify_bytes(source, offset, 4, label)?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        SdkError::VerificationError(format!("{label} has invalid length"))
    })?))
}

fn read_u64_for_verify(source: &dyn ByteSource, offset: u64, label: &str) -> Result<u64> {
    let bytes = read_verify_bytes(source, offset, 8, label)?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        SdkError::VerificationError(format!("{label} has invalid length"))
    })?))
}

const COMPATIBLE_SEALED_CONTINUOUS: u32 = 1 << 2;
pub(super) const HEADER_MIN_SIZE: u64 = 208;
pub(super) const OBJECT_TYPE_DATA: u8 = 1;
const OBJECT_TYPE_FIELD: u8 = 2;
const OBJECT_TYPE_ENTRY: u8 = 3;
const OBJECT_TYPE_DATA_HASH_TABLE: u8 = 4;
const OBJECT_TYPE_FIELD_HASH_TABLE: u8 = 5;
const OBJECT_TYPE_ENTRY_ARRAY: u8 = 6;
pub(super) const OBJECT_TYPE_TAG: u8 = 7;
pub(super) const OBJECT_HEADER_SIZE: u64 = 16;
pub(super) const DATA_OBJECT_HEADER_SIZE: u64 = 64;
pub(super) const COMPACT_DATA_OBJECT_HEADER_SIZE: u64 = 72;
const FIELD_OBJECT_HEADER_SIZE: u64 = 40;
pub(super) const INCOMPATIBLE_COMPACT: u32 = 1 << 4;
const INCOMPATIBLE_COMPRESSED_XZ: u32 = 1 << 0;
const INCOMPATIBLE_COMPRESSED_LZ4: u32 = 1 << 1;
const INCOMPATIBLE_COMPRESSED_ZSTD: u32 = 1 << 3;
const OBJECT_COMPRESSED_XZ: u8 = 1 << 0;
const OBJECT_COMPRESSED_LZ4: u8 = 1 << 1;
const OBJECT_COMPRESSED_ZSTD: u8 = 1 << 2;

#[derive(Clone, Copy)]
struct SealedVerifyObject {
    offset: u64,
    typ: u8,
    flags: u8,
    size: u64,
    aligned_size: u64,
}

#[derive(Clone, Copy)]
struct SealedVerifyEntry {
    seqnum: u64,
    realtime: u64,
    monotonic: u64,
    boot_id: [u8; 16],
}

struct SealedVerifyState<'a> {
    source: &'a dyn ByteSource,
    compatible_flags: u32,
    incompatible_flags: u32,
    seed: [u8; 12],
    msk: Vec<u8>,
    state0: Vec<u8>,
    start_epoch: u64,
    interval_usec: u64,
    is_compact: bool,
    header_size: u64,
    tail_object_offset: u64,
    file_size: u64,
    head_entry_seqnum: u64,
    head_entry_realtime: u64,
    n_objects_header: u64,
    n_entries_header: u64,
    n_tags_header: u64,
    n_objects: u64,
    n_entries: u64,
    n_tags: u64,
    last_tag_end: u64,
    last_epoch: u64,
    last_tag_realtime: u64,
    entry_seqnum: u64,
    entry_seqnum_set: bool,
    entry_monotonic: u64,
    entry_monotonic_set: bool,
    entry_boot_id: [u8; 16],
    entry_realtime: u64,
    entry_realtime_set: bool,
    max_entry_realtime: u64,
    min_entry_realtime: u64,
}

fn verify_sealed(
    source: &dyn ByteSource,
    compatible_flags: u32,
    incompatible_flags: u32,
    seed: [u8; 12],
    start_epoch: u64,
    interval_usec: u64,
) -> Result<()> {
    SealedVerifyState::new(
        source,
        compatible_flags,
        incompatible_flags,
        seed,
        start_epoch,
        interval_usec,
    )?
    .run()
}

impl<'a> SealedVerifyState<'a> {
    fn new(
        source: &'a dyn ByteSource,
        compatible_flags: u32,
        incompatible_flags: u32,
        seed: [u8; 12],
        start_epoch: u64,
        interval_usec: u64,
    ) -> Result<Self> {
        let header_size = read_u64_for_verify(source, 88, "header_size")?;
        let file_size = source.len();
        if header_size < HEADER_MIN_SIZE || header_size > file_size {
            return Err(SdkError::VerificationError(format!(
                "invalid header_size {header_size}"
            )));
        }
        let (msk, mpk) = gen_mk(&seed, RECOMMENDED_SECPAR);
        let n_tags_header = if header_size >= 232 && source.len() >= 232 {
            read_u64_for_verify(source, 224, "n_tags")?
        } else {
            0
        };
        Ok(Self {
            source,
            compatible_flags,
            incompatible_flags,
            seed,
            msk,
            state0: gen_state0(&mpk, &seed),
            start_epoch,
            interval_usec,
            is_compact: (incompatible_flags & INCOMPATIBLE_COMPACT) != 0,
            header_size,
            tail_object_offset: read_u64_for_verify(source, 136, "tail_object_offset")?,
            file_size,
            head_entry_seqnum: read_u64_for_verify(source, 168, "head_entry_seqnum")?,
            head_entry_realtime: read_u64_for_verify(source, 184, "head_entry_realtime")?,
            n_objects_header: read_u64_for_verify(source, 144, "n_objects")?,
            n_entries_header: read_u64_for_verify(source, 152, "n_entries")?,
            n_tags_header,
            n_objects: 0,
            n_entries: 0,
            n_tags: 0,
            last_tag_end: 0,
            last_epoch: 0,
            last_tag_realtime: 0,
            entry_seqnum: 0,
            entry_seqnum_set: false,
            entry_monotonic: 0,
            entry_monotonic_set: false,
            entry_boot_id: [0; 16],
            entry_realtime: 0,
            entry_realtime_set: false,
            max_entry_realtime: 0,
            min_entry_realtime: u64::MAX,
        })
    }

    fn run(mut self) -> Result<()> {
        let mut offset = self.header_size;
        while self.tail_object_offset != 0 {
            let obj = self.read_object(offset)?;
            self.verify_object(obj)?;
            if offset == self.tail_object_offset {
                break;
            }
            offset += obj.aligned_size;
        }
        self.verify_final_counts()
    }

    fn read_object(&self, offset: u64) -> Result<SealedVerifyObject> {
        if offset > self.tail_object_offset {
            return Err(SdkError::VerificationError(format!(
                "object offset {offset} exceeds tail_object_offset {}",
                self.tail_object_offset
            )));
        }
        if offset > self.file_size - OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "object header at offset {offset} exceeds file bounds"
            )));
        }
        let header = read_verify_bytes(self.source, offset, OBJECT_HEADER_SIZE, "object header")?;
        let obj = SealedVerifyObject {
            offset,
            typ: header[0],
            flags: header[1],
            size: u64::from_le_bytes(header[8..16].try_into().map_err(|_| {
                SdkError::VerificationError("object size has invalid length".into())
            })?),
            aligned_size: 0,
        };
        let obj = SealedVerifyObject {
            aligned_size: align8(obj.size),
            ..obj
        };
        self.verify_object_envelope(obj)?;
        self.verify_object_flags(obj)?;
        Ok(obj)
    }

    fn verify_object_envelope(&self, obj: SealedVerifyObject) -> Result<()> {
        if obj.size < OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "object size {} too small at offset {}",
                obj.size, obj.offset
            )));
        }
        if obj.aligned_size < obj.size || obj.aligned_size == 0 {
            return Err(SdkError::VerificationError(format!(
                "object size {} overflows alignment at offset {}",
                obj.size, obj.offset
            )));
        }
        if obj.aligned_size > self.file_size - obj.offset {
            return Err(SdkError::VerificationError(format!(
                "object at offset {} with aligned size {} exceeds file bounds",
                obj.offset, obj.aligned_size
            )));
        }
        Ok(())
    }

    fn verify_object_flags(&self, obj: SealedVerifyObject) -> Result<()> {
        if object_compression_flag_count(obj.flags) > 1 {
            return Err(SdkError::VerificationError(format!(
                "multiple compression flags at offset {}",
                obj.offset
            )));
        }
        self.verify_enabled_compression_flag(obj)?;
        if obj.flags & !(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD) != 0
        {
            return Err(SdkError::VerificationError(format!(
                "unknown object flags 0x{:02x} at offset {}",
                obj.flags, obj.offset
            )));
        }
        if obj.typ != OBJECT_TYPE_DATA && obj.flags != 0 {
            return Err(SdkError::VerificationError(format!(
                "object type {} at offset {} has compression flags",
                obj.typ, obj.offset
            )));
        }
        Ok(())
    }

    fn verify_enabled_compression_flag(&self, obj: SealedVerifyObject) -> Result<()> {
        if obj.flags & OBJECT_COMPRESSED_XZ != 0
            && self.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ == 0
        {
            return Err(SdkError::VerificationError(format!(
                "XZ object in file without XZ support at offset {}",
                obj.offset
            )));
        }
        if obj.flags & OBJECT_COMPRESSED_LZ4 != 0
            && self.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4 == 0
        {
            return Err(SdkError::VerificationError(format!(
                "LZ4 object in file without LZ4 support at offset {}",
                obj.offset
            )));
        }
        if obj.flags & OBJECT_COMPRESSED_ZSTD != 0
            && self.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD == 0
        {
            return Err(SdkError::VerificationError(format!(
                "ZSTD object in file without ZSTD support at offset {}",
                obj.offset
            )));
        }
        Ok(())
    }

    fn verify_object(&mut self, obj: SealedVerifyObject) -> Result<()> {
        self.n_objects += 1;
        match obj.typ {
            OBJECT_TYPE_DATA
            | OBJECT_TYPE_FIELD
            | OBJECT_TYPE_DATA_HASH_TABLE
            | OBJECT_TYPE_FIELD_HASH_TABLE
            | OBJECT_TYPE_ENTRY_ARRAY => Ok(()),
            OBJECT_TYPE_ENTRY => self.verify_entry_object(obj),
            OBJECT_TYPE_TAG => self.verify_tag_object(obj),
            _ => Err(SdkError::VerificationError(format!(
                "unknown object type {} at offset {}",
                obj.typ, obj.offset
            ))),
        }
    }

    fn verify_entry_object(&mut self, obj: SealedVerifyObject) -> Result<()> {
        if self.n_tags == 0 {
            return Err(SdkError::VerificationError(format!(
                "first entry before first tag at offset {}",
                obj.offset
            )));
        }
        let entry = self.read_entry(obj.offset)?;
        self.verify_entry_realtime_floor(obj, entry)?;
        self.verify_entry_seqnum(obj, entry.seqnum)?;
        self.verify_entry_monotonic(obj, entry)?;
        self.verify_entry_realtime_head(obj, entry.realtime)?;
        self.record_entry_realtime(entry.realtime);
        self.n_entries += 1;
        Ok(())
    }

    fn read_entry(&self, offset: u64) -> Result<SealedVerifyEntry> {
        let mut boot_id = [0u8; 16];
        let boot_id_bytes = read_verify_bytes(self.source, offset + 40, 16, "entry boot_id")?;
        boot_id.copy_from_slice(&boot_id_bytes);
        Ok(SealedVerifyEntry {
            seqnum: read_u64_for_verify(self.source, offset + 16, "entry seqnum")?,
            realtime: read_u64_for_verify(self.source, offset + 24, "entry realtime")?,
            monotonic: read_u64_for_verify(self.source, offset + 32, "entry monotonic")?,
            boot_id,
        })
    }

    fn verify_entry_realtime_floor(
        &self,
        obj: SealedVerifyObject,
        entry: SealedVerifyEntry,
    ) -> Result<()> {
        if self.entry_realtime_set && entry.realtime < self.last_tag_realtime {
            return Err(SdkError::VerificationError(format!(
                "older entry after newer tag at offset {}",
                obj.offset
            )));
        }
        Ok(())
    }

    fn verify_entry_seqnum(&mut self, obj: SealedVerifyObject, seqnum: u64) -> Result<()> {
        if !self.entry_seqnum_set && seqnum != self.head_entry_seqnum {
            return Err(SdkError::VerificationError(format!(
                "head entry seqnum mismatch at offset {}",
                obj.offset
            )));
        }
        if self.entry_seqnum_set && self.entry_seqnum >= seqnum {
            return Err(SdkError::VerificationError(format!(
                "entry seqnum out of sync at offset {}",
                obj.offset
            )));
        }
        self.entry_seqnum = seqnum;
        self.entry_seqnum_set = true;
        Ok(())
    }

    fn verify_entry_monotonic(
        &mut self,
        obj: SealedVerifyObject,
        entry: SealedVerifyEntry,
    ) -> Result<()> {
        if self.entry_monotonic_set
            && entry.boot_id == self.entry_boot_id
            && self.entry_monotonic > entry.monotonic
        {
            return Err(SdkError::VerificationError(format!(
                "entry monotonic out of sync at offset {}",
                obj.offset
            )));
        }
        self.entry_monotonic = entry.monotonic;
        self.entry_boot_id = entry.boot_id;
        self.entry_monotonic_set = true;
        Ok(())
    }

    fn verify_entry_realtime_head(&mut self, obj: SealedVerifyObject, realtime: u64) -> Result<()> {
        if !self.entry_realtime_set && realtime != self.head_entry_realtime {
            return Err(SdkError::VerificationError(format!(
                "head entry realtime mismatch at offset {}",
                obj.offset
            )));
        }
        self.entry_realtime = realtime;
        self.entry_realtime_set = true;
        Ok(())
    }

    fn record_entry_realtime(&mut self, realtime: u64) {
        self.max_entry_realtime = self.max_entry_realtime.max(realtime);
        self.min_entry_realtime = self.min_entry_realtime.min(realtime);
    }

    fn verify_tag_object(&mut self, obj: SealedVerifyObject) -> Result<()> {
        if obj.size != OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH as u64 {
            return Err(SdkError::VerificationError(format!(
                "invalid tag object size {} at offset {}",
                obj.size, obj.offset
            )));
        }
        let seqnum = read_u64_for_verify(self.source, obj.offset + 16, "tag seqnum")?;
        let epoch = read_u64_for_verify(self.source, obj.offset + 24, "tag epoch")?;
        self.verify_tag_seqnum(obj, seqnum)?;
        self.verify_tag_epoch(obj, epoch)?;
        let rt = self.verify_tag_realtime_window(obj, epoch)?;
        self.verify_tag_hmac(obj, epoch)?;
        self.record_tag(obj, epoch, rt);
        Ok(())
    }

    fn verify_tag_seqnum(&self, obj: SealedVerifyObject, seqnum: u64) -> Result<()> {
        if seqnum != self.n_tags + 1 {
            return Err(SdkError::VerificationError(format!(
                "tag seqnum mismatch: got {seqnum}, want {} at offset {}",
                self.n_tags + 1,
                obj.offset
            )));
        }
        Ok(())
    }

    fn verify_tag_epoch(&self, obj: SealedVerifyObject, epoch: u64) -> Result<()> {
        if self.compatible_flags & COMPATIBLE_SEALED_CONTINUOUS != 0 {
            return self.verify_continuous_tag_epoch(obj, epoch);
        }
        if epoch < self.last_epoch {
            return Err(SdkError::VerificationError(format!(
                "epoch out of sync: got {epoch}, last {} at offset {}",
                self.last_epoch, obj.offset
            )));
        }
        Ok(())
    }

    fn verify_continuous_tag_epoch(&self, obj: SealedVerifyObject, epoch: u64) -> Result<()> {
        let ok = self.n_tags == 0
            || (self.n_tags == 1 && epoch == self.last_epoch)
            || epoch == self.last_epoch + 1;
        if !ok {
            return Err(SdkError::VerificationError(format!(
                "epoch not continuous: got {epoch}, last {} at offset {}",
                self.last_epoch, obj.offset
            )));
        }
        Ok(())
    }

    fn verify_tag_realtime_window(&self, obj: SealedVerifyObject, epoch: u64) -> Result<u64> {
        let (rt, rt_end) = tag_realtime_range(self.start_epoch, epoch, self.interval_usec)?;
        if self.entry_realtime_set && self.entry_realtime >= rt_end {
            return Err(SdkError::VerificationError(format!(
                "entry realtime {} too late for tag end {rt_end} at offset {}",
                self.entry_realtime, obj.offset
            )));
        }
        if self.max_entry_realtime >= rt_end {
            return Err(SdkError::VerificationError(format!(
                "max entry realtime {} too late for tag end {rt_end} at offset {}",
                self.max_entry_realtime, obj.offset
            )));
        }
        if self.min_entry_realtime < rt {
            return Err(SdkError::VerificationError(format!(
                "entry realtime {} too early for tag start {rt} at offset {}",
                self.min_entry_realtime, obj.offset
            )));
        }
        Ok(rt)
    }

    fn verify_tag_hmac(&self, obj: SealedVerifyObject, epoch: u64) -> Result<()> {
        let mut hm = self.new_tag_hmac(epoch);
        if self.n_tags == 0 {
            self.write_first_tag_header_hmac(&mut hm)?;
        }
        self.write_tag_object_hmacs(&mut hm, obj.offset)?;
        let stored = read_verify_bytes(self.source, obj.offset + 32, TAG_LENGTH as u64, "tag")?;
        if hm.verify_slice(&stored).is_err() {
            return Err(SdkError::VerificationError(format!(
                "tag failed verification at offset {}",
                obj.offset
            )));
        }
        Ok(())
    }

    fn new_tag_hmac(&self, epoch: u64) -> Hmac<Sha256> {
        let state = seek(&self.state0, epoch, &self.msk, &self.seed);
        let key = get_key(&state, TAG_LENGTH, 0);
        Hmac::<Sha256>::new_from_slice(&key).expect("HMAC key length valid")
    }

    fn write_first_tag_header_hmac(&self, hm: &mut Hmac<Sha256>) -> Result<()> {
        update_hmac_range(hm, self.source, 0, 16)?;
        update_hmac_range(hm, self.source, 24, 32)?;
        update_hmac_range(hm, self.source, 72, 24)?;
        update_hmac_range(hm, self.source, 104, 32)?;
        Ok(())
    }

    fn write_tag_object_hmacs(&self, hm: &mut Hmac<Sha256>, tag_offset: u64) -> Result<()> {
        let mut offset = self.last_tag_end;
        if self.n_tags == 0 {
            offset = self.header_size;
        }
        while offset <= tag_offset {
            let obj = self.read_hmac_object(offset)?;
            hmac_object(hm, self.source, offset, obj.typ, obj.size, self.is_compact)?;
            offset += obj.aligned_size;
        }
        Ok(())
    }

    fn read_hmac_object(&self, offset: u64) -> Result<SealedVerifyObject> {
        if offset > self.file_size - OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "HMAC object header at offset {offset} exceeds file bounds"
            )));
        }
        let size = read_u64_for_verify(self.source, offset + 8, "HMAC object size")?;
        let aligned_size = align8(size);
        if size < OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "HMAC object size {size} too small at offset {offset}"
            )));
        }
        if aligned_size < size || aligned_size == 0 {
            return Err(SdkError::VerificationError(format!(
                "HMAC object size {size} overflows alignment at offset {offset}"
            )));
        }
        if aligned_size > self.file_size - offset {
            return Err(SdkError::VerificationError(format!(
                "HMAC object at offset {offset} with aligned size {aligned_size} exceeds file bounds"
            )));
        }
        let typ = read_verify_bytes(self.source, offset, 1, "HMAC object type")?[0];
        Ok(SealedVerifyObject {
            offset,
            typ,
            flags: 0,
            size,
            aligned_size,
        })
    }

    fn record_tag(&mut self, obj: SealedVerifyObject, epoch: u64, realtime: u64) {
        self.n_tags += 1;
        self.last_tag_end = obj.offset + obj.aligned_size;
        self.last_epoch = epoch;
        self.last_tag_realtime = realtime;
        self.min_entry_realtime = u64::MAX;
    }

    fn verify_final_counts(&self) -> Result<()> {
        if self.n_objects != self.n_objects_header {
            return Err(SdkError::VerificationError(format!(
                "object count mismatch: got {}, want {}",
                self.n_objects, self.n_objects_header
            )));
        }
        if self.n_entries != self.n_entries_header {
            return Err(SdkError::VerificationError(format!(
                "entry count mismatch: got {}, want {}",
                self.n_entries, self.n_entries_header
            )));
        }
        if self.n_tags != self.n_tags_header {
            return Err(SdkError::VerificationError(format!(
                "tag count mismatch: got {}, want {}",
                self.n_tags, self.n_tags_header
            )));
        }
        Ok(())
    }
}

fn object_compression_flag_count(flags: u8) -> u32 {
    [
        OBJECT_COMPRESSED_XZ,
        OBJECT_COMPRESSED_LZ4,
        OBJECT_COMPRESSED_ZSTD,
    ]
    .iter()
    .filter(|flag| flags & **flag != 0)
    .count() as u32
}

fn tag_realtime_range(start_epoch: u64, epoch: u64, interval_usec: u64) -> Result<(u64, u64)> {
    let absolute_epoch = start_epoch
        .checked_add(epoch)
        .ok_or_else(|| SdkError::VerificationError("tag realtime overflow".into()))?;
    let rt = absolute_epoch
        .checked_mul(interval_usec)
        .ok_or_else(|| SdkError::VerificationError("tag realtime overflow".into()))?;
    let rt_end = rt
        .checked_add(interval_usec)
        .ok_or_else(|| SdkError::VerificationError("tag realtime overflow".into()))?;
    Ok((rt, rt_end))
}

fn hmac_object(
    hm: &mut impl hmac::Mac,
    source: &dyn ByteSource,
    offset: u64,
    typ: u8,
    size: u64,
    is_compact: bool,
) -> Result<()> {
    update_hmac_range(hm, source, offset, OBJECT_HEADER_SIZE)?;

    match typ {
        OBJECT_TYPE_DATA => {
            update_hmac_range(hm, source, offset + 16, 8)?;
            let payload_offset = if is_compact {
                COMPACT_DATA_OBJECT_HEADER_SIZE
            } else {
                DATA_OBJECT_HEADER_SIZE
            };
            if size > payload_offset {
                update_hmac_range(hm, source, offset + payload_offset, size - payload_offset)?;
            }
        }
        OBJECT_TYPE_FIELD => {
            update_hmac_range(hm, source, offset + 16, 8)?;
            if size > FIELD_OBJECT_HEADER_SIZE {
                update_hmac_range(
                    hm,
                    source,
                    offset + FIELD_OBJECT_HEADER_SIZE,
                    size - FIELD_OBJECT_HEADER_SIZE,
                )?;
            }
        }
        OBJECT_TYPE_ENTRY => {
            if size > OBJECT_HEADER_SIZE {
                update_hmac_range(
                    hm,
                    source,
                    offset + OBJECT_HEADER_SIZE,
                    size - OBJECT_HEADER_SIZE,
                )?;
            }
        }
        OBJECT_TYPE_DATA_HASH_TABLE | OBJECT_TYPE_FIELD_HASH_TABLE | OBJECT_TYPE_ENTRY_ARRAY => {}
        OBJECT_TYPE_TAG => {
            update_hmac_range(hm, source, offset + OBJECT_HEADER_SIZE, 16)?;
        }
        _ => {}
    }
    Ok(())
}

fn update_hmac_range(
    hm: &mut impl hmac::Mac,
    source: &dyn ByteSource,
    offset: u64,
    size: u64,
) -> Result<()> {
    const HMAC_CHUNK_SIZE: u64 = 1 << 20;
    let mut current = offset;
    let end = offset
        .checked_add(size)
        .ok_or_else(|| SdkError::VerificationError("HMAC range overflow".into()))?;
    while current < end {
        let chunk = (end - current).min(HMAC_CHUNK_SIZE);
        let bytes = read_verify_bytes(source, current, chunk, "HMAC range")?;
        hm.update(&bytes);
        current += chunk;
    }
    Ok(())
}
