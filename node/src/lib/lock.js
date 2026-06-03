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
import { createLockOwner, lockOwnerIsActive } from './platform.js';

const LOCK_VERSION = 'systemd-journal-sdk-lock-v1';
const STALE_GRACE_MS = 2000;

export class WriterLock {
  constructor(path, owner) {
    this.path = path;
    this.owner = owner;
  }

  static acquire(journalPath) {
    const lockPath = `${journalPath}.lock`;
    const owner = createLockOwner();

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
    if (sameOwner(owner, this.owner)) {
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
    `owner_id=${owner.ownerId}`,
    `platform=${owner.platform}`,
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

  if (!lockOwnerIsActive(owner)) return { stale: true, holder: `stale pid ${owner.pid}` };
  return { stale: false, holder: `pid ${owner.pid}` };
}

function readLockOwner(path) {
  const lines = readFileSync(path, 'utf8').trim().split('\n');
  if (lines.length < 4 || lines[0] !== LOCK_VERSION) {
    throw new Error('invalid lock metadata');
  }
  const owner = parseLockOwnerLines(lines.slice(1));
  validateLockOwner(owner);
  return owner;
}

function parseLockOwnerLines(lines) {
  const owner = {};
  for (const line of lines) {
    const index = line.indexOf('=');
    if (index < 0) continue;
    assignLockOwnerField(owner, line.slice(0, index), line.slice(index + 1));
  }
  return owner;
}

function assignLockOwnerField(owner, key, value) {
  if (key === 'pid') owner.pid = Number.parseInt(value, 10);
  if (key === 'boot_id') owner.bootId = value;
  if (key === 'start_time') owner.startTime = value;
  if (key === 'owner_id') owner.ownerId = value;
  if (key === 'platform') owner.platform = value;
}

function validateLockOwner(owner) {
  if (!Number.isSafeInteger(owner.pid) || owner.pid <= 0 || owner.startTime === undefined) {
    throw new Error('incomplete lock metadata');
  }
  if (owner.bootId === undefined) owner.bootId = '';
}

function sameOwner(left, right) {
  if (left.ownerId || right.ownerId) {
    return left.ownerId !== undefined && left.ownerId === right.ownerId;
  }
  return left.pid === right.pid && left.bootId === right.bootId && left.startTime === right.startTime;
}
