import { stringToUUID } from './binary.js';

export function dedupeEntryItems(items) {
  items.sort((a, b) => (a.offset < b.offset ? -1 : a.offset > b.offset ? 1 : 0));
  const [firstItem, ...remainingItems] = items;
  const deduped = [firstItem];
  let lastOffset = firstItem.offset;
  for (const item of remainingItems) {
    if (item.offset !== lastOffset) {
      deduped.push(item);
      lastOffset = item.offset;
    }
  }
  return deduped;
}

export function normalizeFileMode(value, defaultMode) {
  if (value === undefined || value === null) return defaultMode;
  if (!Number.isInteger(value) || value < 0 || value > 0o777) {
    throw new Error(`invalid journal file mode: ${value}`);
  }
  return value;
}

export function normalizeLivePublishEveryEntries(value) {
  if (value === undefined || value === null) return 1;
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`invalid livePublishEveryEntries: ${value}`);
  }
  return value;
}

export function uuidOption(value, label) {
  if (value === undefined || value === null) return null;
  let out;
  if (typeof value === 'string') {
    const clean = value.trim().replaceAll('-', '');
    if (!/^[0-9a-fA-F]{32}$/.test(clean)) throw new Error(`${label} must be 16 bytes or 32 hex characters`);
    out = stringToUUID(clean);
  } else {
    out = Buffer.from(value);
  }
  if (out.length !== 16) throw new Error(`${label} must be 16 bytes or 32 hex characters`);
  return out;
}
