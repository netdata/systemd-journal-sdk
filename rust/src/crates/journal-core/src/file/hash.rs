use siphasher::sip::SipHasher24;
use std::hash::Hasher;

pub fn jenkins_hash64(data: &[u8]) -> u64 {
    jenkins_hash64_contiguous(data)
}

pub fn jenkins_hash64_parts<'a, I>(parts: I) -> u64
where
    I: IntoIterator<Item = &'a [u8]>,
    I::IntoIter: Clone,
{
    let iter = parts.into_iter();
    let mut peek = iter.clone();
    let Some(first) = peek.next() else {
        return jenkins_hash64_contiguous(&[]);
    };
    let Some(second) = peek.next() else {
        return jenkins_hash64_contiguous(first);
    };

    let total_len = first.len() + second.len() + peek.map(|part| part.len()).sum::<usize>();
    jenkins_hash64_from_parts(iter, total_len)
}

fn jenkins_hash64_contiguous(data: &[u8]) -> u64 {
    if data.is_empty() {
        return 0xdead_beef_dead_beef;
    }

    let mut remaining = data.len();
    let init = 0xdead_beefu32.wrapping_add(remaining as u32);
    let mut a = init;
    let mut b = init;
    let mut c = init;
    let mut offset = 0usize;

    while remaining > 12 {
        a = a.wrapping_add(read_u32_le_at(data, offset));
        b = b.wrapping_add(read_u32_le_at(data, offset + 4));
        c = c.wrapping_add(read_u32_le_at(data, offset + 8));
        mix(&mut a, &mut b, &mut c);
        offset += 12;
        remaining -= 12;
    }

    let tail = &data[offset..];
    match remaining {
        12 => {
            c = c.wrapping_add(read_u32_le_at(tail, 8));
            b = b.wrapping_add(read_u32_le_at(tail, 4));
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        11 => {
            c = c
                .wrapping_add(tail[8] as u32 | ((tail[9] as u32) << 8) | ((tail[10] as u32) << 16));
            b = b.wrapping_add(read_u32_le_at(tail, 4));
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        10 => {
            c = c.wrapping_add(tail[8] as u32 | ((tail[9] as u32) << 8));
            b = b.wrapping_add(read_u32_le_at(tail, 4));
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        9 => {
            c = c.wrapping_add(tail[8] as u32);
            b = b.wrapping_add(read_u32_le_at(tail, 4));
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        8 => {
            b = b.wrapping_add(read_u32_le_at(tail, 4));
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        7 => {
            b = b.wrapping_add(tail[4] as u32 | ((tail[5] as u32) << 8) | ((tail[6] as u32) << 16));
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        6 => {
            b = b.wrapping_add(tail[4] as u32 | ((tail[5] as u32) << 8));
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        5 => {
            b = b.wrapping_add(tail[4] as u32);
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        4 => {
            a = a.wrapping_add(read_u32_le_at(tail, 0));
        }
        3 => {
            a = a.wrapping_add(tail[0] as u32 | ((tail[1] as u32) << 8) | ((tail[2] as u32) << 16));
        }
        2 => {
            a = a.wrapping_add(tail[0] as u32 | ((tail[1] as u32) << 8));
        }
        1 => {
            a = a.wrapping_add(tail[0] as u32);
        }
        0 => return ((c as u64) << 32) | b as u64,
        _ => unreachable!("tail length cannot exceed 12"),
    }

    final_mix(&mut a, &mut b, &mut c);
    ((c as u64) << 32) | b as u64
}

#[inline(always)]
fn read_u32_le_at(data: &[u8], offset: usize) -> u32 {
    debug_assert!(offset + 4 <= data.len());
    // SAFETY: callers check chunk/tail length before requesting each word.
    // `read_unaligned` is required because journal payloads are byte strings.
    u32::from_le(unsafe { std::ptr::read_unaligned(data.as_ptr().add(offset).cast::<u32>()) })
}

fn jenkins_hash64_from_parts<'a>(
    parts: impl IntoIterator<Item = &'a [u8]>,
    total_len: usize,
) -> u64 {
    let mut reader = PartReader::new(parts.into_iter());
    let mut remaining = total_len;
    let init = 0xdead_beefu32.wrapping_add(total_len as u32);
    let mut a = init;
    let mut b = init;
    let mut c = init;

    while remaining > 12 {
        a = a.wrapping_add(reader.read_u32_le());
        b = b.wrapping_add(reader.read_u32_le());
        c = c.wrapping_add(reader.read_u32_le());
        mix(&mut a, &mut b, &mut c);
        remaining -= 12;
    }

    if remaining == 0 {
        return ((c as u64) << 32) | b as u64;
    }

    let mut tail = [0u8; 12];
    for byte in tail.iter_mut().take(remaining) {
        *byte = reader
            .next_byte()
            .expect("part reader should contain total_len bytes");
    }

    match remaining {
        12 => {
            c = c.wrapping_add(u32::from_le_bytes(tail[8..12].try_into().unwrap()));
            b = b.wrapping_add(u32::from_le_bytes(tail[4..8].try_into().unwrap()));
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        11 => {
            c = c
                .wrapping_add(tail[8] as u32 | ((tail[9] as u32) << 8) | ((tail[10] as u32) << 16));
            b = b.wrapping_add(u32::from_le_bytes(tail[4..8].try_into().unwrap()));
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        10 => {
            c = c.wrapping_add(tail[8] as u32 | ((tail[9] as u32) << 8));
            b = b.wrapping_add(u32::from_le_bytes(tail[4..8].try_into().unwrap()));
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        9 => {
            c = c.wrapping_add(tail[8] as u32);
            b = b.wrapping_add(u32::from_le_bytes(tail[4..8].try_into().unwrap()));
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        8 => {
            b = b.wrapping_add(u32::from_le_bytes(tail[4..8].try_into().unwrap()));
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        7 => {
            b = b.wrapping_add(tail[4] as u32 | ((tail[5] as u32) << 8) | ((tail[6] as u32) << 16));
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        6 => {
            b = b.wrapping_add(tail[4] as u32 | ((tail[5] as u32) << 8));
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        5 => {
            b = b.wrapping_add(tail[4] as u32);
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        4 => {
            a = a.wrapping_add(u32::from_le_bytes(tail[0..4].try_into().unwrap()));
        }
        3 => {
            a = a.wrapping_add(tail[0] as u32 | ((tail[1] as u32) << 8) | ((tail[2] as u32) << 16));
        }
        2 => {
            a = a.wrapping_add(tail[0] as u32 | ((tail[1] as u32) << 8));
        }
        1 => {
            a = a.wrapping_add(tail[0] as u32);
        }
        _ => unreachable!("tail length cannot exceed 12"),
    }

    final_mix(&mut a, &mut b, &mut c);
    ((c as u64) << 32) | b as u64
}

struct PartReader<'a, I>
where
    I: Iterator<Item = &'a [u8]>,
{
    parts: I,
    current: &'a [u8],
    offset: usize,
}

impl<'a, I> PartReader<'a, I>
where
    I: Iterator<Item = &'a [u8]>,
{
    fn new(parts: I) -> Self {
        Self {
            parts,
            current: &[],
            offset: 0,
        }
    }

    fn next_byte(&mut self) -> Option<u8> {
        loop {
            if self.offset < self.current.len() {
                let byte = self.current[self.offset];
                self.offset += 1;
                return Some(byte);
            }

            self.current = self.parts.next()?;
            self.offset = 0;
        }
    }

    fn read_u32_le(&mut self) -> u32 {
        let b0 = self
            .next_byte()
            .expect("part reader should contain enough bytes");
        let b1 = self
            .next_byte()
            .expect("part reader should contain enough bytes");
        let b2 = self
            .next_byte()
            .expect("part reader should contain enough bytes");
        let b3 = self
            .next_byte()
            .expect("part reader should contain enough bytes");
        u32::from_le_bytes([b0, b1, b2, b3])
    }
}

#[inline]
fn rot(value: u32, bits: u32) -> u32 {
    value.rotate_left(bits)
}

#[inline]
fn mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*c);
    *a ^= rot(*c, 4);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= rot(*a, 6);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= rot(*b, 8);
    *b = b.wrapping_add(*a);
    *a = a.wrapping_sub(*c);
    *a ^= rot(*c, 16);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= rot(*a, 19);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= rot(*b, 4);
    *b = b.wrapping_add(*a);
}

#[inline]
fn final_mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *c ^= *b;
    *c = c.wrapping_sub(rot(*b, 14));
    *a ^= *c;
    *a = a.wrapping_sub(rot(*c, 11));
    *b ^= *a;
    *b = b.wrapping_sub(rot(*a, 25));
    *c ^= *b;
    *c = c.wrapping_sub(rot(*b, 16));
    *a ^= *c;
    *a = a.wrapping_sub(rot(*c, 4));
    *b ^= *a;
    *b = b.wrapping_sub(rot(*a, 14));
    *c ^= *b;
    *c = c.wrapping_sub(rot(*b, 24));
}

pub fn siphash24(data: &[u8], key: &[u8; 16]) -> u64 {
    siphash24_parts([data], key)
}

pub fn siphash24_parts<'a>(parts: impl IntoIterator<Item = &'a [u8]>, key: &[u8; 16]) -> u64 {
    let k0 = u64::from_le_bytes(key[0..8].try_into().unwrap());
    let k1 = u64::from_le_bytes(key[8..16].try_into().unwrap());

    let mut hasher = SipHasher24::new_with_keys(k0, k1);
    for part in parts {
        hasher.write(part);
    }
    hasher.finish()
}

pub fn journal_hash_data(data: &[u8], is_keyed_hash: bool, file_id: Option<&[u8; 16]>) -> u64 {
    journal_hash_data_parts([data], is_keyed_hash, file_id)
}

pub fn journal_hash_data_parts<'a, I>(
    parts: I,
    is_keyed_hash: bool,
    file_id: Option<&[u8; 16]>,
) -> u64
where
    I: IntoIterator<Item = &'a [u8]>,
    I::IntoIter: Clone,
{
    if is_keyed_hash {
        if let Some(file_id) = file_id {
            siphash24_parts(parts, file_id)
        } else {
            // FIXME: verify fallback behaviour
            jenkins_hash64_parts(parts)
        }
    } else {
        jenkins_hash64_parts(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jenkins_hash64_matches_systemd_lookup3_values() {
        let cases: &[(&[u8], u64)] = &[
            (b"", 0xdead_beef_dead_beef),
            (b"SYSLOG_IDENTIFIER=netdata", 0x45cc_d0e9_ed13_614a),
            (b"_SYSTEMD_UNIT=netdata.service", 0x1013_c5df_11a9_83f0),
            (b"PRIORITY=6", 0x80f0_9f19_808d_26a3),
            (b"MESSAGE=Test message", 0x8ed5_3fb5_2aa5_c55d),
        ];

        for (payload, expected) in cases {
            assert_eq!(jenkins_hash64(payload), *expected);
        }
    }

    #[test]
    fn multi_part_hashes_match_concatenated_payload() {
        let key = *b"0123456789abcdef";
        let parts = [b"MESSAGE".as_slice(), b"=".as_slice(), b"hello".as_slice()];
        let joined = b"MESSAGE=hello";

        assert_eq!(jenkins_hash64(joined), jenkins_hash64_parts(parts));
        assert_eq!(siphash24(joined, &key), siphash24_parts(parts, &key));
        assert_eq!(
            journal_hash_data(joined, false, None),
            journal_hash_data_parts(parts, false, None)
        );
        assert_eq!(
            journal_hash_data(joined, true, Some(&key)),
            journal_hash_data_parts(parts, true, Some(&key))
        );
    }

    #[test]
    fn multi_part_jenkins_hash_matches_all_payload_splits() {
        let payload = b"MESSAGE=abcdefghijklmnopqrstuvwxyz0123456789";

        for first in 0..=payload.len() {
            for second in first..=payload.len() {
                let parts = [
                    &payload[..first],
                    &payload[first..second],
                    &payload[second..],
                ];
                assert_eq!(
                    jenkins_hash64(payload),
                    jenkins_hash64_parts(parts),
                    "split at {first}, {second}"
                );
            }
        }
    }

    #[test]
    fn contiguous_jenkins_fast_path_matches_multi_part_reference_for_tail_lengths() {
        let payload: Vec<u8> = (0..=255).map(|i| ((i * 37 + 11) & 0xff) as u8).collect();

        for len in 0..=payload.len() {
            let joined = &payload[..len];
            for split in 0..=len {
                let parts = [&joined[..split], &joined[split..], &[][..]];
                assert_eq!(
                    jenkins_hash64(joined),
                    jenkins_hash64_parts(parts),
                    "len={len} split={split}"
                );
            }
        }
    }
}
