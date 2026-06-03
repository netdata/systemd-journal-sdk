use super::*;
use std::io::Read;

pub(super) fn decompress_payload(flags: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    if flags & OBJECT_COMPRESSED_ZSTD != 0 {
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(payload).map_err(|err| err.to_string())?;
        return read_limited_to_end(&mut decoder);
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

pub(super) fn header_contains_field(data: &[u8], header_size: u64, end: usize) -> bool {
    header_size >= end as u64 && data.len() >= end
}

pub(super) fn align8_checked(value: u64) -> Option<u64> {
    value.checked_add(7).map(|v| v & !7)
}

pub(super) fn byte_at(data: &[u8], offset: u64) -> Result<u8, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    data.get(offset)
        .copied()
        .ok_or_else(|| format!("byte read at {offset} exceeds file bounds"))
}

pub(super) fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| format!("uint32 read at {offset} exceeds file bounds"))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub(super) fn u32_at_u64(data: &[u8], offset: u64) -> Result<u32, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    u32_at(data, offset)
}

pub(super) fn u64_at(data: &[u8], offset: usize) -> Result<u64, String> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or_else(|| format!("uint64 read at {offset} exceeds file bounds"))?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

pub(super) fn u64_at_u64(data: &[u8], offset: u64) -> Result<u64, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    u64_at(data, offset)
}

pub(super) fn bytes16_at(data: &[u8], offset: usize) -> Result<[u8; 16], String> {
    let bytes = data
        .get(offset..offset + 16)
        .ok_or_else(|| format!("16-byte read at {offset} exceeds file bounds"))?;
    Ok(bytes.try_into().unwrap())
}

pub(super) fn bytes16_at_u64(data: &[u8], offset: u64) -> Result<[u8; 16], String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    bytes16_at(data, offset)
}

pub(super) fn slice_u64(data: &[u8], start: u64, end: u64) -> Result<&[u8], String> {
    let start =
        usize::try_from(start).map_err(|_| "start offset is not representable".to_string())?;
    let end = usize::try_from(end).map_err(|_| "end offset is not representable".to_string())?;
    data.get(start..end)
        .ok_or_else(|| format!("slice {start}..{end} exceeds file bounds"))
}
