import {
  closeSync,
  constants,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  statSync,
  unlinkSync,
  writeSync,
  fsyncSync,
} from 'node:fs';
import { dirname } from 'node:path';

const LOCK_VERSION = 'systemd-journal-sdk-lock-v1';
const STALE_GRACE_MS = 2000;

export class WriterLock {
  constructor(path, owner) {
    this.path = path;
    this.owner = owner;
  }

  static acquire(journalPath) {
    const lockPath = `${journalPath}.lock`;
    const owner = currentOwner();

    for (;;) {
      mkdirSync(dirname(lockPath), { recursive: true, mode: 0o750 });
      let fd;
      try {
        fd = openSync(lockPath, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, 0o600);
      } catch (error) {
        if (error.code !== 'EEXIST') throw error;
        const { stale, holder } = lockFileIsStale(lockPath);
        if (!stale) throw new Error(`journal writer lock held by ${holder}`);
        try {
          unlinkSync(lockPath);
        } catch (unlinkError) {
          if (unlinkError.code !== 'ENOENT') throw unlinkError;
        }
        continue;
      }

      try {
        writeLockOwner(fd, owner);
        closeSync(fd);
        return new WriterLock(lockPath, owner);
      } catch (error) {
        try {
          if (fd !== undefined) closeSync(fd);
        } catch {
          // Preserve the original error.
        }
        try {
          unlinkSync(lockPath);
        } catch {
          // Best effort cleanup.
        }
        throw error;
      }
    }
  }

  release() {
    if (!this.path) return;
    let owner;
    try {
      owner = readLockOwner(this.path);
    } catch (error) {
      if (error.code === 'ENOENT') {
        this.path = null;
        return;
      }
      throw error;
    }
    if (sameOwner(owner, currentOwner())) {
      try {
        unlinkSync(this.path);
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
    }
    this.path = null;
  }
}

function writeLockOwner(fd, owner) {
  const text = [
    LOCK_VERSION,
    `pid=${owner.pid}`,
    `boot_id=${owner.bootId}`,
    `start_time=${owner.startTime}`,
    '',
  ].join('\n');
  writeSync(fd, text, 0, 'utf8');
  fsyncSync(fd);
}

function lockFileIsStale(path) {
  let owner;
  try {
    owner = readLockOwner(path);
  } catch {
    if (existsSync(path) && Date.now() - statSync(path).mtimeMs <= STALE_GRACE_MS) {
      return { stale: false, holder: 'partially-created lock' };
    }
    return { stale: true, holder: 'malformed stale lock' };
  }

  if (owner.bootId !== bootId()) {
    return { stale: true, holder: `pid ${owner.pid} from previous boot` };
  }
  const startTime = processStartTime(owner.pid);
  if (startTime === null || startTime !== owner.startTime) {
    return { stale: true, holder: `stale pid ${owner.pid}` };
  }
  return { stale: false, holder: `pid ${owner.pid}` };
}

function currentOwner() {
  const startTime = processStartTime(process.pid);
  if (startTime === null) {
    throw new Error(`cannot parse process start time for pid ${process.pid}`);
  }
  return {
    pid: process.pid,
    bootId: bootId(),
    startTime,
  };
}

function bootId() {
  try {
    return readFileSync('/proc/sys/kernel/random/boot_id', 'utf8').trim();
  } catch {
    return '';
  }
}

function processStartTime(pid) {
  try {
    const text = readFileSync(`/proc/${pid}/stat`, 'utf8');
    const end = text.lastIndexOf(')');
    if (end < 0) return null;
    const fields = text.slice(end + 2).trim().split(/\s+/);
    if (fields.length < 20) return null;
    return fields[19];
  } catch {
    return null;
  }
}

function readLockOwner(path) {
  const lines = readFileSync(path, 'utf8').trim().split('\n');
  if (lines.length < 4 || lines[0] !== LOCK_VERSION) {
    throw new Error('invalid lock metadata');
  }
  const owner = {};
  for (const line of lines.slice(1)) {
    const index = line.indexOf('=');
    if (index < 0) continue;
    const key = line.slice(0, index);
    const value = line.slice(index + 1);
    if (key === 'pid') owner.pid = Number.parseInt(value, 10);
    if (key === 'boot_id') owner.bootId = value;
    if (key === 'start_time') owner.startTime = value;
  }
  if (!Number.isSafeInteger(owner.pid) || owner.pid <= 0 || !owner.startTime) {
    throw new Error('incomplete lock metadata');
  }
  return owner;
}

function sameOwner(left, right) {
  return left.pid === right.pid && left.bootId === right.bootId && left.startTime === right.startTime;
}
