use super::object::*;
use crate::error::Result;
use std::num::NonZeroU64;
use zerocopy::{ByteSlice, ByteSliceMut, Ref, SplitByteSlice, SplitByteSliceMut};

pub trait HashableObject {
    /// Get the hash value of this object
    fn hash(&self) -> u64;

    /// Get the payload data for matching
    fn raw_payload(&self) -> &[u8];

    /// Check if the payload is compressed
    fn is_compressed(&self) -> bool;

    /// Decompress the payload into the provided buffer.
    /// Returns the number of decompressed bytes.
    fn decompress(&self, buf: &mut Vec<u8>) -> Result<usize>;

    /// Get the offset to the next object in the hash chain
    fn next_hash_offset(&self) -> Option<NonZeroU64>;

    /// Get the object type
    fn object_type() -> ObjectType;
}

pub trait HashableObjectMut: HashableObject {
    /// Set the offset to the next object in the hash chain
    fn set_next_hash_offset(&mut self, offset: NonZeroU64);

    /// Set the payload of the object
    fn set_payload(&mut self, data: &[u8]);
}

/// Trait for hash table operations
pub trait HashTable {
    /// The type of objects stored in this hash table
    type Object: HashableObject;

    /// Get the hash item for a given hash value
    fn hash_item_ref(&self, hash: u64) -> &HashItem;

    /// Get the length of the hash table (number of buckets)
    fn len(&self) -> usize;

    /// Make clippy happy
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Trait for mutable hash table operations
pub trait HashTableMut: HashTable {
    /// Get a mutable reference to the hash item for a given hash value
    fn hash_item_mut(&mut self, hash: u64) -> &mut HashItem;
}

pub struct DataHashTable<B: ByteSlice> {
    pub header: Ref<B, ObjectHeader>,
    pub items: Ref<B, [HashItem]>,
}

pub struct FieldHashTable<B: ByteSlice> {
    pub header: Ref<B, ObjectHeader>,
    pub items: Ref<B, [HashItem]>,
}

// Implement HashTable for DataHashTable
impl<B: ByteSlice> HashTable for DataHashTable<B> {
    type Object = DataObject<B>;

    fn hash_item_ref(&self, hash: u64) -> &HashItem {
        let bucket_index = hash as usize % self.items.len();
        &self.items[bucket_index]
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

// Implement HashTable for FieldHashTable
impl<B: ByteSlice> HashTable for FieldHashTable<B> {
    type Object = FieldObject<B>;

    fn hash_item_ref(&self, hash: u64) -> &HashItem {
        let bucket_index = hash as usize % self.items.len();
        &self.items[bucket_index]
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

// Implement HashTableMut for DataHashTable
impl<B: ByteSliceMut> HashTableMut for DataHashTable<B> {
    fn hash_item_mut(&mut self, hash: u64) -> &mut HashItem {
        let bucket_index = hash as usize % self.items.len();
        &mut self.items[bucket_index]
    }
}

// Implement HashTableMut for FieldHashTable
impl<B: ByteSliceMut> HashTableMut for FieldHashTable<B> {
    fn hash_item_mut(&mut self, hash: u64) -> &mut HashItem {
        let bucket_index = hash as usize % self.items.len();
        &mut self.items[bucket_index]
    }
}

// Implement JournalObject for DataHashTable
impl<B: SplitByteSlice> JournalObject<B> for DataHashTable<B> {
    fn from_data(data: B, _is_compact: bool) -> Option<Self> {
        let (header_data, items_data) = data.split_at(std::mem::size_of::<ObjectHeader>()).ok()?;

        let header = zerocopy::Ref::from_bytes(header_data).ok()?;
        let items = zerocopy::Ref::from_bytes(items_data).ok()?;

        Some(DataHashTable { header, items })
    }
}

// Implement JournalObjectMut for DataHashTable
impl<B: SplitByteSliceMut> JournalObjectMut<B> for DataHashTable<B> {
    fn from_data_mut(data: B, _is_compact: bool) -> Option<Self> {
        let (header_data, items_data) = data.split_at(std::mem::size_of::<ObjectHeader>()).ok()?;

        let header = zerocopy::Ref::from_bytes(header_data).ok()?;
        let items = zerocopy::Ref::from_bytes(items_data).ok()?;

        Some(DataHashTable { header, items })
    }
}

// Implement JournalObject for FieldHashTable
impl<B: SplitByteSlice> JournalObject<B> for FieldHashTable<B> {
    fn from_data(data: B, _is_compact: bool) -> Option<Self> {
        let (header_data, items_data) = data.split_at(std::mem::size_of::<ObjectHeader>()).ok()?;

        let header = zerocopy::Ref::from_bytes(header_data).ok()?;
        let items = zerocopy::Ref::from_bytes(items_data).ok()?;

        Some(FieldHashTable { header, items })
    }
}

// Implement JournalObjectMut for FieldHashTable
impl<B: SplitByteSliceMut> JournalObjectMut<B> for FieldHashTable<B> {
    fn from_data_mut(data: B, _is_compact: bool) -> Option<Self> {
        let (header_data, items_data) = data.split_at(std::mem::size_of::<ObjectHeader>()).ok()?;

        let header = zerocopy::Ref::from_bytes(header_data).ok()?;
        let items = zerocopy::Ref::from_bytes(items_data).ok()?;

        Some(FieldHashTable { header, items })
    }
}

impl<B: ByteSlice> HashableObject for FieldObject<B> {
    fn hash(&self) -> u64 {
        self.header.hash
    }

    fn raw_payload(&self) -> &[u8] {
        &self.payload
    }

    fn is_compressed(&self) -> bool {
        false
    }

    fn decompress(&self, buf: &mut Vec<u8>) -> Result<usize> {
        buf.clear();
        buf.extend_from_slice(&self.payload);
        Ok(buf.len())
    }

    fn next_hash_offset(&self) -> Option<NonZeroU64> {
        self.header.next_hash_offset
    }

    fn object_type() -> ObjectType {
        ObjectType::Field
    }
}

impl HashableObjectMut for FieldObject<&mut [u8]> {
    fn set_next_hash_offset(&mut self, next_hash_offset: NonZeroU64) {
        self.header.next_hash_offset = Some(next_hash_offset);
    }

    fn set_payload(&mut self, data: &[u8]) {
        self.payload.copy_from_slice(data);
    }
}

impl<B: ByteSlice> HashableObject for DataObject<B> {
    fn hash(&self) -> u64 {
        self.header.hash
    }

    fn raw_payload(&self) -> &[u8] {
        self.raw_payload()
    }

    fn is_compressed(&self) -> bool {
        DataObject::is_compressed(self)
    }

    fn decompress(&self, buf: &mut Vec<u8>) -> Result<usize> {
        DataObject::decompress(self, buf)
    }

    fn next_hash_offset(&self) -> Option<NonZeroU64> {
        self.header.next_hash_offset
    }

    fn object_type() -> ObjectType {
        ObjectType::Data
    }
}

impl HashableObjectMut for DataObject<&mut [u8]> {
    fn set_next_hash_offset(&mut self, next_hash_offset: NonZeroU64) {
        self.header.next_hash_offset = Some(next_hash_offset);
    }

    fn set_payload(&mut self, data: &[u8]) {
        match &mut self.payload {
            DataPayloadType::Regular(payload) => {
                payload.copy_from_slice(data);
            }
            DataPayloadType::Compact { payload, .. } => {
                payload.copy_from_slice(data);
            }
        };
    }
}
