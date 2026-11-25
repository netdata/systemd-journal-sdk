use super::*;
use std::io::Read;

pub(crate) trait VerifyByteSource {
    fn len(&self) -> u64;
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
    fn read_vec(&self, offset: u64, size: u64) -> Result<Vec<u8>, String>;
}

pub(super) fn decompress_payload(flags: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    if flags & OBJECT_COMPRESSED_ZSTD != 0 {
        let mut out = Vec::new();
        journal_core::file::decompress_zstd_data(payload, &mut out)
            .map_err(|err| err.to_string())?;
        return Ok(out);
    }
    if flags & OBJECT_COMPRESSED_XZ != 0 {
        let mut decoder = lzma_rust2::XzReader::new(payload, false);
        return read_limited_to_end(&mut decoder);
    }
    if flags & OBJECT_COMPRESSED_LZ4 != 0 {
        if payload.len() < 8 {
            return Err("lz4 compressed payload too short".to_string());
        }
        let expected = usize::try_from(u64::from_le_bytes(
            payload[0..8]
                .try_into()
                .map_err(|_| "bad lz4 size prefix")?,
        ))
        .map_err(|_| "lz4 decompressed payload too large".to_string())?;
        if expected > MAX_UNCOMPRESSED_DATA_OBJECT_SIZE {
            return Err("lz4 decompressed payload too large".to_string());
        }
        // No lz4 block decodes to more than 255 times its size (see
        // journal-core `LZ4_MAX_EXPANSION`); reject a forged prefix unallocated.
        if expected > (payload.len() - 8).saturating_mul(255) {
            return Err("lz4 size prefix exceeds the maximum expansion".to_string());
        }
        let mut out = vec![0; expected];
        let len = lz4_flex::block::decompress_into(&payload[8..], &mut out)
            .map_err(|err| err.to_string())?;
        if len != expected {
            return Err("lz4 decompressed size mismatch".to_string());
        }
        return Ok(out);
    }
    Ok(payload.to_vec())
}

pub(super) fn read_limited_to_end<R: Read>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        if out.len() == MAX_UNCOMPRESSED_DATA_OBJECT_SIZE {
            let mut extra = [0u8; 1];
            match reader.read(&mut extra) {
                Ok(0) => return Ok(out),
                Ok(_) => return Err("decompressed payload too large".to_string()),
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err.to_string()),
            }
        }
        let remaining = MAX_UNCOMPRESSED_DATA_OBJECT_SIZE - out.len();
        let read_len = remaining.min(buf.len());
        match reader.read(&mut buf[..read_len]) {
            Ok(0) => return Ok(out),
            Ok(len) => out.extend_from_slice(&buf[..len]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.to_string()),
        }
    }
}

pub(super) fn header_contains_field(
    source: &dyn VerifyByteSource,
    header_size: u64,
    end: usize,
) -> bool {
    header_size >= end as u64 && source.len() >= end as u64
}

pub(super) fn align8_checked(value: u64) -> Option<u64> {
    value.checked_add(7).map(|v| v & !7)
}

pub(super) fn byte_at(source: &dyn VerifyByteSource, offset: u64) -> Result<u8, String> {
    let bytes = source.read_vec(offset, 1)?;
    Ok(bytes[0])
}

pub(super) fn u32_at(source: &dyn VerifyByteSource, offset: usize) -> Result<u32, String> {
    let bytes = source.read_vec(offset as u64, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub(super) fn u32_at_u64(source: &dyn VerifyByteSource, offset: u64) -> Result<u32, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    u32_at(source, offset)
}

pub(super) fn u64_at(source: &dyn VerifyByteSource, offset: usize) -> Result<u64, String> {
    let bytes = source.read_vec(offset as u64, 8)?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

pub(super) fn u64_at_u64(source: &dyn VerifyByteSource, offset: u64) -> Result<u64, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    u64_at(source, offset)
}

pub(super) fn bytes16_at(source: &dyn VerifyByteSource, offset: usize) -> Result<[u8; 16], String> {
    let bytes = source.read_vec(offset as u64, 16)?;
    Ok(bytes.try_into().unwrap())
}

pub(super) fn bytes16_at_u64(
    source: &dyn VerifyByteSource,
    offset: u64,
) -> Result<[u8; 16], String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    bytes16_at(source, offset)
}

pub(super) fn slice_u64(
    source: &dyn VerifyByteSource,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    let size = end
        .checked_sub(start)
        .ok_or_else(|| format!("slice {start}..{end} is invalid"))?;
    source.read_vec(start, size)
}

#[cfg(test)]
mod lz4_bound_tests {
    use super::*;

    #[test]
    fn lz4_prefix_beyond_maximum_expansion_is_rejected() {
        let compressed = [0x10, 0x61, 0, 0, 0, 0, 0];
        let mut payload = ((compressed.len() * 255 + 1) as u64).to_le_bytes().to_vec();
        payload.extend_from_slice(&compressed);
        let err = decompress_payload(OBJECT_COMPRESSED_LZ4, &payload).unwrap_err();
        assert!(err.contains("maximum expansion"), "{err}");
    }

    #[test]
    fn lz4_maximum_expansion_still_decodes() {
        let original = vec![0u8; 1 << 20];
        let mut payload = (original.len() as u64).to_le_bytes().to_vec();
        payload.extend_from_slice(&lz4_flex::block::compress(&original));
        assert_eq!(
            decompress_payload(OBJECT_COMPRESSED_LZ4, &payload).unwrap(),
            original
        );
    }
}
