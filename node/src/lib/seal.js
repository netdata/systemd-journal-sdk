// Forward Secure Sealing support for journal writers.

import { createHmac } from 'node:crypto';
import {
  fsprgGenMK, fsprgGenState0, fsprgGetEpoch, fsprgEvolve, fsprgGetKey,
  RECOMMENDED_SECPAR, RECOMMENDED_SEEDLEN,
} from './fss.js';

export const TAG_LENGTH = 256 / 8;
export const OBJECT_TYPE_TAG = 7;
export const COMPATIBLE_SEALED = 1 << 0;
export const COMPATIBLE_SEALED_CONTINUOUS = 1 << 2;

export class SealOptions {
  constructor(seed, intervalUsec, startUsec) {
    if (seed.length !== RECOMMENDED_SEEDLEN) {
      throw new Error(`seal seed must be ${RECOMMENDED_SEEDLEN} bytes`);
    }
    this.seed = Buffer.from(seed);
    this.intervalUsec = intervalUsec;
    this.startUsec = startUsec;
  }
}

export class SealState {
  constructor(opts) {
    const { msk, mpk } = fsprgGenMK(opts.seed, RECOMMENDED_SECPAR);
    this.fsprgState = fsprgGenState0(mpk, opts.seed);
    this.msk = msk;
    this.seed = Buffer.from(opts.seed);
    this.interval = BigInt(opts.intervalUsec);
    this.start = BigInt(opts.startUsec);
    this.hmac = null;
    this.hmacRunning = false;
  }

  getEpoch() {
    return fsprgGetEpoch(this.fsprgState);
  }

  getGoalEpoch(realtime) {
    if (this.start === 0n || this.interval === 0n) {
      throw new Error('FSS start or interval not set');
    }
    const rt = BigInt(realtime);
    if (rt < this.start) {
      throw new Error('realtime before FSS start');
    }
    return (rt - this.start) / this.interval;
  }

  needEvolve(realtime) {
    const goal = this.getGoalEpoch(realtime);
    const epoch = this.getEpoch();
    if (epoch > goal) {
      throw new Error(`FSS epoch ${epoch} > goal ${goal}`);
    }
    return epoch !== goal;
  }

  evolveState() {
    this.fsprgState = fsprgEvolve(this.fsprgState);
  }

  hmacStart() {
    if (this.hmacRunning) return;
    const key = fsprgGetKey(this.fsprgState, TAG_LENGTH, 0);
    this.hmac = createHmac('sha256', key);
    this.hmacRunning = true;
  }

  hmacWrite(data) {
    this.hmacStart();
    this.hmac.update(data);
  }

  hmacReset() {
    this.hmacRunning = false;
    this.hmac = null;
  }

  hmacSum() {
    return this.hmac.digest();
  }
}
