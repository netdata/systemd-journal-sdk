use ruzstd::decoding::{FrameDecoder, StreamingDecoder};
use std::io::{self, BufReader, Read, Write};

fn invalid_stream() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid Zstandard stream")
}

/// Decode all frames without buffering the whole input or output.
#[doc(hidden)]
pub fn copy_zstd_stream(input: impl Read, output: &mut impl Write) -> io::Result<u64> {
    let mut input = BufReader::new(input);
    let mut frame = FrameDecoder::new();
    let mut total = 0u64;
    let mut seen_frame = false;
    loop {
        let mut prefix = [0u8; 5];
        match input.read(&mut prefix[..1]) {
            Ok(0) if seen_frame => return Ok(total),
            Ok(0) => return Err(invalid_stream()),
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
        input.read_exact(&mut prefix[1..4])?;
        let magic = u32::from_le_bytes(prefix[..4].try_into().unwrap());
        if magic & 0xfffffff0 == 0x184d2a50 {
            let mut size = [0u8; 4];
            input.read_exact(&mut size)?;
            let size = u64::from(u32::from_le_bytes(size));
            if io::copy(&mut (&mut input).take(size), &mut io::sink())? != size {
                return Err(invalid_stream());
            }
            seen_frame = true;
            continue;
        }
        if magic != 0xfd2fb528 {
            return Err(invalid_stream());
        }
        input.read_exact(&mut prefix[4..])?;
        let has_size = prefix[4] & 0xe0 != 0;
        let source = prefix.as_slice().chain(&mut input);
        let mut decoder =
            StreamingDecoder::new_with_decoder(source, &mut frame).map_err(|_| invalid_stream())?;
        let expected = decoder.decoder.content_size();
        let count = io::copy(&mut decoder, output)?;
        if (has_size && count != expected)
            || decoder
                .decoder
                .get_checksum_from_data()
                .is_some_and(|checksum| Some(checksum) != decoder.decoder.get_calculated_checksum())
        {
            return Err(invalid_stream());
        }
        total = total.checked_add(count).ok_or_else(invalid_stream)?;
        seen_frame = true;
    }
}
