import { readFileSync } from 'node:fs';

const PROCESS_STATUS_KB_KEYS = new Set([
  'VmSize', 'VmPeak', 'VmRSS', 'VmHWM', 'RssAnon', 'RssFile',
  'RssShmem', 'VmData', 'VmStk', 'VmExe', 'VmLib', 'VmPTE',
]);

export function processStatusKb() {
  const status = readProcessStatus();
  return status === null ? {} : parseStatusKb(status);
}

function readProcessStatus() {
  try {
    return readFileSync('/proc/self/status', 'utf8');
  } catch {
    return null;
  }
}

function parseStatusKb(status) {
  const out = {};
  const lines = status.split('\n');
  for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
    const line = lines[lineIndex];
    const idx = line.indexOf(':');
    if (idx < 0) continue;
    const key = line.slice(0, idx);
    if (!PROCESS_STATUS_KB_KEYS.has(key)) continue;
    const parts = line.slice(idx + 1).trim().split(/\s+/);
    const value = parts.length > 0 ? Number(parts[0]) : NaN;
    if (Number.isFinite(value)) Reflect.set(out, key + '_kb', value);
  }
  return out;
}
