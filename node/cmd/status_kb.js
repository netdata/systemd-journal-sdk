import { readFileSync } from 'node:fs';

const PROCESS_STATUS_KB_SETTERS = new Map([
  ['VmSize', (out, value) => { out.VmSize_kb = value; }],
  ['VmPeak', (out, value) => { out.VmPeak_kb = value; }],
  ['VmRSS', (out, value) => { out.VmRSS_kb = value; }],
  ['VmHWM', (out, value) => { out.VmHWM_kb = value; }],
  ['RssAnon', (out, value) => { out.RssAnon_kb = value; }],
  ['RssFile', (out, value) => { out.RssFile_kb = value; }],
  ['RssShmem', (out, value) => { out.RssShmem_kb = value; }],
  ['VmData', (out, value) => { out.VmData_kb = value; }],
  ['VmStk', (out, value) => { out.VmStk_kb = value; }],
  ['VmExe', (out, value) => { out.VmExe_kb = value; }],
  ['VmLib', (out, value) => { out.VmLib_kb = value; }],
  ['VmPTE', (out, value) => { out.VmPTE_kb = value; }],
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
  for (const line of lines) {
    const idx = line.indexOf(':');
    if (idx < 0) continue;
    const key = line.slice(0, idx);
    if (!PROCESS_STATUS_KB_SETTERS.has(key)) continue;
    const parts = line.slice(idx + 1).trim().split(/\s+/);
    const value = parts.length > 0 ? Number(parts[0]) : NaN;
    if (Number.isFinite(value)) assignStatusKb(out, key, value);
  }
  return out;
}

function assignStatusKb(out, key, value) {
  const setter = PROCESS_STATUS_KB_SETTERS.get(key);
  if (setter) setter(out, value);
}
