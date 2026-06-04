use crate::error::{JournalError, Result};
use std::io::Read;

// systemd limits journal DATA field payloads to 768 MiB; reject corrupt size prefixes before allocating.
pub(super) const MAX_UNCOMPRESSED_DATA_OBJECT_SIZE: usize = 768 * 1024 * 1024;
const DECOMPRESSION_READ_CHUNK_SIZE: usize = 8 * 1024;
const MIN_DECOMPRESSION_RESERVE_SIZE: usize = 64 * 1024;

fn read_limited_to_end<R: Read>(reader: R, buf: &mut Vec<u8>) -> Result<usize> {
    read_limited_to_end_with_cap(reader, buf, MAX_UNCOMPRESSED_DATA_OBJECT_SIZE)
}

pub(super) fn read_limited_to_end_with_cap<R: Read>(
    mut reader: R,
    buf: &mut Vec<u8>,
    max_size: usize,
) -> Result<usize> {
    buf.clear();
    let mut chunk = [0u8; DECOMPRESSION_READ_CHUNK_SIZE];

    loop {
        if buf.len() == max_size {
            let mut extra = [0u8; 1];
            match reader.read(&mut extra) {
                Ok(0) => return Ok(buf.len()),
                Ok(_) => {
                    *buf = Vec::new();
                    return Err(JournalError::DecompressorError);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    *buf = Vec::new();
                    return Err(JournalError::DecompressorError);
                }
            }
        }

        let remaining = max_size - buf.len();
        let read_len = remaining.min(chunk.len());
        match reader.read(&mut chunk[..read_len]) {
            Ok(0) => return Ok(buf.len()),
            Ok(len) => extend_decompression_buffer(buf, &chunk[..len], max_size)?,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                *buf = Vec::new();
                return Err(JournalError::DecompressorError);
            }
        }
    }
}

fn extend_decompression_buffer(buf: &mut Vec<u8>, data: &[u8], max_size: usize) -> Result<()> {
    let Some(required) = buf
        .len()
        .checked_add(data.len())
        .filter(|required| *required <= max_size)
    else {
        *buf = Vec::new();
        return Err(JournalError::DecompressorError);
    };

    if required > buf.capacity() {
        let target_capacity = required
            .max(buf.capacity().saturating_mul(2))
            .max(MIN_DECOMPRESSION_RESERVE_SIZE)
            .min(max_size);

        if buf.try_reserve_exact(target_capacity - buf.len()).is_err() {
            *buf = Vec::new();
            return Err(JournalError::DecompressorError);
        }
    }

    buf.extend_from_slice(data);
    Ok(())
}

fn clear_decompression_error<T>(buf: &mut Vec<u8>) -> Result<T> {
    *buf = Vec::new();
    Err(JournalError::DecompressorError)
}

pub(super) fn clear_compression_error<T>(buf: &mut Vec<u8>, err: JournalError) -> Result<T> {
    *buf = Vec::new();
    Err(err)
}

fn lz4_uncompressed_size(payload: &[u8]) -> Result<usize> {
    if payload.len() < 8 {
        return Err(JournalError::DecompressorError);
    }
    let size_bytes: [u8; 8] = payload[..8]
        .try_into()
        .map_err(|_| JournalError::DecompressorError)?;
    let uncompressed_size = usize::try_from(u64::from_le_bytes(size_bytes))
        .map_err(|_| JournalError::DecompressorError)?;
    if uncompressed_size > MAX_UNCOMPRESSED_DATA_OBJECT_SIZE {
        return Err(JournalError::DecompressorError);
    }
    Ok(uncompressed_size)
}

fn prepare_lz4_output_buffer(buf: &mut Vec<u8>, uncompressed_size: usize) -> Result<()> {
    buf.clear();
    if uncompressed_size > buf.capacity() && buf.try_reserve_exact(uncompressed_size).is_err() {
        return Err(JournalError::DecompressorError);
    }
    buf.resize(uncompressed_size, 0);
    Ok(())
}

fn zstd_uncompressed_size(payload: &[u8]) -> Result<usize> {
    let Some(size) = zstd::zstd_safe::get_frame_content_size(payload)
        .map_err(|_| JournalError::DecompressorError)?
    else {
        return Err(JournalError::DecompressorError);
    };

    let uncompressed_size = usize::try_from(size).map_err(|_| JournalError::DecompressorError)?;
    if uncompressed_size > MAX_UNCOMPRESSED_DATA_OBJECT_SIZE {
        return Err(JournalError::DecompressorError);
    }
    Ok(uncompressed_size)
}

fn prepare_zstd_output_buffer(buf: &mut Vec<u8>, uncompressed_size: usize) -> Result<()> {
    buf.clear();
    if uncompressed_size > buf.capacity() && buf.try_reserve_exact(uncompressed_size).is_err() {
        return Err(JournalError::DecompressorError);
    }
    Ok(())
}

pub(super) fn decompress_zstd_payload(payload: &[u8], buf: &mut Vec<u8>) -> Result<usize> {
    if let Ok(len) = decompress_zstd_payload_native(payload, buf) {
        return Ok(len);
    }

    decompress_zstd_payload_streaming(payload, buf)
}

fn decompress_zstd_payload_native(payload: &[u8], buf: &mut Vec<u8>) -> Result<usize> {
    let uncompressed_size = match zstd_uncompressed_size(payload) {
        Ok(size) => size,
        Err(_) => return clear_decompression_error(buf),
    };
    if prepare_zstd_output_buffer(buf, uncompressed_size).is_err() {
        return clear_decompression_error(buf);
    }

    match zstd::zstd_safe::decompress(buf, payload) {
        Ok(len) if len == uncompressed_size => Ok(len),
        Ok(_) | Err(_) => clear_decompression_error(buf),
    }
}

fn decompress_zstd_payload_streaming(payload: &[u8], buf: &mut Vec<u8>) -> Result<usize> {
    use ruzstd::decoding::StreamingDecoder;

    let decoder = StreamingDecoder::new(payload).map_err(|_| JournalError::DecompressorError)?;
    read_limited_to_end(decoder, buf)
}

pub(super) fn decompress_lz4_payload(payload: &[u8], buf: &mut Vec<u8>) -> Result<usize> {
    let uncompressed_size = match lz4_uncompressed_size(payload) {
        Ok(size) => size,
        Err(_) => return clear_decompression_error(buf),
    };
    if prepare_lz4_output_buffer(buf, uncompressed_size).is_err() {
        return clear_decompression_error(buf);
    }

    let compressed_data = &payload[8..];
    match lz4_flex::block::decompress_into(compressed_data, buf) {
        Ok(len) if len == uncompressed_size => Ok(len),
        Ok(_) | Err(_) => clear_decompression_error(buf),
    }
}

pub(super) fn decompress_xz_payload(payload: &[u8], buf: &mut Vec<u8>) -> Result<usize> {
    use lzma_rust2::XzReader;

    let decoder = XzReader::new(payload, false);
    read_limited_to_end(decoder, buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn native_zstd_decompresses_into_reserved_vec() {
        let payload = b"FIELD=native zstd payload ".repeat(128);
        let compressed = zstd::bulk::compress(&payload, 1).expect("compress zstd payload");
        let mut out = Vec::new();

        let len =
            decompress_zstd_payload_native(&compressed, &mut out).expect("native zstd decode");

        assert_eq!(len, payload.len());
        assert_eq!(&out[..len], payload.as_slice());
    }

    #[test]
    fn zstd_decompressor_keeps_ruzstd_fallback() {
        let payload = b"FIELD=ruzstd compatibility payload ".repeat(128);
        let compressed = ruzstd::encoding::compress_to_vec(
            Cursor::new(payload.as_slice()),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        let mut out = Vec::new();

        let len = decompress_zstd_payload(&compressed, &mut out).expect("fallback zstd decode");

        assert_eq!(len, payload.len());
        assert_eq!(&out[..len], payload.as_slice());
    }
}
