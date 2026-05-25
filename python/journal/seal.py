"""Forward Secure Sealing support for journal writers.

Implements file-format sealing with deterministic synthetic keys,
matching systemd v260.1 HMAC byte ranges and tag object layout.
"""

import struct
import hmac
import hashlib
from .fss import gen_mk, gen_state0, get_epoch, evolve, RECOMMENDED_SECPAR, RECOMMENDED_SEEDLEN

TAG_LENGTH = 256 // 8
OBJECT_TYPE_TAG = 7
COMPATIBLE_SEALED = 1 << 0
COMPATIBLE_SEALED_CONTINUOUS = 1 << 2


class SealOptions:
    """Configures Forward Secure Sealing."""

    def __init__(self, seed, interval_usec, start_usec):
        if len(seed) != RECOMMENDED_SEEDLEN:
            raise ValueError(f'seal seed must be {RECOMMENDED_SEEDLEN} bytes')
        self.seed = bytes(seed)
        self.interval_usec = interval_usec
        self.start_usec = start_usec


class SealState:
    """Per-writer FSS+HMAC state."""

    def __init__(self, opts):
        self._msk, self._mpk = gen_mk(opts.seed, RECOMMENDED_SECPAR)
        self._fsprg_state = gen_state0(self._mpk, opts.seed)
        self._seed = opts.seed
        self._interval = opts.interval_usec
        self._start = opts.start_usec
        self._hmac = None
        self._hmac_running = False

    def get_epoch(self):
        return get_epoch(self._fsprg_state)

    def get_goal_epoch(self, realtime):
        if self._start == 0 or self._interval == 0:
            raise ValueError('FSS start or interval not set')
        if realtime < self._start:
            raise ValueError('realtime before FSS start')
        return (realtime - self._start) // self._interval

    def need_evolve(self, realtime):
        goal = self.get_goal_epoch(realtime)
        epoch = self.get_epoch()
        if epoch > goal:
            raise ValueError(f'FSS epoch {epoch} > goal {goal}')
        return epoch != goal

    def evolve_state(self):
        self._fsprg_state = evolve(self._fsprg_state)

    def hmac_start(self):
        if self._hmac_running:
            return
        from .fss import get_key
        key = get_key(self._fsprg_state, TAG_LENGTH, 0)
        self._hmac = hmac.new(key, digestmod=hashlib.sha256)
        self._hmac_running = True

    def hmac_write(self, data):
        self.hmac_start()
        self._hmac.update(data)

    def hmac_reset(self):
        self._hmac_running = False
        self._hmac = None

    def hmac_sum(self):
        return self._hmac.digest()
