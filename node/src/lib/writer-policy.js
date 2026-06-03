export const FIELD_NAME_POLICY_JOURNALD = 'journald';
export const FIELD_NAME_POLICY_RAW = 'raw';
export const FIELD_NAME_POLICY_JOURNAL_APP = 'journal-app';

export function normalizeFieldNamePolicy(value) {
  if (value === undefined || value === null || value === '') return FIELD_NAME_POLICY_JOURNALD;
  if (value === FIELD_NAME_POLICY_JOURNALD) return FIELD_NAME_POLICY_JOURNALD;
  if (value === FIELD_NAME_POLICY_RAW) return FIELD_NAME_POLICY_RAW;
  if (value === FIELD_NAME_POLICY_JOURNAL_APP) return FIELD_NAME_POLICY_JOURNAL_APP;
  throw new Error(`unsupported field name policy: ${value}`);
}

export function writerPolicyForLogPolicy(policy) {
  // Log applies JOURNAL-APP filtering before injecting SDK-owned protected fields.
  // The underlying writer must therefore accept those trusted metadata fields.
  return normalizeFieldNamePolicy(policy) === FIELD_NAME_POLICY_RAW
    ? FIELD_NAME_POLICY_RAW
    : FIELD_NAME_POLICY_JOURNALD;
}

export function prepareFieldsForPolicy(fields, policy) {
  const normalized = normalizeFieldNamePolicy(policy);
  if (!Array.isArray(fields) || fields.length === 0) throw new Error('empty entry');
  if (normalized === FIELD_NAME_POLICY_JOURNAL_APP) {
    const filtered = fields.filter((field) => {
      try {
        validateFieldNameForPolicy(field.name, normalized);
        return true;
      } catch {
        return false;
      }
    });
    if (filtered.length === 0) throw new Error('empty entry');
    return filtered;
  }
  for (const field of fields) validateFieldNameForPolicy(field.name, normalized);
  return fields;
}

export function prepareRawPayloadsForPolicy(payloads, policy) {
  const normalized = normalizeFieldNamePolicy(policy);
  if (!Array.isArray(payloads) || payloads.length === 0) throw new Error('empty entry');
  const prepared = payloads.map(rawPayloadBytes);
  for (const payload of prepared) {
    const separator = payload.indexOf(0x3d);
    if (separator <= 0) throw new Error('invalid raw field payload: missing field name separator');
  }
  if (normalized === FIELD_NAME_POLICY_JOURNAL_APP) {
    const filtered = prepared.filter((payload) => {
      try {
        validateFieldNameForPolicy(payload.subarray(0, payload.indexOf(0x3d)), normalized);
        return true;
      } catch {
        return false;
      }
    });
    if (filtered.length === 0) throw new Error('empty entry');
    return filtered;
  }
  for (const payload of prepared) {
    validateFieldNameForPolicy(payload.subarray(0, payload.indexOf(0x3d)), normalized);
  }
  return prepared;
}

export function validateFieldNameForPolicy(name, policy = FIELD_NAME_POLICY_JOURNALD) {
  const normalized = normalizeFieldNamePolicy(policy);
  if (normalized === FIELD_NAME_POLICY_RAW) return validateRawFieldName(name);
  return validateJournaldFieldName(name, normalized === FIELD_NAME_POLICY_JOURNALD);
}

function rawPayloadBytes(payload) {
  if (Buffer.isBuffer(payload)) return payload;
  if (payload instanceof Uint8Array) return Buffer.from(payload);
  if (typeof payload === 'string') return Buffer.from(payload, 'utf8');
  return Buffer.from(payload);
}

export function fieldNameBytes(name) {
  if (Buffer.isBuffer(name)) return name;
  if (name instanceof Uint8Array) return Buffer.from(name);
  return Buffer.from(String(name), 'utf8');
}

function fieldNameForError(name) {
  if (Buffer.isBuffer(name) || name instanceof Uint8Array) return Buffer.from(name).toString('utf8');
  return String(name);
}

function validateRawFieldName(name) {
  const bytes = fieldNameBytes(name);
  if (bytes.length === 0) throw new Error('invalid field name: empty');
  for (let i = 0; i < bytes.length; i++) {
    if (bytes.readUInt8(i) === 0x3d) throw new Error(`invalid field name: contains '=': ${fieldNameForError(name)}`);
  }
}

function validateJournaldFieldName(name, allowProtected) {
  const bytes = fieldNameBytes(name);
  const display = fieldNameForError(name);
  if (bytes.length === 0) throw new Error('invalid field name: empty');
  if (bytes.length > 64) throw new Error(`invalid field name: too long (${bytes.length})`);
  if (!allowProtected && bytes[0] === 0x5f) throw new Error(`invalid field name: protected: ${display}`);
  if (bytes[0] >= 0x30 && bytes[0] <= 0x39) throw new Error(`invalid field name: starts with digit: ${display}`);
  validateJournaldFieldNameBytes(bytes, display);
}

function validateJournaldFieldNameBytes(bytes, display) {
  for (let i = 0; i < bytes.length; i++) {
    if (!isJournaldFieldNameByte(bytes.readUInt8(i))) {
      throw new Error(`invalid field name: bad char at ${i}: ${display}`);
    }
  }
}

function isJournaldFieldNameByte(c) {
  return c === 0x5f || (c >= 0x41 && c <= 0x5a) || (c >= 0x30 && c <= 0x39);
}
