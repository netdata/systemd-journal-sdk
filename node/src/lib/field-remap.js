import { createHash } from 'node:crypto';

export const REMAPPING_MARKER = 'ND_REMAPPING';
const REMAPPED_PREFIX = 'ND_';
const MASK64 = (1n << 64n) - 1n;

const LOWERCASE = 0;
const UPPERCASE = 1;
const DOT = 2;
const UNDERSCORE = 3;
const HYPHEN = 4;

const TOKEN_LOWERCASE = 0;
const TOKEN_UPPERCASE = 1;
const TOKEN_CAPITALIZED = 2;

const FIELD_LOWERCASE = 0;
const FIELD_UPPERCASE = 1;
const FIELD_LOWER_CAMEL = 2;
const FIELD_UPPER_CAMEL = 3;
const FIELD_EMPTY = 4;

const SEP_DOT = 0;
const SEP_HYPHEN = 1;
const SEP_UNDERSCORE = 2;

export function isSystemdCompatibleLogFieldName(name) {
  if (typeof name !== 'string' || name.length === 0 || name.length > 64) return false;
  const first = name.charCodeAt(0);
  if (first < 0x41 || first > 0x5a) return false;
  for (let i = 0; i < name.length; i++) {
    const c = name.charCodeAt(i);
    if (c === 0x5f || (c >= 0x41 && c <= 0x5a) || (c >= 0x30 && c <= 0x39)) continue;
    return false;
  }
  return true;
}

export function remapFields(fields, registry) {
  const out = [];
  const mappings = [];
  const pending = new Map();

  for (const field of fields) {
    if (isSystemdCompatibleLogFieldName(field.name)) {
      out.push(field);
      continue;
    }

    let mapped = registry.get(field.name) || pending.get(field.name);
    if (!mapped) {
      mapped = encodeRemappedFieldName(field.name);
      pending.set(field.name, mapped);
      mappings.push({ original: field.name, mapped });
    }
    out.push({ ...field, name: mapped });
  }

  return { fields: out, mappings };
}

export function encodeRemappedFieldName(fieldName) {
  const bytes = Buffer.isBuffer(fieldName) ? Buffer.from(fieldName) : Buffer.from(String(fieldName), 'utf8');
  const encoded = rdpEncode(bytes);
  if (encoded === null) return md5Fallback(bytes);

  const compressed = hasChecksum(encoded)
    ? encoded.slice(0, 2) + compressRuns(encoded.slice(2))
    : compressRuns(encoded);

  let normalized = bytes.toString('utf8').toUpperCase().replace(/[.-]/g, '_');
  if (normalized.startsWith('RESOURCE_ATTRIBUTES_')) {
    normalized = `RA_${normalized.slice('RESOURCE_ATTRIBUTES_'.length)}`;
  } else if (normalized.startsWith('LOG_ATTRIBUTES_')) {
    normalized = `LA_${normalized.slice('LOG_ATTRIBUTES_'.length)}`;
  } else if (normalized.startsWith('LOG_BODY_')) {
    normalized = `LB_${normalized.slice('LOG_BODY_'.length)}`;
  }

  const result = `ND${compressed.toUpperCase()}_${normalized}`;
  if (result.length > 64) return md5Fallback(bytes);
  return result;
}

function md5Fallback(bytes) {
  return `${REMAPPED_PREFIX}${createHash('md5').update(bytes).digest('hex').toUpperCase()}`;
}

function rdpEncode(bytes) {
  const tokens = tokenize(bytes);
  if (tokens === null) return null;
  return encodeNodes(bytes.toString('utf8'), parseTokens(tokens));
}

function charKind(c) {
  if (c >= 0x61 && c <= 0x7a) return LOWERCASE;
  if ((c >= 0x41 && c <= 0x5a) || (c >= 0x30 && c <= 0x39)) return UPPERCASE;
  if (c === 0x2e) return DOT;
  if (c === 0x5f) return UNDERSCORE;
  if (c === 0x2d) return HYPHEN;
  return null;
}

function tokenize(bytes) {
  if (bytes.length === 0) return [];

  const tokens = [];
  let start = 0;
  let previous = null;
  let first = null;
  let hasLowercase = false;
  let hasUppercase = false;

  for (let i = 0; i < bytes.length; i++) {
    const current = charKind(bytes[i]);
    if (current === null) return null;
    if (previous === null) {
      first = current;
      previous = current;
      continue;
    }

    let shouldSplit = false;
    if (previous === DOT || previous === UNDERSCORE || previous === HYPHEN) {
      shouldSplit = true;
    } else if (current === DOT || current === UNDERSCORE || current === HYPHEN) {
      shouldSplit = true;
    } else if (previous === UPPERCASE && current === UPPERCASE) {
      if (i + 1 < bytes.length) {
        const next = charKind(bytes[i + 1]);
        if (next === null) return null;
        shouldSplit = next === LOWERCASE;
      }
    } else if (previous === LOWERCASE && current === LOWERCASE) {
      shouldSplit = false;
    } else if (previous === UPPERCASE && current === LOWERCASE) {
      shouldSplit = hasUppercase && hasLowercase;
    } else {
      shouldSplit = true;
    }

    if (shouldSplit) {
      tokens.push(createToken(first, hasLowercase, hasUppercase, start, i));
      start = i;
      first = current;
      hasLowercase = false;
      hasUppercase = false;
    } else if (current === LOWERCASE) {
      hasLowercase = true;
    } else if (current === UPPERCASE) {
      hasUppercase = true;
    }
    previous = current;
  }

  if (start < bytes.length) tokens.push(createToken(first, hasLowercase, hasUppercase, start, bytes.length));
  return tokens;
}

function createToken(first, hasLowercase, _hasUppercase, start, end) {
  if (first === LOWERCASE) return { word: true, kind: TOKEN_LOWERCASE, start, end };
  if (first === UPPERCASE) {
    return { word: true, kind: hasLowercase ? TOKEN_CAPITALIZED : TOKEN_UPPERCASE, start, end };
  }
  if (first === DOT) return { word: false, sep: SEP_DOT };
  if (first === HYPHEN) return { word: false, sep: SEP_HYPHEN };
  return { word: false, sep: SEP_UNDERSCORE };
}

function parseTokens(tokens) {
  const nodes = [];
  if (tokens.length > 0 && !tokens[0].word) nodes.push({ field: true, type: FIELD_EMPTY });

  let builder = null;
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (!token.word) {
      if (builder) {
        nodes.push({ field: true, type: builder.type });
        builder = null;
      }
      nodes.push({ field: false, sep: token.sep });
      if (i + 1 >= tokens.length || !tokens[i + 1].word) nodes.push({ field: true, type: FIELD_EMPTY });
      continue;
    }

    if (builder) {
      if (canAdd(builder.type, token.kind)) {
        builder.extended = true;
        continue;
      }
      if (builder.type === FIELD_LOWERCASE && !builder.extended && token.kind === TOKEN_CAPITALIZED) {
        builder.type = FIELD_LOWER_CAMEL;
        builder.extended = true;
        continue;
      }
      nodes.push({ field: true, type: builder.type });
    }
    builder = { type: fieldTypeForToken(token.kind), extended: false };
  }

  if (builder) nodes.push({ field: true, type: builder.type });
  return nodes;
}

function canAdd(fieldType, tokenType) {
  return (fieldType === FIELD_LOWERCASE && tokenType === TOKEN_LOWERCASE) ||
    (fieldType === FIELD_UPPERCASE && tokenType === TOKEN_UPPERCASE) ||
    (fieldType === FIELD_LOWER_CAMEL && tokenType === TOKEN_CAPITALIZED) ||
    (fieldType === FIELD_UPPER_CAMEL && tokenType === TOKEN_CAPITALIZED);
}

function fieldTypeForToken(tokenType) {
  if (tokenType === TOKEN_LOWERCASE) return FIELD_LOWERCASE;
  if (tokenType === TOKEN_UPPERCASE) return FIELD_UPPERCASE;
  return FIELD_UPPER_CAMEL;
}

function encodeNodes(source, nodes) {
  const hasCamel = nodes.some((node) =>
    node.field && (node.type === FIELD_LOWER_CAMEL || node.type === FIELD_UPPER_CAMEL));
  let out = hasCamel ? checksum(source) : '';

  for (let i = 0; i < nodes.length;) {
    const node = nodes[i];
    if (!node.field) {
      i++;
      continue;
    }
    const nextIsSeparator = i + 1 < nodes.length && !nodes[i + 1].field;
    const nextIsField = i + 1 < nodes.length && nodes[i + 1].field;
    const sep = nextIsSeparator ? nodes[i + 1].sep : SEP_DOT;
    out += String.fromCharCode(pairChar(node.type, nextIsSeparator, nextIsField, sep));
    i += nextIsSeparator ? 2 : 1;
  }

  return out;
}

function pairChar(fieldType, nextIsSeparator, nextIsField, sep) {
  let base = 0x61;
  if (fieldType === FIELD_LOWER_CAMEL) base = 0x66;
  else if (fieldType === FIELD_UPPER_CAMEL) base = 0x6b;
  else if (fieldType === FIELD_UPPERCASE) base = 0x70;
  else if (fieldType === FIELD_EMPTY) base = 0x75;

  if (nextIsSeparator) {
    if (sep === SEP_DOT) return base;
    if (sep === SEP_UNDERSCORE) return base + 1;
    return base + 2;
  }
  if (nextIsField && fieldType !== FIELD_EMPTY) return base + 3;
  return base + 4;
}

function checksum(source) {
  const msg = Buffer.concat([Buffer.from(source, 'utf8'), Buffer.from([0xff])]);
  const hash = sipHash13Zero(msg);
  const first = Number((hash / 36n) % 36n);
  const second = Number(hash % 36n);
  return String.fromCharCode(checksumChar(first), checksumChar(second));
}

function checksumChar(index) {
  return index < 26 ? 0x41 + index : 0x30 + index - 26;
}

function hasChecksum(encoded) {
  if (encoded.length === 0) return false;
  const c = encoded.charCodeAt(0);
  return (c >= 0x41 && c <= 0x5a) || (c >= 0x30 && c <= 0x39);
}

function compressRuns(value) {
  let out = '';
  for (let i = 0; i < value.length;) {
    const ch = value[i];
    let count = 1;
    while (i + count < value.length && value[i + count] === ch) count++;
    if (count <= 2) {
      out += ch.repeat(count);
    } else {
      let remaining = count;
      while (remaining > 0) {
        if (remaining > 9) {
          out += `9${ch}`;
          remaining -= 9;
        } else if (remaining > 2) {
          out += `${remaining}${ch}`;
          remaining = 0;
        } else {
          out += ch.repeat(remaining);
          remaining = 0;
        }
      }
    }
    i += count;
  }
  return out;
}

function sipHash13Zero(msg) {
  let v0 = 0x736f6d6570736575n;
  let v1 = 0x646f72616e646f6dn;
  let v2 = 0x6c7967656e657261n;
  let v3 = 0x7465646279746573n;

  const round = () => {
    v0 = add64(v0, v1);
    v1 = rotl64(v1, 13) ^ v0;
    v0 = rotl64(v0, 32);
    v2 = add64(v2, v3);
    v3 = rotl64(v3, 16) ^ v2;
    v0 = add64(v0, v3);
    v3 = rotl64(v3, 21) ^ v0;
    v2 = add64(v2, v1);
    v1 = rotl64(v1, 17) ^ v2;
    v2 = rotl64(v2, 32);
  };

  let offset = 0;
  while (offset + 8 <= msg.length) {
    const m = msg.readBigUInt64LE(offset);
    v3 ^= m;
    round();
    v0 ^= m;
    offset += 8;
  }

  let b = BigInt(msg.length) << 56n;
  for (let i = 0; offset + i < msg.length; i++) {
    b |= BigInt(msg[offset + i]) << BigInt(8 * i);
  }

  v3 ^= b;
  round();
  v0 ^= b;
  v2 ^= 0xffn;
  round();
  round();
  round();

  return (v0 ^ v1 ^ v2 ^ v3) & MASK64;
}

function add64(a, b) {
  return (a + b) & MASK64;
}

function rotl64(value, bits) {
  const n = BigInt(bits);
  return ((value << n) | (value >> (64n - n))) & MASK64;
}
