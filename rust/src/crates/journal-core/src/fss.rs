//! Pure-Rust Forward Secure Pseudorandom Generator (FSPRG).
//!
//! Implements the deterministic key-evolution primitive used by systemd
//! journal Forward Secure Sealing, matching systemd v260.1 behavior.

use num_bigint::BigUint;
use sha2::{Digest, Sha256};

pub const RECOMMENDED_SECPAR: u32 = 1536;
pub const RECOMMENDED_SEEDLEN: usize = 12;

fn is_valid_secpar(secpar: u32) -> bool {
    secpar % 16 == 0 && secpar >= 16 && secpar <= 16384
}

/// Return the size of the master-secret key for a given secpar.
pub fn msk_in_bytes(secpar: u32) -> usize {
    assert!(is_valid_secpar(secpar), "invalid secpar");
    2 + (secpar / 8) as usize
}

/// Return the size of the master-public key for a given secpar.
pub fn mpk_in_bytes(secpar: u32) -> usize {
    assert!(is_valid_secpar(secpar), "invalid secpar");
    2 + (secpar / 8) as usize
}

/// Return the size of an FSPRG state for a given secpar.
pub fn state_in_bytes(secpar: u32) -> usize {
    assert!(is_valid_secpar(secpar), "invalid secpar");
    2 + 2 * (secpar / 8) as usize + 8
}

fn store_secpar(secpar: u32) -> [u8; 2] {
    let v = (secpar / 16 - 1) as u16;
    v.to_be_bytes()
}

fn read_secpar(buf: &[u8]) -> u32 {
    let v = u16::from_be_bytes([buf[0], buf[1]]);
    16 * (u32::from(v) + 1)
}

fn mpi_export(x: &BigUint, buflen: usize) -> Vec<u8> {
    let b = x.to_bytes_be();
    assert!(b.len() <= buflen, "mpi_export: value too large for buffer");
    if b.len() == buflen {
        return b;
    }
    let mut out = vec![0u8; buflen];
    out[buflen - b.len()..].copy_from_slice(&b);
    out
}

fn mpi_import(buf: &[u8]) -> BigUint {
    BigUint::from_bytes_be(buf)
}

fn uint64_export(x: u64) -> [u8; 8] {
    x.to_be_bytes()
}

fn uint64_import(buf: &[u8]) -> u64 {
    u64::from_be_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ])
}

/// Deterministically generate `buflen` pseudorandom bytes from `seed` and `idx`.
fn det_randomize(buflen: usize, seed: &[u8], idx: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(buflen);
    // Build the intermediate SHA256 state of seed||idx.
    let mut base = Sha256::new();
    base.update(seed);
    base.update(idx.to_be_bytes());
    // We need to clone the hasher for each counter.  sha2::Sha256 implements Clone.
    let mut ctr: u32 = 0;
    while out.len() < buflen {
        let mut h = base.clone();
        h.update(ctr.to_be_bytes());
        let chunk = h.finalize();
        let cpylen = std::cmp::min(buflen - out.len(), 32);
        out.extend_from_slice(&chunk[..cpylen]);
        ctr += 1;
    }
    out
}

/// Deterministic Miller-Rabin probable-prime test using the first `rounds`
/// prime bases.  Using 64 bases provides a stronger deterministic check than
/// 12 bases and reduces arbitrary-seed divergence risk.  This does not claim
/// to reproduce libgcrypt's random witness selection; it is an intentionally
/// stronger deterministic check.
fn miller_rabin(n: &BigUint, rounds: usize) -> bool {
    if n < &BigUint::from(2u32) {
        return false;
    }
    if *n == BigUint::from(2u32) || *n == BigUint::from(3u32) {
        return true;
    }
    if n.bit(0) == false {
        // even
        return false;
    }

    // Write n-1 as 2^r * d
    let mut d = n - BigUint::from(1u32);
    let mut r = 0;
    while d.bit(0) == false {
        d >>= 1;
        r += 1;
    }

    let bases: [u64; 64] = [
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53,
        59, 61, 67, 71, 73, 79, 83, 89, 97, 101, 103, 107, 109, 113,
        127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181,
        191, 193, 197, 199, 211, 223, 227, 229, 233, 239, 241, 251,
        257, 263, 269, 271, 277, 281, 283, 293, 307, 311,
    ];
    for &a in bases.iter().take(rounds) {
        let a_big = BigUint::from(a);
        if &a_big >= n {
            continue;
        }
        let mut x = a_big.modpow(&d, n);
        let n_minus_1 = n - BigUint::from(1u32);
        if x == BigUint::from(1u32) || x == n_minus_1 {
            continue;
        }
        let mut cont = false;
        for _ in 1..r {
            x = (&x * &x) % n;
            if x == n_minus_1 {
                cont = true;
                break;
            }
        }
        if cont {
            continue;
        }
        return false;
    }
    true
}

fn gen_prime_3mod4(bits: u32, seed: &[u8], idx: u32) -> BigUint {
    let buflen = (bits / 8) as usize;
    let mut buf = det_randomize(buflen, seed, idx);
    buf[0] |= 0xc0;
    let last = buflen - 1;
    buf[last] |= 0x03;
    let four = BigUint::from(4u32);
    let mut p = mpi_import(&buf);
    while !miller_rabin(&p, 64) {
        p += &four;
    }
    p
}

fn gen_square(n: &BigUint, seed: &[u8], idx: u32, secpar: u32) -> BigUint {
    let buflen = (secpar / 8) as usize;
    let mut buf = det_randomize(buflen, seed, idx);
    buf[0] &= 0x7f;
    let x = mpi_import(&buf);
    assert!(x < *n, "genSquare: x >= n");
    (&x * &x) % n
}

fn twopowmodphi(m: u64, p: &BigUint) -> BigUint {
    let phi = p - BigUint::from(1u32);
    BigUint::from(2u32).modpow(&BigUint::from(m), &phi)
}

fn crt_compose(xp: &BigUint, xq: &BigUint, p: &BigUint, q: &BigUint) -> BigUint {
    let mut a = if xq >= xp {
        xq - xp
    } else {
        xq + q - xp
    };
    let u = p
        .modinv(q)
        .expect("CRT: p and q must be coprime");
    a = (&a * &u) % q;
    let n = p * q;
    (p * &a + xp) % &n
}

/// Generate a master key pair deterministically from `seed`.
///
/// Returns `(msk, mpk)` where `msk` is the master secret key and `mpk` is
/// the master public key.
pub fn gen_mk(seed: &[u8], secpar: u32) -> (Vec<u8>, Vec<u8>) {
    assert!(is_valid_secpar(secpar), "invalid secpar");
    let p = gen_prime_3mod4(secpar / 2, seed, 0x01);
    let q = gen_prime_3mod4(secpar / 2, seed, 0x02);
    let n = &p * &q;
    let msk = [
        store_secpar(secpar).as_slice(),
        &mpi_export(&p, (secpar / 16) as usize),
        &mpi_export(&q, (secpar / 16) as usize),
    ]
    .concat();
    let mpk = [
        store_secpar(secpar).as_slice(),
        &mpi_export(&n, (secpar / 8) as usize),
    ]
    .concat();
    (msk, mpk)
}

/// Generate the epoch-0 state from `mpk` and `seed`.
pub fn gen_state0(mpk: &[u8], seed: &[u8]) -> Vec<u8> {
    let secpar = read_secpar(mpk);
    let n = mpi_import(&mpk[2..2 + (secpar / 8) as usize]);
    let x = gen_square(&n, seed, 0x03, secpar);
    let mut state = vec![0u8; state_in_bytes(secpar)];
    state[..mpk.len()].copy_from_slice(mpk);
    state[2 + (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize]
        .copy_from_slice(&mpi_export(&x, (secpar / 8) as usize));
    // epoch zero is already zero-initialized
    state
}

/// Return the epoch encoded in `state`.
pub fn get_epoch(state: &[u8]) -> u64 {
    let secpar = read_secpar(state);
    uint64_import(&state[2 + 2 * (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize + 8])
}

/// Evolve `state` forward by one epoch.  The input is not modified.
pub fn evolve(state: &[u8]) -> Vec<u8> {
    let secpar = read_secpar(state);
    let n = mpi_import(&state[2..2 + (secpar / 8) as usize]);
    let mut x = mpi_import(&state[2 + (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize]);
    let mut epoch = uint64_import(&state[2 + 2 * (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize + 8]);
    x = (&x * &x) % &n;
    epoch += 1;
    let mut new_state = state.to_vec();
    new_state[2 + (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize]
        .copy_from_slice(&mpi_export(&x, (secpar / 8) as usize));
    new_state[2 + 2 * (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize + 8]
        .copy_from_slice(&uint64_export(epoch));
    new_state
}

/// Seek `state` to an arbitrary `epoch` using `msk` and `seed`.
///
/// The supplied `state` must be an epoch-0 state.
pub fn seek(state: &[u8], epoch: u64, msk: &[u8], seed: &[u8]) -> Vec<u8> {
    let secpar = read_secpar(msk);
    let p = mpi_import(&msk[2..2 + (secpar / 16) as usize]);
    let q = mpi_import(&msk[2 + (secpar / 16) as usize..2 + 2 * (secpar / 16) as usize]);
    let n = &p * &q;
    let x = gen_square(&n, seed, 0x03, secpar);
    let mut xp = &x % &p;
    let mut xq = &x % &q;
    let kp = twopowmodphi(epoch, &p);
    let kq = twopowmodphi(epoch, &q);
    xp = xp.modpow(&kp, &p);
    xq = xq.modpow(&kq, &q);
    let xm = crt_compose(&xp, &xq, &p, &q);
    let mut new_state = vec![0u8; state.len()];
    new_state[..2 + (secpar / 8) as usize].copy_from_slice(&state[..2 + (secpar / 8) as usize]);
    new_state[2 + (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize]
        .copy_from_slice(&mpi_export(&xm, (secpar / 8) as usize));
    new_state[2 + 2 * (secpar / 8) as usize..2 + 2 * (secpar / 8) as usize + 8]
        .copy_from_slice(&uint64_export(epoch));
    new_state
}

/// Extract a deterministic key from `state`.
pub fn get_key(state: &[u8], keylen: usize, idx: u32) -> Vec<u8> {
    let secpar = read_secpar(state);
    det_randomize(keylen, &state[2..2 + 2 * (secpar / 8) as usize + 8], idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[derive(serde::Deserialize)]
    struct VectorKey {
        idx: u32,
        keylen: usize,
        key_hex: String,
    }

    #[derive(serde::Deserialize)]
    struct VectorEpoch {
        epoch: u64,
        state_hex: String,
        seek_state_hex: String,
        keys: Vec<VectorKey>,
    }

    #[derive(serde::Deserialize)]
    struct Vector {
        seed_desc: String,
        seed_hex: String,
        msk_hex: String,
        mpk_hex: String,
        state0_hex: String,
        epochs: Vec<VectorEpoch>,
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        fsprg_params: serde_json::Value,
        vectors: Vec<Vector>,
    }

    fn load_fixture() -> Fixture {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // journal-core lives in rust/src/crates/journal-core ; repo root is 4 levels up.
        for _ in 0..4 {
            path.pop();
        }
        path.push("tests");
        path.push("fss");
        path.push("fixtures");
        path.push("fsprg-vectors-v01.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("failed to read fixture at {}: {}", path.display(), e)
        });
        serde_json::from_str(&text).expect("invalid fixture JSON")
    }

    #[test]
    fn test_fsprg_vectors() {
        let fixture = load_fixture();
        let secpar = fixture
            .fsprg_params
            .get("secpar")
            .and_then(|v| v.as_u64())
            .expect("missing secpar") as u32;
        assert_eq!(secpar, RECOMMENDED_SECPAR);
        for vec in &fixture.vectors {
            let seed = hex::decode(&vec.seed_hex).expect("decode seed");
            let expected_msk = hex::decode(&vec.msk_hex).expect("decode msk");
            let expected_mpk = hex::decode(&vec.mpk_hex).expect("decode mpk");
            let expected_state0 = hex::decode(&vec.state0_hex).expect("decode state0");
            assert_eq!(seed.len(), RECOMMENDED_SEEDLEN);
            assert_eq!(expected_msk.len(), msk_in_bytes(secpar));
            assert_eq!(expected_mpk.len(), mpk_in_bytes(secpar));

            let (msk, mpk) = gen_mk(&seed, secpar);
            assert_eq!(msk, expected_msk, "msk mismatch for {}", vec.seed_desc);
            assert_eq!(mpk, expected_mpk, "mpk mismatch for {}", vec.seed_desc);

            let state0 = gen_state0(&mpk, &seed);
            assert_eq!(state0, expected_state0, "state0 mismatch for {}", vec.seed_desc);
            assert_eq!(get_epoch(&state0), 0, "epoch0 mismatch for {}", vec.seed_desc);

            for ep in &vec.epochs {
                let mut evolved = state0.clone();
                for _ in 0..ep.epoch {
                    evolved = evolve(&evolved);
                }
                let expected_state = hex::decode(&ep.state_hex).expect("decode state");
                assert_eq!(
                    evolved, expected_state,
                    "evolve mismatch for {} epoch {}",
                    vec.seed_desc, ep.epoch
                );

                let seeked = seek(&state0, ep.epoch, &msk, &seed);
                let expected_seek = hex::decode(&ep.seek_state_hex).expect("decode seek_state");
                assert_eq!(
                    seeked, expected_seek,
                    "seek mismatch for {} epoch {}",
                    vec.seed_desc, ep.epoch
                );

                for k in &ep.keys {
                    let key = get_key(&evolved, k.keylen, k.idx);
                    let expected_key = hex::decode(&k.key_hex).expect("decode key");
                    assert_eq!(
                        key, expected_key,
                        "key mismatch for {} epoch {} idx {}",
                        vec.seed_desc, ep.epoch, k.idx
                    );
                }
            }
        }
    }
}
