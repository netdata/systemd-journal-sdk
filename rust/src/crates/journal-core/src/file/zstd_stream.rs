use ruzstd::decoding::errors::FrameDecoderError;
use ruzstd::decoding::{FrameDecoder, StreamingDecoder};
use std::error::Error;
use std::io::{self, BufReader, Read, Write};

fn decoder_error(error: FrameDecoderError) -> io::Error {
    // Decoder wrappers retain transport errors in their source chain.
    let mut source: Option<&(dyn Error + 'static)> = Some(&error);
    let mut kind = io::ErrorKind::InvalidData;
    while let Some(cause) = source {
        if let Some(io_error) = cause.downcast_ref::<io::Error>() {
            kind = if io_error.kind() == io::ErrorKind::UnexpectedEof {
                io::ErrorKind::InvalidData
            } else {
                io_error.kind()
            };
        }
        source = cause.source();
    }
    io::Error::new(kind, error)
}

fn copy_error(error: io::Error) -> io::Error {
    if error
        .get_ref()
        .is_some_and(|cause| cause.is::<FrameDecoderError>())
    {
        let inner = error
            .into_inner()
            .unwrap()
            .downcast::<FrameDecoderError>()
            .unwrap();
        decoder_error(*inner)
    } else {
        error
    }
}

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
            StreamingDecoder::new_with_decoder(source, &mut frame).map_err(decoder_error)?;
        let expected = decoder.decoder.content_size();
        let count = io::copy(&mut decoder, output).map_err(copy_error)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingRead<'a> {
        bytes: &'a [u8],
        left: usize,
    }
    impl Read for FailingRead<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.left == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "source unavailable",
                ));
            }
            let count = out.len().min(self.left).min(self.bytes.len());
            out[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes = &self.bytes[count..];
            self.left -= count;
            Ok(count)
        }
    }
    struct FailingWrite;
    impl Write for FailingWrite {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "destination full",
            ))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn stream_preserves_io_failures() {
        let plain = b"stream failure classification".repeat(64);
        let compressed = ruzstd::encoding::compress_to_vec(
            plain.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        for left in [0, 2, 5, 6, compressed.len() - 1] {
            let source = FailingRead {
                bytes: &compressed,
                left,
            };
            let error = copy_zstd_stream(source, &mut Vec::new()).unwrap_err();
            assert_eq!(
                error.kind(),
                io::ErrorKind::PermissionDenied,
                "offset {left}: {error}"
            );
        }
        assert_eq!(
            copy_zstd_stream(compressed.as_slice(), &mut FailingWrite)
                .unwrap_err()
                .kind(),
            io::ErrorKind::StorageFull
        );
        for end in [0, 2, 5, compressed.len() - 1] {
            let error = copy_zstd_stream(&compressed[..end], &mut Vec::new()).unwrap_err();
            assert!(
                matches!(
                    error.kind(),
                    io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof
                ),
                "offset {end}: {error}"
            );
        }
        let mut out = Vec::new();
        copy_zstd_stream(compressed.as_slice(), &mut out).unwrap();
        assert_eq!(out, plain);
    }
}
