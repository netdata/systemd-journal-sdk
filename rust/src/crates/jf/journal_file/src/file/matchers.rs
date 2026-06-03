use super::*;
use std::marker::PhantomData;
use zerocopy::ByteSlice;

pub trait BucketVisitor<'a> {
    type Object: JournalObject<&'a [u8]> + HashableObject;
    type Output;

    /// Called for each object in the bucket. Return Some(output) to stop iteration,
    /// or None to continue to the next object.
    fn visit(&mut self, object: &ValueGuard<'a, Self::Object>) -> Result<Option<Self::Output>>;
}

pub(super) struct PayloadMatcher<'data, T> {
    payload: &'data [u8],
    hash: u64,
    _phantom: PhantomData<T>,
}

pub(super) struct DataPayloadMatcher<'data> {
    payload: &'data [u8],
    hash: u64,
    decompressed_payload: Vec<u8>,
}

impl<'data> DataPayloadMatcher<'data> {
    pub(super) fn new(payload: &'data [u8], hash: u64) -> Self {
        Self {
            payload,
            hash,
            decompressed_payload: Vec::new(),
        }
    }

    pub(super) fn payload_matches<B: ByteSlice>(&mut self, object: &DataObject<B>) -> Result<bool> {
        if object.get_payload() == self.payload {
            return Ok(true);
        }

        if object.is_compressed() {
            let len = match object.decompress(&mut self.decompressed_payload) {
                Ok(len) => len,
                Err(JournalError::DecompressorError | JournalError::UnknownCompressionMethod) => {
                    return Ok(false);
                }
                Err(e) => return Err(e),
            };
            return Ok(&self.decompressed_payload[..len] == self.payload);
        }

        Ok(false)
    }
}

impl<'data, B: ByteSlice> PayloadMatcher<'data, FieldObject<B>> {
    pub(super) fn field_matcher(payload: &'data [u8], hash: u64) -> Self {
        Self {
            payload,
            hash,
            _phantom: PhantomData::<FieldObject<B>>,
        }
    }
}

impl<'a, 'data> BucketVisitor<'a> for DataPayloadMatcher<'data> {
    type Object = DataObject<&'a [u8]>;
    type Output = NonZeroU64;

    fn visit(&mut self, object: &ValueGuard<'a, Self::Object>) -> Result<Option<Self::Output>> {
        if object.hash() == self.hash && self.payload_matches(object)? {
            Ok(Some(object.offset()))
        } else {
            Ok(None)
        }
    }
}

impl<'a, T> BucketVisitor<'a> for PayloadMatcher<'_, T>
where
    T: JournalObject<&'a [u8]> + HashableObject,
{
    type Object = T;
    type Output = NonZeroU64;

    fn visit(&mut self, object: &ValueGuard<'a, Self::Object>) -> Result<Option<Self::Output>> {
        if object.hash() == self.hash && object.get_payload() == self.payload {
            Ok(Some(object.offset()))
        } else {
            Ok(None)
        }
    }
}
