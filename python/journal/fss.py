"""Pure-Python Forward Secure Pseudorandom Generator (FSPRG).

Implements the deterministic key-evolution primitive used by systemd
journal Forward Secure Sealing, matching systemd v260.1 behavior.
"""

import hashlib


RECOMMENDED_SECPAR = 1536
RECOMMENDED_SEEDLEN = 12


def _is_valid_secpar(secpar):
    return secpar % 16 == 0 and 16 <= secpar <= 16384


def msk_in_bytes(secpar):
    """Return the size of the master-secret key for a given secpar."""
    if not _is_valid_secpar(secpar):
        raise ValueError('invalid secpar')
    return 2 + secpar // 8


def mpk_in_bytes(secpar):
    """Return the size of the master-public key for a given secpar."""
    if not _is_valid_secpar(secpar):
        raise ValueError('invalid secpar')
    return 2 + secpar // 8


def state_in_bytes(secpar):
    """Return the size of an FSPRG state for a given secpar."""
    if not _is_valid_secpar(secpar):
        raise ValueError('invalid secpar')
    return 2 + 2 * secpar // 8 + 8


def _store_secpar(secpar):
    """Encode secpar as a 2-byte big-endian uint16."""
    v = secpar // 16 - 1
    return v.to_bytes(2, 'big')


def _read_secpar(buf):
    """Decode secpar from a 2-byte big-endian uint16."""
    v = int.from_bytes(buf[:2], 'big')
    return 16 * (v + 1)


def _mpi_export(x, buflen):
    """Export a non-negative int as big-endian, zero-padded to buflen."""
    return x.to_bytes(buflen, 'big')


def _mpi_import(buf):
    """Import a big-endian byte sequence as a non-negative int."""
    return int.from_bytes(buf, 'big')


def _uint64_export(x):
    """Export a uint64 as an 8-byte big-endian sequence."""
    return x.to_bytes(8, 'big')


def _uint64_import(buf):
    """Import an 8-byte big-endian sequence as a uint64."""
    return int.from_bytes(buf[:8], 'big')


def _det_randomize(buflen, seed, idx):
    """Deterministically generate buflen pseudorandom bytes.

    Uses SHA-256 in counter mode: H(seed || idx || ctr).
    """
    out = bytearray()
    base = hashlib.sha256(seed)
    base.update(idx.to_bytes(4, 'big'))
    ctr = 0
    while buflen > 0:
        h = base.copy()
        h.update(ctr.to_bytes(4, 'big'))
        chunk = h.digest()
        cpylen = min(buflen, 32)
        out.extend(chunk[:cpylen])
        buflen -= cpylen
        ctr += 1
    return bytes(out)


def _miller_rabin(n, rounds=64):
    """Deterministic Miller-Rabin probable-prime test.

    Uses the first ``rounds`` prime bases.  Using 64 bases provides a
    stronger deterministic check than 12 bases and reduces
    arbitrary-seed divergence risk.  This does not claim to reproduce
    libgcrypt's random witness selection; it is an intentionally
    stronger deterministic check.
    """
    if n < 2:
        return False
    if n == 2 or n == 3:
        return True
    if n % 2 == 0:
        return False

    # Write n-1 as 2^r * d
    r = 0
    d = n - 1
    while d % 2 == 0:
        r += 1
        d //= 2

    bases = [
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53,
        59, 61, 67, 71, 73, 79, 83, 89, 97, 101, 103, 107, 109, 113,
        127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181,
        191, 193, 197, 199, 211, 223, 227, 229, 233, 239, 241, 251,
        257, 263, 269, 271, 277, 281, 283, 293, 307, 311,
    ]
    for a in bases[:rounds]:
        if a >= n:
            continue
        x = pow(a, d, n)
        if x == 1 or x == n - 1:
            continue
        for _ in range(r - 1):
            x = pow(x, 2, n)
            if x == n - 1:
                break
        else:
            return False
    return True


def _genprime3mod4(bits, seed, idx):
    """Deterministically generate a prime that is 3 (mod 4)."""
    buflen = bits // 8
    buf = bytearray(_det_randomize(buflen, seed, idx))
    buf[0] |= 0xc0          # set top two bits
    buf[-1] |= 0x03         # set bottom two bits => 3 (mod 4)
    p = int.from_bytes(buf, 'big')
    while not _miller_rabin(p, 64):
        p += 4
    return p


def _gensquare(n, seed, idx, secpar):
    """Deterministically generate a quadratic residue (mod n)."""
    buflen = secpar // 8
    buf = bytearray(_det_randomize(buflen, seed, idx))
    buf[0] &= 0x7f          # clear top bit so x < n
    x = int.from_bytes(buf, 'big')
    if x >= n:
        raise ValueError('generated x is not less than n')
    return (x * x) % n


def _twopowmodphi(m, p):
    """Compute 2^m (mod phi(p)) where phi(p) = p-1 for prime p."""
    return pow(2, m, p - 1)


def _crt_compose(xp, xq, p, q):
    """Compose (xp, xq) into x (mod n) using the Chinese Remainder Theorem."""
    a = (xq - xp) % q
    u = pow(p, -1, q)
    a = (a * u) % q
    return (p * a + xp) % (p * q)


def gen_mk(seed, secpar=RECOMMENDED_SECPAR):
    """Generate a master key pair deterministically from *seed*.

    Returns ``(msk, mpk)`` where *msk* is the master secret key and
    *mpk* is the master public key.
    """
    if not _is_valid_secpar(secpar):
        raise ValueError('invalid secpar')
    p = _genprime3mod4(secpar // 2, seed, 0x01)
    q = _genprime3mod4(secpar // 2, seed, 0x02)
    n = p * q
    msk = _store_secpar(secpar) + _mpi_export(p, secpar // 16) + _mpi_export(q, secpar // 16)
    mpk = _store_secpar(secpar) + _mpi_export(n, secpar // 8)
    return msk, mpk


def gen_state0(mpk, seed):
    """Generate the epoch-0 state from *mpk* and *seed*."""
    secpar = _read_secpar(mpk)
    n = _mpi_import(mpk[2:2 + secpar // 8])
    x = _gensquare(n, seed, 0x03, secpar)
    state = bytearray(mpk)
    state.extend(_mpi_export(x, secpar // 8))
    state.extend(b'\x00' * 8)
    return bytes(state)


def evolve(state):
    """Evolve *state* forward by one epoch.

    Returns the new state; the input is not modified.
    """
    secpar = _read_secpar(state)
    n = _mpi_import(state[2:2 + secpar // 8])
    x = _mpi_import(state[2 + secpar // 8:2 + 2 * secpar // 8])
    epoch = _uint64_import(state[2 + 2 * secpar // 8:2 + 2 * secpar // 8 + 8])
    x = (x * x) % n
    epoch += 1
    new_state = bytearray(state)
    new_state[2 + secpar // 8:2 + 2 * secpar // 8] = _mpi_export(x, secpar // 8)
    new_state[2 + 2 * secpar // 8:2 + 2 * secpar // 8 + 8] = _uint64_export(epoch)
    return bytes(new_state)


def get_epoch(state):
    """Return the epoch encoded in *state*."""
    secpar = _read_secpar(state)
    return _uint64_import(state[2 + 2 * secpar // 8:2 + 2 * secpar // 8 + 8])


def seek(state, epoch, msk, seed):
    """Seek *state* to an arbitrary *epoch* using *msk* and *seed*.

    The supplied *state* must be an epoch-0 state (typically produced
    by :func:`gen_state0`).  Returns the state at the requested epoch.
    """
    secpar = _read_secpar(msk)
    p = _mpi_import(msk[2:2 + secpar // 16])
    q = _mpi_import(msk[2 + secpar // 16:2 + 2 * secpar // 16])
    n = p * q
    x = _gensquare(n, seed, 0x03, secpar)
    xp = x % p
    xq = x % q
    kp = _twopowmodphi(epoch, p)
    kq = _twopowmodphi(epoch, q)
    xp = pow(xp, kp, p)
    xq = pow(xq, kq, q)
    xm = _crt_compose(xp, xq, p, q)
    new_state = bytearray(state[:2 + secpar // 8])
    new_state.extend(_mpi_export(xm, secpar // 8))
    new_state.extend(_uint64_export(epoch))
    return bytes(new_state)


def get_key(state, keylen, idx):
    """Extract a deterministic key from *state*.

    *idx* is a 32-bit index allowing multiple independent keys per
    epoch.
    """
    secpar = _read_secpar(state)
    return _det_randomize(keylen, state[2:2 + 2 * secpar // 8 + 8], idx)
