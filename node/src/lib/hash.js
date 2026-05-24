// SipHash-2-4 and Jenkins hash for systemd journal files.
// Both return BigInt for full 64-bit precision (matching Go).

function rotl64(v, n) {
  return ((v << BigInt(n)) | (v >> BigInt(64 - n))) & 0xffffffffffffffffn;
}

// SipHash-2-4 keyed hash (key is a 16-byte Buffer).
export function sipHash24(key, msg) {
  if (!Buffer.isBuffer(msg)) msg = Buffer.from(msg);
  const k0 = key.readBigUInt64LE(0);
  const k1 = key.readBigUInt64LE(8);

  let v0 = 0x736f6d6570736575n ^ k0;
  let v1 = 0x646f72616e646f6dn ^ k1;
  let v2 = 0x6c7967656e657261n ^ k0;
  let v3 = 0x7465646279746573n ^ k1;

  const round = () => {
    v0 = (v0 + v1) & 0xffffffffffffffffn;
    v1 = rotl64(v1, 13);
    v1 ^= v0;
    v0 = rotl64(v0, 32);
    v2 = (v2 + v3) & 0xffffffffffffffffn;
    v3 = rotl64(v3, 16);
    v3 ^= v2;
    v0 = (v0 + v3) & 0xffffffffffffffffn;
    v3 = rotl64(v3, 21);
    v3 ^= v0;
    v2 = (v2 + v1) & 0xffffffffffffffffn;
    v1 = rotl64(v1, 17);
    v1 ^= v2;
    v2 = rotl64(v2, 32);
  };

  const len = BigInt(msg.length);
  let i = 0;
  while (i + 8 <= msg.length) {
    const m = msg.readBigUInt64LE(i);
    v3 ^= m;
    round();
    round();
    v0 ^= m;
    i += 8;
  }

  let b = len << 56n;
  for (let j = 0; i < msg.length; i++, j++) {
    b |= BigInt(msg[i]) << BigInt(8 * j);
  }

  v3 ^= b;
  round();
  round();
  v0 ^= b;
  v2 ^= 0xffn;
  for (let r = 0; r < 4; r++) round();

  return (v0 ^ v1 ^ v2 ^ v3) & 0xffffffffffffffffn;
}

// Jenkins lookup3 hash returning a 64-bit BigInt.
export function jenkinsHash64(data) {
  if (!Buffer.isBuffer(data)) data = Buffer.from(data);
  const [a, b] = jenkinsHashLittle2(data);
  return (BigInt(a) << 32n) | BigInt(b);
}

function jenkinsHashLittle2(data) {
  const length = data.length;
  let a = (0xdeadbeef + length) >>> 0;
  let b = a;
  let c = a;

  let i = 0;
  while (i + 12 <= length) {
    a = (a + (data[i] | (data[i+1] << 8) | (data[i+2] << 16) | (data[i+3] << 24))) >>> 0;
    b = (b + (data[i+4] | (data[i+5] << 8) | (data[i+6] << 16) | (data[i+7] << 24))) >>> 0;
    c = (c + (data[i+8] | (data[i+9] << 8) | (data[i+10] << 16) | (data[i+11] << 24))) >>> 0;
    [a, b, c] = jenkinsMix(a, b, c);
    i += 12;
  }

  const k = data.slice(i);
  if (k.length === 0) return [c >>> 0, b >>> 0];

  switch (k.length) {
    case 12: c = (c + (k[11] << 24)) >>> 0; // fallthrough
    case 11: c = (c + (k[10] << 16)) >>> 0;
    case 10: c = (c + (k[9] << 8)) >>> 0;
    case 9:  c = (c + k[8]) >>> 0;
    case 8:  b = (b + (k[7] << 24)) >>> 0;
    case 7:  b = (b + (k[6] << 16)) >>> 0;
    case 6:  b = (b + (k[5] << 8)) >>> 0;
    case 5:  b = (b + k[4]) >>> 0;
    case 4:  a = (a + (k[3] << 24)) >>> 0;
    case 3:  a = (a + (k[2] << 16)) >>> 0;
    case 2:  a = (a + (k[1] << 8)) >>> 0;
    case 1:  a = (a + k[0]) >>> 0;
  }

  [a, b, c] = jenkinsFinal(a, b, c);
  return [c >>> 0, b >>> 0];
}

function jenkinsMix(a, b, c) {
  a = (a - c) >>> 0; a = (a ^ rotl32(c, 4)) >>> 0;  c = (c + b) >>> 0;
  b = (b - a) >>> 0; b = (b ^ rotl32(a, 6)) >>> 0;  a = (a + c) >>> 0;
  c = (c - b) >>> 0; c = (c ^ rotl32(b, 8)) >>> 0;  b = (b + a) >>> 0;
  a = (a - c) >>> 0; a = (a ^ rotl32(c, 16)) >>> 0; c = (c + b) >>> 0;
  b = (b - a) >>> 0; b = (b ^ rotl32(a, 19)) >>> 0; a = (a + c) >>> 0;
  c = (c - b) >>> 0; c = (c ^ rotl32(b, 4)) >>> 0;  b = (b + a) >>> 0;
  return [a, b, c];
}

function jenkinsFinal(a, b, c) {
  c = (c ^ b) >>> 0; c = (c - rotl32(b, 14)) >>> 0;
  a = (a ^ c) >>> 0; a = (a - rotl32(c, 11)) >>> 0;
  b = (b ^ a) >>> 0; b = (b - rotl32(a, 25)) >>> 0;
  c = (c ^ b) >>> 0; c = (c - rotl32(b, 16)) >>> 0;
  a = (a ^ c) >>> 0; a = (a - rotl32(c, 4)) >>> 0;
  b = (b ^ a) >>> 0; b = (b - rotl32(a, 14)) >>> 0;
  c = (c ^ b) >>> 0; c = (c - rotl32(b, 24)) >>> 0;
  return [a, b, c];
}

function rotl32(v, n) {
  return ((v << n) | (v >>> (32 - n))) >>> 0;
}

// Validate and parse a match string (FIELD=value).
export function parseMatchString(s) {
  if (s === '') throw new Error('EINVAL: empty match string');
  if (s === '=') throw new Error('EINVAL: missing field name');
  if (s.startsWith('=')) throw new Error('EINVAL: field name cannot start with =');

  const eq = s.indexOf('=');
  if (eq < 0) throw new Error('EINVAL: missing = separator');

  const field = s.slice(0, eq);
  if (field === '') throw new Error('EINVAL: empty field name');
  if (field.length > 64) throw new Error('EINVAL: field name too long');
  if (field[0] >= '0' && field[0] <= '9') throw new Error(`EINVAL: invalid field name "${field}"`);
  for (let i = 0; i < field.length; i++) {
    const c = field.charCodeAt(i);
    if (c !== 0x5f && !(c >= 0x41 && c <= 0x5a) && !(c >= 0x30 && c <= 0x39)) {
      throw new Error(`EINVAL: invalid field name "${field}"`);
    }
  }
  return Buffer.from(s, 'binary');
}
