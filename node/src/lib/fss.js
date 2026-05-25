/** Pure-JavaScript Forward Secure Pseudorandom Generator (FSPRG).
 *
 * Implements the deterministic key-evolution primitive used by systemd
 * journal Forward Secure Sealing, matching systemd v260.1 behavior.
 */

import { createHash } from 'node:crypto';

export const RECOMMENDED_SECPAR = 1536;
export const RECOMMENDED_SEEDLEN = 12;

function isValidSecpar(secpar) {
  return secpar % 16 === 0 && secpar >= 16 && secpar <= 16384;
}

/** Return the size of the master-secret key for a given secpar. */
export function mskInBytes(secpar) {
  if (!isValidSecpar(secpar)) throw new Error('invalid secpar');
  return 2 + secpar / 8;
}

/** Return the size of the master-public key for a given secpar. */
export function mpkInBytes(secpar) {
  if (!isValidSecpar(secpar)) throw new Error('invalid secpar');
  return 2 + secpar / 8;
}

/** Return the size of an FSPRG state for a given secpar. */
export function stateInBytes(secpar) {
  if (!isValidSecpar(secpar)) throw new Error('invalid secpar');
  return 2 + (2 * secpar) / 8 + 8;
}

function storeSecpar(secpar) {
  const v = secpar / 16 - 1;
  const buf = Buffer.allocUnsafe(2);
  buf.writeUInt16BE(v, 0);
  return buf;
}

function readSecpar(buf) {
  const v = buf.readUInt16BE(0);
  return 16 * (v + 1);
}

function mpiExport(x, buflen) {
  let hex = x.toString(16);
  if (hex.length > buflen * 2) throw new Error('mpiExport: value too large');
  if (hex.length % 2) hex = '0' + hex;
  const out = Buffer.alloc(buflen);
  const b = Buffer.from(hex, 'hex');
  b.copy(out, buflen - b.length);
  return out;
}

function mpiImport(buf) {
  return BigInt('0x' + buf.toString('hex'));
}

function uint64Export(x) {
  const buf = Buffer.allocUnsafe(8);
  buf.writeBigUInt64BE(x, 0);
  return buf;
}

function uint64Import(buf) {
  return buf.readBigUInt64BE(0);
}

/** Deterministically generate buflen pseudorandom bytes from seed and idx. */
function detRandomize(buflen, seed, idx) {
  const out = Buffer.allocUnsafe(buflen);
  let off = 0;
  let ctr = 0;
  const idxBuf = Buffer.allocUnsafe(4);
  idxBuf.writeUInt32BE(idx, 0);
  while (buflen > 0) {
    const h = createHash('sha256');
    h.update(seed);
    h.update(idxBuf);
    const ctrBuf = Buffer.allocUnsafe(4);
    ctrBuf.writeUInt32BE(ctr, 0);
    h.update(ctrBuf);
    const chunk = h.digest();
    const cpylen = Math.min(buflen, 32);
    chunk.copy(out, off, 0, cpylen);
    off += cpylen;
    buflen -= cpylen;
    ctr += 1;
  }
  return out;
}

/** Modular exponentiation: base^exp (mod mod). */
function modPow(base, exp, mod) {
  if (mod === 1n) return 0n;
  let result = 1n;
  let b = base % mod;
  let e = exp;
  while (e > 0n) {
    if (e & 1n) result = (result * b) % mod;
    b = (b * b) % mod;
    e >>= 1n;
  }
  return result;
}

/** Extended Euclidean algorithm returning [g, x, y] such that ax + by = g = gcd(a, b). */
function extendedGcd(a, b) {
  if (b === 0n) return [a, 1n, 0n];
  const [g, x1, y1] = extendedGcd(b, a % b);
  const x = y1;
  const y = x1 - (a / b) * y1;
  return [g, x, y];
}

/** Modular inverse of a (mod m). */
function modInv(a, m) {
  const [g, x] = extendedGcd(a % m, m);
  if (g !== 1n && g !== -1n) throw new Error('modular inverse does not exist');
  return ((x % m) + m) % m;
}

/** Deterministic Miller-Rabin probable-prime test using the first `rounds` prime bases.
 *  Using 64 bases provides a stronger deterministic check than 12 bases and
 *  reduces arbitrary-seed divergence risk.  This does not claim to reproduce
 *  libgcrypt's random witness selection; it is an intentionally stronger
 *  deterministic check.
 */
function isProbablePrime(n, rounds = 64) {
  if (n < 2n) return false;
  if (n === 2n || n === 3n) return true;
  if (n % 2n === 0n) return false;

  let d = n - 1n;
  let r = 0;
  while (d % 2n === 0n) {
    d /= 2n;
    r += 1;
  }

  const bases = [
    2n, 3n, 5n, 7n, 11n, 13n, 17n, 19n, 23n, 29n, 31n, 37n, 41n, 43n, 47n, 53n,
    59n, 61n, 67n, 71n, 73n, 79n, 83n, 89n, 97n, 101n, 103n, 107n, 109n, 113n,
    127n, 131n, 137n, 139n, 149n, 151n, 157n, 163n, 167n, 173n, 179n, 181n,
    191n, 193n, 197n, 199n, 211n, 223n, 227n, 229n, 233n, 239n, 241n, 251n,
    257n, 263n, 269n, 271n, 277n, 281n, 283n, 293n, 307n, 311n,
  ];
  for (let i = 0; i < rounds && i < bases.length; i++) {
    const a = bases[i];
    if (a >= n) continue;
    let x = modPow(a, d, n);
    if (x === 1n || x === n - 1n) continue;
    let cont = false;
    for (let j = 1; j < r; j++) {
      x = modPow(x, 2n, n);
      if (x === n - 1n) {
        cont = true;
        break;
      }
    }
    if (cont) continue;
    return false;
  }
  return true;
}

function genPrime3Mod4(bits, seed, idx) {
  const buflen = bits / 8;
  const buf = Buffer.from(detRandomize(buflen, seed, idx));
  buf[0] |= 0xc0;
  buf[buf.length - 1] |= 0x03;
  let p = mpiImport(buf);
  while (!isProbablePrime(p, 64)) {
    p += 4n;
  }
  return p;
}

function genSquare(n, seed, idx, secpar) {
  const buflen = secpar / 8;
  const buf = Buffer.from(detRandomize(buflen, seed, idx));
  buf[0] &= 0x7f;
  const x = mpiImport(buf);
  if (x >= n) throw new Error('genSquare: x >= n');
  return (x * x) % n;
}

function twopowmodphi(m, p) {
  const phi = p - 1n;
  return modPow(2n, BigInt(m), phi);
}

function crtCompose(xp, xq, p, q) {
  let a = (xq - xp) % q;
  if (a < 0n) a += q;
  const u = modInv(p, q);
  a = (a * u) % q;
  return ((p * a + xp) % (p * q));
}

/** Generate a master key pair deterministically from seed.
 *
 * Returns { msk, mpk } where msk is the master secret key and mpk is the
 * master public key.
 */
export function fsprgGenMK(seed, secpar = RECOMMENDED_SECPAR) {
  if (!isValidSecpar(secpar)) throw new Error('invalid secpar');
  const p = genPrime3Mod4(secpar / 2, seed, 0x01);
  const q = genPrime3Mod4(secpar / 2, seed, 0x02);
  const n = p * q;
  const msk = Buffer.concat([storeSecpar(secpar), mpiExport(p, secpar / 16), mpiExport(q, secpar / 16)]);
  const mpk = Buffer.concat([storeSecpar(secpar), mpiExport(n, secpar / 8)]);
  return { msk, mpk };
}

/** Generate the epoch-0 state from mpk and seed. */
export function fsprgGenState0(mpk, seed) {
  const secpar = readSecpar(mpk);
  const n = mpiImport(mpk.slice(2, 2 + secpar / 8));
  const x = genSquare(n, seed, 0x03, secpar);
  const state = Buffer.alloc(stateInBytes(secpar));
  mpk.copy(state);
  mpiExport(x, secpar / 8).copy(state, 2 + secpar / 8);
  // epoch zero is already zero-initialized
  return state;
}

/** Return the epoch encoded in state. */
export function fsprgGetEpoch(state) {
  const secpar = readSecpar(state);
  return uint64Import(state.slice(2 + 2 * secpar / 8, 2 + 2 * secpar / 8 + 8));
}

/** Evolve state forward by one epoch.  The input is not modified. */
export function fsprgEvolve(state) {
  const secpar = readSecpar(state);
  const n = mpiImport(state.slice(2, 2 + secpar / 8));
  let x = mpiImport(state.slice(2 + secpar / 8, 2 + 2 * secpar / 8));
  let epoch = uint64Import(state.slice(2 + 2 * secpar / 8, 2 + 2 * secpar / 8 + 8));
  x = (x * x) % n;
  epoch += 1n;
  const newState = Buffer.alloc(state.length);
  state.copy(newState);
  mpiExport(x, secpar / 8).copy(newState, 2 + secpar / 8);
  uint64Export(epoch).copy(newState, 2 + 2 * secpar / 8);
  return newState;
}

/** Seek state to an arbitrary epoch using msk and seed.
 *
 * The supplied state must be an epoch-0 state.
 */
export function fsprgSeek(state, epoch, msk, seed) {
  const secpar = readSecpar(msk);
  const p = mpiImport(msk.slice(2, 2 + secpar / 16));
  const q = mpiImport(msk.slice(2 + secpar / 16, 2 + 2 * secpar / 16));
  const n = p * q;
  const x = genSquare(n, seed, 0x03, secpar);
  let xp = x % p;
  let xq = x % q;
  const kp = twopowmodphi(epoch, p);
  const kq = twopowmodphi(epoch, q);
  xp = modPow(xp, kp, p);
  xq = modPow(xq, kq, q);
  const xm = crtCompose(xp, xq, p, q);
  const newState = Buffer.alloc(state.length);
  state.slice(0, 2 + secpar / 8).copy(newState);
  mpiExport(xm, secpar / 8).copy(newState, 2 + secpar / 8);
  uint64Export(BigInt(epoch)).copy(newState, 2 + 2 * secpar / 8);
  return newState;
}

/** Extract a deterministic key from state. */
export function fsprgGetKey(state, keylen, idx) {
  const secpar = readSecpar(state);
  return detRandomize(keylen, state.slice(2, 2 + 2 * secpar / 8 + 8), idx);
}
