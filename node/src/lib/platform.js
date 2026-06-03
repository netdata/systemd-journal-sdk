import { randomFillSync } from 'node:crypto';
import { safeReadFileSync } from './fs-safe.js';

export const UNKNOWN_PROCESS_START_TIME = 'unavailable';

export function readHostBootIdText() {
  if (process.platform !== 'linux') return '';
  try {
    const text = safeReadFileSync('/proc/sys/kernel/random/boot_id', 'utf8').trim().toLowerCase();
    if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(text)) {
      return text;
    }
  } catch {
    // Boot IDs are a Linux convenience; callers have portable fallbacks.
  }
  return '';
}

export function readHostBootId() {
  const text = readHostBootIdText().replaceAll('-', '');
  if (!/^[0-9a-f]{32}$/.test(text)) return null;
  return Buffer.from(text, 'hex');
}

export function createLockOwner() {
  return {
    pid: process.pid,
    bootId: readHostBootIdText(),
    startTime: processStartTime(process.pid) || UNKNOWN_PROCESS_START_TIME,
    ownerId: randomOwnerId(),
    platform: process.platform,
  };
}

export function lockOwnerIsActive(owner, deps = {}) {
  const bootId = Object.hasOwn(deps, 'bootId') ? deps.bootId : readHostBootIdText();
  if (owner.bootId && bootId && owner.bootId !== bootId) return false;

  const processStart = deps.processStartTime || processStartTime;
  const startTime = processStart(owner.pid);
  if (
    startTime !== null &&
    startTime !== undefined &&
    owner.startTime &&
    owner.startTime !== UNKNOWN_PROCESS_START_TIME &&
    String(startTime) !== owner.startTime
  ) {
    return false;
  }

  const processAlive = deps.processAlive || processIsAlive;
  const alive = processAlive(owner.pid);
  return alive !== false;
}

export function processStartTime(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return null;
  if (process.platform !== 'linux') return null;
  try {
    return parseLinuxProcStatStartTime(safeReadFileSync(`/proc/${pid}/stat`, 'utf8'));
  } catch {
    return null;
  }
}

export function parseLinuxProcStatStartTime(text) {
  const end = text.lastIndexOf(')');
  if (end < 0) return null;
  const fields = text.slice(end + 2).trim().split(/\s+/);
  if (fields.length < 20) return null;
  return fields[19] || null;
}

export function processIsAlive(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error && error.code === 'ESRCH') return false;
    if (error && error.code === 'EPERM') return true;
    return null;
  }
}

function randomOwnerId() {
  const buf = Buffer.alloc(16);
  randomFillSync(buf);
  return buf.toString('hex');
}
