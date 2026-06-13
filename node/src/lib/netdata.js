/**
 * Netdata function surface for the systemd journal SDK (Node.js port).
 *
 * This module is the Node.js equivalent of:
 * - rust/src/journal/src/netdata.rs
 * - python/journal/netdata.py
 *
 * Chunk 2a: constants, config, display scope/context, profiles,
 * field-by-field display transformations. Request handling, source
 * discovery, and envelope attach happen in chunk 2b.
 *
 * Pure ESM. No native addons. No external dependencies.
 */

// ---------------------------------------------------------------------------
// Source-type bit flags (mirror Rust L38-44)
// ---------------------------------------------------------------------------

export const NETDATA_SOURCE_TYPE_ALL = 1 << 0;
export const NETDATA_SOURCE_TYPE_LOCAL_ALL = 1 << 1;
export const NETDATA_SOURCE_TYPE_REMOTE_ALL = 1 << 2;
export const NETDATA_SOURCE_TYPE_LOCAL_SYSTEM = 1 << 3;
export const NETDATA_SOURCE_TYPE_LOCAL_USER = 1 << 4;
export const NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE = 1 << 5;
export const NETDATA_SOURCE_TYPE_LOCAL_OTHER = 1 << 6;

// ---------------------------------------------------------------------------
// Accepted request parameter names (mirror Rust L54-71)
// ---------------------------------------------------------------------------

export const NETDATA_ACCEPTED_PARAMS = [
  'info',
  '__logs_sources',
  'after',
  'before',
  'anchor',
  'direction',
  'last',
  'query',
  'facets',
  'histogram',
  'if_modified_since',
  'data_only',
  'delta',
  'tail',
  'sampling',
  'slice',
];

// ---------------------------------------------------------------------------
// Default view keys (22) and facets (58) — copied byte-exact from
// rust/src/journal/src/netdata.rs L73-157. Order is significant:
// it drives UI column order.
// ---------------------------------------------------------------------------

export const SYSTEMD_DEFAULT_VIEW_KEYS = [
  '_HOSTNAME',
  'ND_JOURNAL_PROCESS',
  'MESSAGE',
  'PRIORITY',
  'SYSLOG_FACILITY',
  'ERRNO',
  'ND_JOURNAL_FILE',
  'SYSLOG_IDENTIFIER',
  'UNIT',
  'USER_UNIT',
  'MESSAGE_ID',
  '_BOOT_ID',
  '_SYSTEMD_OWNER_UID',
  '_UID',
  'OBJECT_SYSTEMD_OWNER_UID',
  'OBJECT_UID',
  '_GID',
  'OBJECT_GID',
  '_CAP_EFFECTIVE',
  '_AUDIT_LOGINUID',
  'OBJECT_AUDIT_LOGINUID',
  '_SOURCE_REALTIME_TIMESTAMP',
];

export const SYSTEMD_DEFAULT_FACETS = [
  '_HOSTNAME',
  'PRIORITY',
  'SYSLOG_FACILITY',
  'ERRNO',
  'SYSLOG_IDENTIFIER',
  'UNIT',
  'USER_UNIT',
  'MESSAGE_ID',
  '_BOOT_ID',
  '_SYSTEMD_OWNER_UID',
  '_UID',
  'OBJECT_SYSTEMD_OWNER_UID',
  'OBJECT_UID',
  '_GID',
  'OBJECT_GID',
  '_AUDIT_LOGINUID',
  'OBJECT_AUDIT_LOGINUID',
  'CODE_FILE',
  '_SYSTEMD_UNIT',
  '_SYSTEMD_USER_SLICE',
  'CODE_FUNC',
  '_TRANSPORT',
  '_COMM',
  '_RUNTIME_SCOPE',
  '_MACHINE_ID',
  '_SYSTEMD_SLICE',
  'UNIT_RESULT',
  '_SYSTEMD_CGROUP',
  '_EXE',
  '_SYSTEMD_USER_UNIT',
  '_SYSTEMD_SESSION',
  'COREDUMP_CGROUP',
  'COREDUMP_USER_UNIT',
  'COREDUMP_UNIT',
  'COREDUMP_SIGNAL_NAME',
  'COREDUMP_COMM',
  '_UDEV_DEVNODE',
  '_KERNEL_SUBSYSTEM',
  'OBJECT_EXE',
  'OBJECT_SYSTEMD_CGROUP',
  'OBJECT_COMM',
  'OBJECT_SYSTEMD_UNIT',
  'OBJECT_SYSTEMD_USER_UNIT',
  '_SELINUX_CONTEXT',
  '_NAMESPACE',
  'OBJECT_SYSTEMD_SESSION',
  'CONTAINER_ID',
  'CONTAINER_NAME',
  'CONTAINER_TAG',
  'IMAGE_NAME',
  'ND_NIDL_NODE',
  'ND_NIDL_CONTEXT',
  'ND_LOG_SOURCE',
  'ND_ALERT_NAME',
  'ND_ALERT_CLASS',
  'ND_ALERT_COMPONENT',
  'ND_ALERT_TYPE',
  'ND_ALERT_STATUS',
];

// ---------------------------------------------------------------------------
// Internal behaviour constants (mirror Rust L20-37)
// ---------------------------------------------------------------------------

export const DEFAULT_FUNCTION_NAME = 'systemd-journal';
export const DEFAULT_SOURCE_SELECTOR_NAME = 'Journal Sources';
export const DEFAULT_SOURCE_SELECTOR_HELP = 'Select the logs source to query';
export const DEFAULT_ITEMS_TO_RETURN = 200;
export const DEFAULT_TIME_WINDOW_SECONDS = 3600;
export const DEFAULT_ITEMS_SAMPLING = 1_000_000;
export const DEFAULT_HISTOGRAM_BUCKETS = 150;
export const EFFECTIVELY_DISABLED_TIMEOUT_SECONDS = 100 * 365 * 24 * 60 * 60;
export const NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC = 5_000_000;
export const NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC = 2 * 60 * 1_000_000;
export const NETDATA_FACET_MAX_VALUE_LENGTH = 8192;
export const NETDATA_MAX_DIRECTORY_SCAN_DEPTH = 64;
export const NETDATA_MAX_DIRECTORY_SCAN_COUNT = 8192;
export const DATA_ONLY_CHECK_EVERY_ROWS = 128;

// ---------------------------------------------------------------------------
// DisplayScope (mirror Rust L205-210)
// ---------------------------------------------------------------------------

export const DisplayScope = {
  Data: 'data',
  Facet: 'facet',
  Histogram: 'histogram',
};

// ---------------------------------------------------------------------------
// DisplayContext — reusable per-function display state
// (mirror Rust L198-203)
//
// Three caches: boot_first_realtime, uid_display, gid_display.
// Reusable across requests so that uid/gid name lookups and
// boot-id timestamps are computed at most once per distinct value.
// ---------------------------------------------------------------------------

export class DisplayContext {
  constructor() {
    this._bootFirstRealtime = new Map();
    this._uidCache = new Map();
    this._gidCache = new Map();
  }

  registerBootFirstRealtime(bootIdBytes, realtimeUsec) {
    this._bootFirstRealtime.set(
      typeof bootIdBytes === 'string' ? bootIdBytes : new TextDecoder().decode(bootIdBytes),
      realtimeUsec,
    );
  }
}

// ---------------------------------------------------------------------------
// Config (mirror Rust NetdataFunctionConfig L159-196)
// ---------------------------------------------------------------------------

export class NetdataFunctionConfig {
  constructor({
    functionName = DEFAULT_FUNCTION_NAME,
    sourceSelectorName = DEFAULT_SOURCE_SELECTOR_NAME,
    sourceSelectorHelp = DEFAULT_SOURCE_SELECTOR_HELP,
    defaultFacets = [...SYSTEMD_DEFAULT_FACETS],
    defaultViewKeys = [...SYSTEMD_DEFAULT_VIEW_KEYS],
    defaultHistogram = 'PRIORITY',
    readerOptions = null,
    explorerStrategy = null,
  } = {}) {
    this.functionName = functionName;
    this.sourceSelectorName = sourceSelectorName;
    this.sourceSelectorHelp = sourceSelectorHelp;
    this.defaultFacets = defaultFacets;
    this.defaultViewKeys = defaultViewKeys;
    this.defaultHistogram = defaultHistogram;
    this.readerOptions = readerOptions;
    this.explorerStrategy = explorerStrategy;
  }

  static systemdJournal() {
    return new NetdataFunctionConfig();
  }

  backfillDefaults() {
    if (!this.sourceSelectorName) {
      this.sourceSelectorName = DEFAULT_SOURCE_SELECTOR_NAME;
    }
    if (!this.sourceSelectorHelp) {
      this.sourceSelectorHelp = DEFAULT_SOURCE_SELECTOR_HELP;
    }
    return this;
  }
}

// ---------------------------------------------------------------------------
// Profile base class + two concrete profiles
// (mirror Rust L212-265)
// ---------------------------------------------------------------------------

export class NetdataFunctionProfile {
  fieldDisplayValue(_context, _scope, _field, value) {
    return bytesToText(value);
  }

  facetOptionName(context, field, rawValue) {
    const rendered = this.fieldDisplayValue(context, DisplayScope.Facet, field, rawValue);
    if (typeof rendered === 'string') return rendered;
    return String(rendered);
  }

  rowOptions(fields) {
    const priorityValues = fields.PRIORITY;
    if (priorityValues && priorityValues.length > 0) {
      return { severity: priorityToRowSeverity(priorityValues[0]) };
    }
    return { severity: 'normal' };
  }
}

export class SystemdJournalProfile extends NetdataFunctionProfile {
  fieldDisplayValue(context, scope, field, value) {
    return systemdFieldDisplayValue(context, scope, field, value, false);
  }
}

export class SystemdJournalPluginProfile extends NetdataFunctionProfile {
  // UID/GID resolution: Rust resolves via getpwuid_r / getgrgid_r on unix
  // (netdata.rs:4415-4443), falling back to the raw numeric string when
  // resolution fails or on non-unix (netdata.rs:4441-4443,
  // netdata.rs:4395: unwrap_or_else(|| raw.to_string())).
  //
  // Node has no stdlib pwd/grp and native addons are forbidden, so
  // this profile always uses the raw numeric value, which is exactly
  // what Rust emits when resolution fails. The three-peer comparator
  // will adjudicate; a future chunk may add a pure-JS /etc/passwd
  // parse gated to the plugin profile, mirroring NSS file resolution.

  fieldDisplayValue(context, scope, field, value) {
    return systemdFieldDisplayValue(context, scope, field, value, true);
  }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

function bytesToText(value) {
  if (typeof value === 'string') return value;
  if (value instanceof Uint8Array || ArrayBuffer.isView(value) || value instanceof ArrayBuffer) {
    const buf = value instanceof ArrayBuffer ? new Uint8Array(value) : new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    return new TextDecoder('utf-8', { fatal: false }).decode(buf);
  }
  return String(value);
}

const UID_FIELDS = new Set([
  '_UID',
  '_SYSTEMD_OWNER_UID',
  'OBJECT_SYSTEMD_OWNER_UID',
  'OBJECT_UID',
  '_AUDIT_LOGINUID',
  'OBJECT_AUDIT_LOGINUID',
]);

const GID_FIELDS = new Set(['_GID', 'OBJECT_GID']);

function tryInt(raw) {
  if (typeof raw !== 'string') return null;
  const n = Number(raw);
  if (!Number.isFinite(n) || n < 0 || !/^-?\d+$/.test(raw)) return null;
  return n;
}

const PRIORITY_NAMES = {
  0: 'panic',
  1: 'alert',
  2: 'critical',
  3: 'error',
  4: 'warning',
  5: 'notice',
  6: 'info',
  7: 'debug',
};

function priorityName(raw) {
  const n = tryInt(raw);
  if (n === null) return null;
  return PRIORITY_NAMES[n] ?? null;
}

export function priorityToRowSeverity(raw) {
  const text = bytesToText(raw);
  const n = tryInt(text);
  if (n === null) return 'normal';
  if (n <= 3) return 'critical';
  if (n === 4) return 'warning';
  if (n === 5) return 'notice';
  if (n >= 7) return 'debug';
  return 'normal';
}

const SYSLOG_FACILITY_NAMES = {
  0: 'kern', 1: 'user', 2: 'mail', 3: 'daemon', 4: 'auth', 5: 'syslog',
  6: 'lpr', 7: 'news', 8: 'uucp', 9: 'cron', 10: 'authpriv', 11: 'ftp',
  16: 'local0', 17: 'local1', 18: 'local2', 19: 'local3',
  20: 'local4', 21: 'local5', 22: 'local6', 23: 'local7',
};

function syslogFacilityName(raw) {
  const n = tryInt(raw);
  if (n === null) return null;
  return SYSLOG_FACILITY_NAMES[n] ?? null;
}

const ERRNO_NAMES = {
  1: 'EPERM', 2: 'ENOENT', 3: 'ESRCH', 4: 'EINTR', 5: 'EIO',
  6: 'ENXIO', 7: 'E2BIG', 8: 'ENOEXEC', 9: 'EBADF', 10: 'ECHILD',
  11: 'EAGAIN', 12: 'ENOMEM', 13: 'EACCES', 14: 'EFAULT', 15: 'ENOTBLK',
  16: 'EBUSY', 17: 'EEXIST', 18: 'EXDEV', 19: 'ENODEV', 20: 'ENOTDIR',
  21: 'EISDIR', 22: 'EINVAL', 23: 'ENFILE', 24: 'EMFILE', 25: 'ENOTTY',
  26: 'ETXTBSY', 27: 'EFBIG', 28: 'ENOSPC', 29: 'ESPIPE', 30: 'EROFS',
  31: 'EMLINK', 32: 'EPIPE', 33: 'EDOM', 34: 'ERANGE', 35: 'EDEADLK',
  36: 'ENAMETOOLONG', 37: 'ENOLCK', 38: 'ENOSYS', 39: 'ENOTEMPTY',
  40: 'ELOOP', 42: 'ENOMSG', 43: 'EIDRM', 44: 'ECHRNG', 45: 'EL2NSYNC',
  46: 'EL3HLT', 47: 'EL3RST', 48: 'ELNRNG', 49: 'EUNATCH', 50: 'ENOCSI',
  51: 'EL2HLT', 52: 'EBADE', 53: 'EBADR', 54: 'EXFULL', 55: 'ENOANO',
  56: 'EBADRQC', 57: 'EBADSLT', 59: 'EBFONT', 60: 'ENOSTR', 61: 'ENODATA',
  62: 'ETIME', 63: 'ENOSR', 64: 'ENONET', 65: 'ENOPKG', 66: 'EREMOTE',
  67: 'ENOLINK', 68: 'EADV', 69: 'ESRMNT', 70: 'ECOMM', 71: 'EPROTO',
  72: 'EMULTIHOP', 73: 'EDOTDOT', 74: 'EBADMSG', 75: 'EOVERFLOW',
  76: 'ENOTUNIQ', 77: 'EBADFD', 78: 'EREMCHG', 79: 'ELIBACC',
  80: 'ELIBBAD', 81: 'ELIBSCN', 82: 'ELIBMAX', 83: 'ELIBEXEC',
  84: 'EILSEQ', 85: 'ERESTART', 86: 'ESTRPIPE', 87: 'EUSERS',
  88: 'ENOTSOCK', 89: 'EDESTADDRREQ', 90: 'EMSGSIZE', 91: 'EPROTOTYPE',
  92: 'ENOPROTOOPT', 93: 'EPROTONOSUPPORT', 94: 'ESOCKTNOSUPPORT',
  95: 'ENOTSUP', 96: 'EPFNOSUPPORT', 97: 'EAFNOSUPPORT',
  98: 'EADDRINUSE', 99: 'EADDRNOTAVAIL', 100: 'ENETDOWN',
  101: 'ENETUNREACH', 102: 'ENETRESET', 103: 'ECONNABORTED',
  104: 'ECONNRESET', 105: 'ENOBUFS', 106: 'EISCONN', 107: 'ENOTCONN',
  108: 'ESHUTDOWN', 109: 'ETOOMANYREFS', 110: 'ETIMEDOUT',
  111: 'ECONNREFUSED', 112: 'EHOSTDOWN', 113: 'EHOSTUNREACH',
  114: 'EALREADY', 115: 'EINPROGRESS', 116: 'ESTALE', 117: 'EUCLEAN',
  118: 'ENOTNAM', 119: 'ENAVAIL', 120: 'EISNAM', 121: 'EREMOTEIO',
  122: 'EDQUOT', 123: 'ENOMEDIUM', 124: 'EMEDIUMTYPE', 125: 'ECANCELED',
  126: 'ENOKEY', 127: 'EKEYEXPIRED', 128: 'EKEYREVOKED',
  129: 'EKEYREJECTED', 130: 'EOWNERDEAD', 131: 'ENOTRECOVERABLE',
  132: 'ERFKILL', 133: 'EHWPOISON',
};

function errnoName(raw) {
  const n = tryInt(raw);
  if (n === null) return null;
  const name = ERRNO_NAMES[n];
  if (name === undefined) return null;
  return `${n} (${name})`;
}

const CAPABILITIES = [
  'CHOWN', 'DAC_OVERRIDE', 'DAC_READ_SEARCH', 'FOWNER', 'FSETID',
  'KILL', 'SETGID', 'SETUID', 'SETPCAP', 'LINUX_IMMUTABLE',
  'NET_BIND_SERVICE', 'NET_BROADCAST', 'NET_ADMIN', 'NET_RAW',
  'IPC_LOCK', 'IPC_OWNER', 'SYS_MODULE', 'SYS_RAWIO', 'SYS_CHROOT',
  'SYS_PTRACE', 'SYS_PACCT', 'SYS_ADMIN', 'SYS_BOOT', 'SYS_NICE',
  'SYS_RESOURCE', 'SYS_TIME', 'SYS_TTY_CONFIG', 'MKNOD', 'LEASE',
  'AUDIT_WRITE', 'AUDIT_CONTROL', 'SETFCAP', 'MAC_OVERRIDE',
  'MAC_ADMIN', 'SYSLOG', 'WAKE_ALARM', 'BLOCK_SUSPEND', 'AUDIT_READ',
  'PERFMON', 'BPF', 'CHECKPOINT_RESTORE',
];

function capEffectiveDisplay(raw) {
  if (!raw) return raw;
  const text = Buffer.isBuffer(raw) ? raw.toString('latin1') : String(raw);
  if (!text || !/^[0-9a-fA-F]+$/.test(text)) return text;
  let value;
  try {
    value = BigInt(`0x${text}`);
  } catch {
    return text;
  }
  if (value === 0n) return text;
  const names = [];
  for (let i = 0; i < CAPABILITIES.length; i++) {
    if ((value >> BigInt(i)) & 1n) names.push(CAPABILITIES[i]);
  }
  if (names.length === 0) return text;
  return `${text} (${names.join(' | ')})`;
}

const MESSAGE_ID_NAMES = {
  f77379a8490b408bbe5f6940505a777b: 'Journal started',
  d93fb3c9c24d451a97cea615ce59c00b: 'Journal stopped',
  a596d6fe7bfa4994828e72309e95d61e: 'Journal messages suppressed',
  e9bf28e6e834481bb6f48f548ad13606: 'Journal messages missed',
  ec387f577b844b8fa948f33cad9a75e6: 'Journal disk space usage',
  fc2e22bc6ee647b6b90729ab34a250b1: 'Coredump',
  '5aadd8e954dc4b1a8c954d63fd9e1137': 'Coredump truncated',
  '1f4e0a44a88649939aaea34fc6da8c95': 'Backtrace',
  '8d45620c1a4348dbb17410da57c60c66': 'User Session created',
  '3354939424b4456d9802ca8333ed424a': 'User Session terminated',
  fcbefc5da23d428093f97c82a9290f7b: 'Seat started',
  e7852bfe46784ed0accde04bc864c2d5: 'Seat removed',
  '24d8d4452573402496068381a6312df2': 'VM or container started',
  '58432bd3bace477cb514b56381b8a758': 'VM or container stopped',
  c7a787079b354eaaa9e77b371893cd27: 'Time change',
  '45f82f4aef7a4bbf942ce861d1f20990': 'Timezone change',
  '50876a9db00f4c40bde1a2ad381c3a1b': 'System configuration issues',
  b07a249cd024414a82dd00cd181378ff: 'System start-up completed',
  eed00a68ffd84e31882105fd973abdd1: 'User start-up completed',
  '6bbd95ee977941e497c48be27c254128': 'Sleep start',
  '8811e6df2a8e40f58a94cea26f8ebf14': 'Sleep stop',
  '98268866d1d54a499c4e98921d93bc40': 'System shutdown initiated',
  c14aaf76ec284a5fa1f105f88dfb061c: 'System factory reset initiated',
  d9ec5e95e4b646aaaea2fd05214edbda: 'Container init crashed',
  '3ed0163e868a4417ab8b9e210407a96c': 'System reboot failed after crash',
  '645c735537634ae0a32b15a7c6cba7d4': 'Init execution froze',
  '5addb3a06a734d3396b794bf98fb2d01': 'Init crashed no coredump',
  '5c9e98de4ab94c6a9d04d0ad793bd903': 'Init crashed no fork',
  '5e6f1f5e4db64a0eaee3368249d20b94': 'Init crashed unknown signal',
  '83f84b35ee264f74a3896a9717af34cb': 'Init crashed systemd signal',
  '3a73a98baf5b4b199929e3226c0be783': 'Init crashed process signal',
  '2ed18d4f78ca47f0a9bc25271c26adb4': 'Init crashed waitpid failed',
  '56b1cd96f24246c5b607666fda952356': 'Init crashed coredump failed',
  '4ac7566d4d7548f4981f629a28f0f829': 'Init crashed coredump',
  '38e8b1e039ad469291b18b44c553a5b7': 'Crash shell failed to fork',
  '872729b47dbe473eb768ccecd477beda': 'Crash shell failed to execute',
  '658a67adc1c940b3b3316e7e8628834a': 'Selinux failed',
  e6f456bd92004d9580160b2207555186: 'Battery low warning',
  '267437d33fdd41099ad76221cc24a335': 'Battery low powering off',
  '79e05b67bc4545d1922fe47107ee60c5': 'Manager mainloop failed',
  dbb136b10ef4457ba47a795d62f108c9: 'Manager no xdgdir path',
  ed158c2df8884fa584eead2d902c1032: 'Init failed to drop capability bounding set of usermode',
  '42695b500df048298bee37159caa9f2e': 'Init failed to drop capability bounding set',
  bfc2430724ab44499735b4f94cca9295: "User manager can't disable new privileges",
  '59288af523be43a28d494e41e26e4510': 'Manager failed to start default target',
  '689b4fcc97b4486ea5da92db69c9e314': 'Manager failed to isolate default target',
  '5ed836f1766f4a8a9fc5da45aae23b29': 'Manager failed to collect passed file descriptors',
  '6a40fbfbd2ba4b8db02fb40c9cd090d7': 'Init failed to fix up environment variables',
  '0e54470984ac419689743d957a119e2e': 'Manager failed to allocate',
  d67fa9f847aa4b048a2ae33535331adb: 'Manager failed to write Smack',
  af55a6f75b544431b72649f36ff6d62c: 'System shutdown critical error',
  d18e0339efb24a068d9c1060221048c2: 'Init failed to fork off valgrind',
  '7d4958e842da4a758f6c1cdc7b36dcc5': 'Unit starting',
  '39f53479d3a045ac8e11786248231fbf': 'Unit started',
  be02cf6855d2428ba40df7e9d022f03d: 'Unit failed',
  de5b426a63be47a7b6ac3eaac82e2f6f: 'Unit stopping',
  '9d1aaa27d60140bd96365438aad20286': 'Unit stopped',
  d34d037fff1847e6ae669a370e694725: 'Unit reloading',
  '7b05ebc668384222baa8881179cfda54': 'Unit reloaded',
  '5eb03494b6584870a536b337290809b3': 'Unit restart scheduled',
  ae8f7b866b0347b9af31fe1c80b127c0: 'Unit resources',
  '7ad2d189f7e94e70a38c781354912448': 'Unit success',
  '0e4284a0caca4bfc81c0bb6786972673': 'Unit skipped',
  d9b373ed55a64feb8242e02dbe79a49c: 'Unit failure result',
  '641257651c1b4ec9a8624d7a40a9e1e7': 'Process execution failed',
  '98e322203f7a4ed290d09fe03c09fe15': 'Unit process exited',
  '0027229ca0644181a76c4e92458afa2e': 'Syslog forward missed',
  '1dee0369c7fc4736b7099b38ecb46ee7': 'Mount point is not empty',
  d989611b15e44c9dbf31e3c81256e4ed: 'Unit oomd kill',
  fe6faa94e7774663a0da52717891d8ef: 'Unit out of memory',
  b72ea4a2881545a0b50e200e55b9b06f: 'Lid opened',
  b72ea4a2881545a0b50e200e55b9b070: 'Lid closed',
  f5f416b862074b28927a48c3ba7d51ff: 'System docked',
  '51e171bd585248568110144c517cca53': 'System undocked',
  b72ea4a2881545a0b50e200e55b9b071: 'Power key',
  '3e0117101eb243c1b9a50db3494ab10b': 'Power key long press',
  '9fa9d2c012134ec385451ffe316f97d0': 'Reboot key',
  f1c59a58c9d943668965c337caec5975: 'Reboot key long press',
  b72ea4a2881545a0b50e200e55b9b072: 'Suspend key',
  bfdaf6d312ab4007bc1fe40a15df78e8: 'Suspend key long press',
  b72ea4a2881545a0b50e200e55b9b073: 'Hibernate key',
  '167836df6f7f428e98147227b2dc8945': 'Hibernate key long press',
  c772d24e9a884cbeb9ea12625c306c01: 'Invalid configuration',
  '1675d7f172174098b1108bf8c7dc8f5d': 'DNSSEC validation failed',
  '4d4408cfd0d144859184d1e65d7c8a65': 'DNSSEC trust anchor revoked',
  '36db2dfa5a9045e1bd4af5f93e1cf057': 'DNSSEC turned off',
  b61fdac612e94b9182285b998843061f: 'Username unsafe',
  '1b3bb94037f04bbf81028e135a12d293': 'Mount point path not suitable',
  '010190138f494e29a0ef6669749531aa': 'Device path not suitable',
  b480325f9c394a7b802c231e51a2752c: 'Nobody user unsuitable',
  '1c0454c1bd2241e0ac6fefb4bc631433': 'Systemd udev settle deprecated',
  '7c8a41f37b764941a0e1780b1be2f037': 'Time initial sync',
  '7db73c8af0d94eeb822ae04323fe6ab6': 'Time initial bump',
  '9e7066279dc8403da79ce4b1a69064b2': 'Shutdown scheduled',
  '249f6fb9e6e2428c96f3f0875681ffa3': 'Shutdown canceled',
  '3f7d5ef3e54f4302b4f0b143bb270cab': 'TPM PCR Extended',
  f9b0be465ad540d0850ad32172d57c21: 'Memory Trimmed',
  a8fa8dacdb1d443e9503b8be367a6adb: 'SysV Service Found',
  '187c62eb1e7f463bb530394f52cb090f': 'Portable Service attached',
  '76c5c754d628490d8ecba4c9d042112b': 'Portable Service detached',
  '9cf56b8baf9546cf9478783a8de42113': 'systemd-networkd sysctl changed by foreign process',
  ad7089f928ac4f7ea00c07457d47ba8a: 'SRK into TPM authorization failure',
  b2bcbaf5edf948e093ce50bbea0e81ec: 'Secure Attention Key (SAK) was pressed',
  '7fc63312330b479bb32e598d47cef1a8': 'dbus activate no unit',
  ee9799dab1e24d81b7bee7759a543e1b: 'dbus activate masked unit',
  a0fa58cafd6f4f0c8d003d16ccf9e797: 'dbus broker exited',
  c8c6cde1c488439aba371a664353d9d8: 'dbus dirwatch',
  '8af3357071af4153af414daae07d38e7': 'dbus dispatch stats',
  '199d4300277f495f84ba4028c984214c': 'dbus no sopeergroup',
  b209c0d9d1764ab38d13b8e00d1784d6: 'dbus protocol violation',
  '6fa70fa776044fa28be7a21daf42a108': 'dbus receive failed',
  '0ce0fa61d1a9433dabd67417f6b8e535': 'dbus service failed open',
  '24dc708d9e6a4226a3efe2033bb744de': 'dbus service invalid',
  f15d2347662d483ea9bcd8aa1a691d28: 'dbus sighup',
  '0ce153587afa4095832d233c17a88001': 'Gnome SM startup succeeded',
  '10dd2dc188b54a5e98970f56499d1f73': 'Gnome SM unrecoverable failure',
  f3ea493c22934e26811cd62abe8e203a: 'Gnome shell started',
  c7b39b1e006b464599465e105b361485: 'Flatpak cache',
  '75ba3deb0af041a9a46272ff85d9e73e': 'Flathub pulls',
  f02bce89a54e4efab3a94a797d26204a: 'Flathub pull errors',
  dd11929c788e48bdbb6276fb5f26b08a: 'Boltd starting',
  '1e6061a9fbd44501b3ccc368119f2b69': 'Netdata startup',
  ed4cdb8f1beb4ad3b57cb3cae2d162fa: 'Netdata connection from child',
  '6e2e3839067648968b646045dbf28d66': 'Netdata connection to parent',
  '9ce0cb58ab8b44df82c4bf1ad9ee22de': 'Netdata alert transition',
  '6db0018e83e34320ae2a659d78019fb7': 'Netdata alert notification',
  '23e93dfccbf64e11aac858b9410d8a82': 'Netdata fatal message',
  '8ddaf5ba33a74078b609250db1e951f3': 'Sensor state transition',
  ec87a56120d5431bace51e2fb8bba243: 'Netdata log flood protection',
  acb33cb95778476baac702eb7e4e151d: 'Netdata Cloud connection',
  d1f59606dd4d41e3b217a0cfcae8e632: 'Netdata extreme cardinality',
  '02f47d350af5449197bf7a95b605a468': 'Netdata exit reason',
  '4fdf40816c124623a032b7fe73beacb8': 'Netdata dynamic configuration',
};

function messageIdName(raw) {
  return MESSAGE_ID_NAMES[raw] ?? null;
}

function formatRealtimeUsec(timestamp, micros) {
  if (typeof timestamp !== 'number' || timestamp < 0 || !Number.isFinite(timestamp)) {
    return String(timestamp);
  }
  const seconds = Math.floor(timestamp / 1_000_000);
  const remMicros = timestamp % 1_000_000;
  try {
    const date = new Date(seconds * 1000);
    if (isNaN(date.getTime())) return String(timestamp);
    const iso = date.toISOString();
    if (micros) {
      const base = iso.slice(0, 19);
      return `${base}.${String(remMicros).padStart(6, '0')}Z`;
    }
    return iso;
  } catch {
    return String(timestamp);
  }
}

function cachedUidDisplay(context, raw) {
  const cached = context._uidCache.get(raw);
  if (cached !== undefined) return cached;
  const display = raw;
  context._uidCache.set(raw, display);
  return display;
}

function cachedGidDisplay(context, raw) {
  const cached = context._gidCache.get(raw);
  if (cached !== undefined) return cached;
  const display = raw;
  context._gidCache.set(raw, display);
  return display;
}

// ---------------------------------------------------------------------------
// Field-by-field display rule (mirror Rust L4329-4389)
// ---------------------------------------------------------------------------

function systemdFieldDisplayValue(context, scope, field, value, resolveUserGroupNames) {
  const raw = bytesToText(value);
  const valueKey = typeof value === 'string' ? value : bytesToText(value);

  if (field === 'PRIORITY') {
    return priorityName(raw) ?? raw;
  }

  if (field === 'SYSLOG_FACILITY') {
    return syslogFacilityName(raw) ?? raw;
  }

  if (field === 'ERRNO') {
    return errnoName(raw) ?? raw;
  }

  if (field === 'MESSAGE_ID') {
    const name = messageIdName(raw);
    if (name !== null) {
      if (scope === DisplayScope.Data) return `${raw} (${name})`;
      return name;
    }
    return raw;
  }

  if (field === '_BOOT_ID') {
    const ts = context._bootFirstRealtime.get(valueKey);
    if (ts !== undefined) {
      const formatted = formatRealtimeUsec(ts, false);
      if (scope === DisplayScope.Data) return `${raw} (${formatted})  `;
      return formatted;
    }
    return raw;
  }

  if (UID_FIELDS.has(field)) {
    if (resolveUserGroupNames) {
      return cachedUidDisplay(context, raw);
    }
    return raw;
  }

  if (GID_FIELDS.has(field)) {
    if (resolveUserGroupNames) {
      return cachedGidDisplay(context, raw);
    }
    return raw;
  }

  if (field === '_CAP_EFFECTIVE') {
    return capEffectiveDisplay(raw);
  }

  if (field === '_SOURCE_REALTIME_TIMESTAMP') {
    const parsed = tryInt(raw);
    if (parsed !== null && parsed !== 0) {
      return `${raw} (${formatRealtimeUsec(parsed, true)})`;
    }
    return raw;
  }

  return raw;
}

// ---------------------------------------------------------------------------
// chunk 2b: request handling, source discovery, merge, envelope attach
// ---------------------------------------------------------------------------

import { Buffer } from 'node:buffer';
import { readdirSync, statSync, realpathSync } from 'node:fs';
import { join, basename, dirname } from 'node:path';
import {
  Direction as ExplorerDirection,
  ExplorerAnchor as ExplorerAnchorClass,
  ExplorerAnchorKind,
  ExplorerControl,
  ExplorerFieldMode,
  ExplorerFilter,
  ExplorerQuery,
  ExplorerStats,
  ExplorerStrategy,
  ExplorerStopReason,
  exploreWithStrategyAndControl,
  _newHistogram,
} from './explorer.js';
import { isJournalFileName } from './compress.js';
import { FileReader, readFileHeader } from './reader.js';

const NETDATA_REMOTE_PATH_FRAGMENT = '/remote/';
const API_RELATIVE_TIME_MAX_SECONDS = 3 * 365 * 86_400;
const NETDATA_MISSING_AFTER_RELATIVE_SECONDS = 600;

function _unixNowSeconds(injectableNow) {
  if (injectableNow != null) return Math.floor(injectableNow / 1000);
  return Math.floor(Date.now() / 1000);
}

function _normalizeTimestampToUsec(value) {
  if (value < 0) value = 0;
  if (value >= 1_000_000_000_000) return Math.floor(value);
  return value * 1_000_000;
}

function _normalizeTimestampToUsecWithRounding(value, endOfSecond) {
  if (value < 0) value = 0;
  if (value >= 1_000_000_000_000) return Math.floor(value);
  if (endOfSecond) return Math.floor(value) * 1_000_000 + 999_999;
  return Math.floor(value) * 1_000_000;
}

function _anchorOutsideWindow(anchor, afterUsec, beforeUsec) {
  if (anchor.kind !== ExplorerAnchorKind.Realtime) return false;
  const anchorUsec = Number(anchor.realtimeUsec);
  if (afterUsec != null && anchorUsec < afterUsec) return true;
  if (beforeUsec != null && anchorUsec > beforeUsec) return true;
  return false;
}

function _relativeWindowToAbsolute(nowSeconds, after, before) {
  if (Math.abs(before) <= API_RELATIVE_TIME_MAX_SECONDS) {
    if (before > 0) before = -before;
    before = nowSeconds + before;
  }
  if (Math.abs(after) <= API_RELATIVE_TIME_MAX_SECONDS) {
    if (after > 0) after = -after;
    if (after === 0) after = -NETDATA_MISSING_AFTER_RELATIVE_SECONDS;
    after = before + after + 1;
  }
  if (after > before) { const t = after; after = before; before = t; }
  if (before > nowSeconds) {
    const delta = before - nowSeconds;
    before -= delta;
    after -= delta;
  }
  return [after, before];
}

export function normalizeTimeWindow(nowSeconds, after, before, injectableNow) {
  const ns = injectableNow != null ? Math.floor(injectableNow / 1000) : nowSeconds != null ? nowSeconds : Math.floor(Date.now() / 1000);
  let a = after != null ? after : 0;
  let b = before != null ? before : 0;
  if (a === 0 && b === 0) {
    b = ns;
    a = b - DEFAULT_TIME_WINDOW_SECONDS;
  } else {
    [a, b] = _relativeWindowToAbsolute(ns, a, b);
  }
  if (a > b) { const t = a; a = b; b = t; }
  if (a === b) a = b - DEFAULT_TIME_WINDOW_SECONDS;
  return [
    _normalizeTimestampToUsecWithRounding(Math.max(a, 0), false),
    _normalizeTimestampToUsecWithRounding(Math.max(b, 0), true),
  ];
}

// ---------------------------------------------------------------------------
// Source classification (mirror Rust L3422-3465)
// ---------------------------------------------------------------------------

function _localNamespaceSourceName(pathStr) {
  const parent = dirname(pathStr);
  const parentName = basename(parent);
  const dotIdx = parentName.lastIndexOf('.');
  if (dotIdx < 0) return null;
  const namespace = parentName.slice(dotIdx + 1);
  if (!namespace) return null;
  return `namespace-${namespace}`;
}

export function journalFileSourceType(pathStr) {
  if (pathStr.includes(NETDATA_REMOTE_PATH_FRAGMENT)) {
    return NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_REMOTE_ALL;
  }
  const namespace = _localNamespaceSourceName(pathStr);
  if (namespace !== null) {
    return NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE;
  }
  const name = basename(pathStr);
  if (name.startsWith('system')) {
    return NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_SYSTEM;
  }
  if (name.startsWith('user')) {
    return NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_USER;
  }
  return NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_OTHER;
}

function _journalFileExactSourceName(pathStr) {
  if (pathStr.includes(NETDATA_REMOTE_PATH_FRAGMENT)) {
    const name = basename(pathStr);
    if (name.includes('@')) return name.split('@')[0];
    for (const suffix of ['.journal~.zst', '.journal.zst', '.journal~', '.journal']) {
      if (name.endsWith(suffix)) {
        const stripped = name.slice(0, -suffix.length);
        if (stripped.startsWith('remote-')) return stripped;
        return null;
      }
    }
    return null;
  }
  return _localNamespaceSourceName(pathStr);
}

// ---------------------------------------------------------------------------
// BFS journal file discovery (mirror Rust L3848-3895)
// ---------------------------------------------------------------------------

export class JournalFileCollection {
  constructor() {
    this.files = [];
    this.skipped = 0;
    this.errors = [];
  }
}

function _canonical(pathStr) {
  try { return realpathSync(pathStr); } catch { return pathStr; }
}

export function collectJournalFiles(directory) {
  const collection = new JournalFileCollection();
  const pending = [{ dir: directory, depth: 0 }];
  const visited = new Set();
  while (pending.length > 0) {
    const { dir, depth } = pending.shift();
    const key = _canonical(dir);
    if (visited.has(key)) continue;
    if (visited.size >= NETDATA_MAX_DIRECTORY_SCAN_COUNT) {
      collection.skipped += 1;
      collection.errors.push(`${dir}: directory scan limit reached`);
      continue;
    }
    visited.add(key);
    let entries;
    try { entries = readdirSync(dir, { withFileTypes: true }); }
    catch (err) {
      if (dir === directory) throw err;
      collection.skipped += 1;
      collection.errors.push(`${dir}: ${err.message}`);
      continue;
    }
    for (const entry of entries) {
      try {
        if (entry.isFile()) {
          if (isJournalFileName(entry.name)) {
            collection.files.push(join(dir, entry.name));
          }
        } else if (entry.isDirectory()) {
          if (depth < NETDATA_MAX_DIRECTORY_SCAN_DEPTH) {
            pending.push({ dir: join(dir, entry.name), depth: depth + 1 });
          }
        }
      } catch { continue; }
    }
  }
  collection.files.sort();
  const seen = new Set();
  const deduped = [];
  for (const p of collection.files) {
    const k = _canonical(p);
    if (seen.has(k)) continue;
    seen.add(k);
    deduped.push(p);
  }
  collection.files = deduped;
  return collection;
}

// ---------------------------------------------------------------------------
// Netdata request decoding (all 16 params)
// ---------------------------------------------------------------------------

function _getBool(req, key, def = false) {
  const v = req[key];
  if (typeof v === 'boolean') return v;
  return def;
}

function _getI64(req, key) {
  const v = req[key];
  if (v == null) return null;
  if (typeof v === 'boolean') return null;
  const n = Number(v);
  if (!Number.isFinite(n)) return null;
  return n;
}

function _getU64(req, key) {
  const v = req[key];
  if (v == null) return null;
  if (typeof v === 'boolean') return null;
  const n = Number(v);
  if (!Number.isFinite(n) || n < 0) return null;
  return n;
}

function _getStr(req, key) {
  const v = req[key];
  return typeof v === 'string' ? v : null;
}

function _parseStringArray(value) {
  if (!Array.isArray(value)) return null;
  return value.filter(item => typeof item === 'string');
}

function _requestDirection(req) {
  const v = _getStr(req, 'direction') || 'backward';
  if (v === 'forward' || v === 'forwards' || v === 'next') return ExplorerDirection.Forward;
  return ExplorerDirection.Backward;
}

function _requestLimit(req) {
  const v = _getU64(req, 'last');
  if (v == null || v === 0) return DEFAULT_ITEMS_TO_RETURN;
  return v;
}

function _requestFacets(req, config) {
  const parsed = _parseStringArray(req.facets);
  if (parsed == null) return config.defaultFacets.map(f => Buffer.from(f, 'utf8'));
  return parsed.map(f => Buffer.from(f, 'utf8'));
}

function _requestHistogram(req) {
  const v = _getStr(req, 'histogram');
  if (v == null || v === '') return null;
  return v;
}

function _requestHistogramOrDefault(requested, config) {
  return requested != null ? requested : config.defaultHistogram;
}

function _requestQuery(req) {
  const v = _getStr(req, 'query');
  return (v == null || v === '') ? null : v;
}

const PRIORITY_NAME_TO_NUMBER = {
  panic: 0, alert: 1, critical: 2, error: 3,
  warning: 4, notice: 5, info: 6, debug: 7,
};

function _normalizeFilterValue(field, value) {
  if (field === 'PRIORITY') {
    const n = PRIORITY_NAME_TO_NUMBER[value];
    if (n != null) return Buffer.from(String(n), 'ascii');
  }
  return Buffer.from(value, 'utf8');
}

function _parseFilters(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return [];
  const out = [];
  for (const [field, rawValues] of Object.entries(value)) {
    if (field === 'query' || field === 'source' || field === '__logs_sources') continue;
    const values = _parseStringArray(rawValues);
    if (!values || !field) continue;
    const normalized = values.map(v => _normalizeFilterValue(field, v));
    out.push(new ExplorerFilter(Buffer.from(field, 'utf8'), normalized));
  }
  return out;
}

const SOURCE_TYPE_FOR_NAME = {
  'all': NETDATA_SOURCE_TYPE_ALL,
  'all-local-logs': NETDATA_SOURCE_TYPE_LOCAL_ALL,
  'all-remote-systems': NETDATA_SOURCE_TYPE_REMOTE_ALL,
  'all-local-system-logs': NETDATA_SOURCE_TYPE_LOCAL_SYSTEM,
  'all-local-user-logs': NETDATA_SOURCE_TYPE_LOCAL_USER,
  'all-local-namespaces': NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE,
  'all-uncategorized': NETDATA_SOURCE_TYPE_LOCAL_OTHER,
};

function _parseSourceSelection(value) {
  let sourceType = NETDATA_SOURCE_TYPE_ALL;
  const exact = [];
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return [sourceType, exact];
  const raw = value.__logs_sources;
  const values = _parseStringArray(raw);
  if (!values) return [sourceType, exact];
  sourceType = 0;
  for (const v of values) {
    const mapped = SOURCE_TYPE_FOR_NAME[v];
    if (mapped == null) exact.push(v);
    else sourceType |= mapped;
  }
  return [sourceType, exact];
}

export class NetdataRequest {
  constructor() {
    this.info = false;
    this.echo = {};
    this.afterRealtimeUsec = null;
    this.beforeRealtimeUsec = null;
    this.ifModifiedSinceUsec = 0;
    this.anchor = ExplorerAnchorClass.auto();
    this.direction = ExplorerDirection.Backward;
    this.limit = DEFAULT_ITEMS_TO_RETURN;
    this.dataOnly = false;
    this.delta = false;
    this.tail = false;
    this.sampling = DEFAULT_ITEMS_SAMPLING;
    this.sourceType = NETDATA_SOURCE_TYPE_ALL;
    this.exactSources = [];
    this.filters = [];
    this.facets = [];
    this.histogram = null;
    this.query = null;
    this._explicitKeys = new Set();
  }

  static parse(value, config, injectableNow) {
    const req = new NetdataRequest();
    const info = _getBool(value, 'info', false);
    const after = _getI64(value, 'after');
    const before = _getI64(value, 'before');
    const [afterUsec, beforeUsec] = normalizeTimeWindow(null, after, before, injectableNow);
    const direction = _requestDirection(value);
    const ifModified = _getU64(value, 'if_modified_since') || 0;
    const dataOnly = _getBool(value, 'data_only', false);
    const delta = dataOnly && _getBool(value, 'delta', false);
    const tail = dataOnly && ifModified !== 0 && _getBool(value, 'tail', false);
    let sampling = _getU64(value, 'sampling');
    if (sampling == null) sampling = DEFAULT_ITEMS_SAMPLING;
    const anchorValue = _getU64(value, 'anchor');
    let anchor = (anchorValue != null && anchorValue !== 0)
      ? ExplorerAnchorClass.realtime(_normalizeTimestampToUsec(anchorValue))
      : ExplorerAnchorClass.auto();
    let dir = direction;
    if (tail && anchor.kind === ExplorerAnchorKind.Realtime) {
      dir = ExplorerDirection.Backward;
    } else if (_anchorOutsideWindow(anchor, afterUsec, beforeUsec)) {
      anchor = ExplorerAnchorClass.auto();
      dir = ExplorerDirection.Backward;
    }
    const requestedLimit = _requestLimit(value);
    const limit = Math.max(2, requestedLimit);
    const requestedFacets = _parseStringArray(value.facets);
    const facets = _requestFacets(value, config);
    const requestedHistogram = _requestHistogram(value);
    const histogram = _requestHistogramOrDefault(requestedHistogram, config);
    const requestedQuery = _requestQuery(value);
    const [sourceType, exactSources] = _parseSourceSelection(value.selections);
    const filters = _parseFilters(value.selections);
    const echo = _buildEcho({
      info, afterUsec, beforeUsec, ifModified, anchor, direction: dir,
      limit: requestedLimit, dataOnly, delta, tail, sampling, sourceType,
      requestedFacets, selections: value.selections, histogram: requestedHistogram, query: requestedQuery,
    });
    req.info = info;
    req.echo = echo;
    req.afterRealtimeUsec = afterUsec;
    req.beforeRealtimeUsec = beforeUsec;
    req.ifModifiedSinceUsec = ifModified;
    req.anchor = anchor;
    req.direction = dir;
    req.limit = limit;
    req.dataOnly = dataOnly;
    req.delta = delta;
    req.tail = tail;
    req.sampling = sampling;
    req.sourceType = sourceType;
    req.exactSources = exactSources;
    req.filters = filters;
    req.facets = facets;
    req.histogram = histogram;
    req.query = requestedQuery;
    req._explicitKeys = typeof value === 'object' && value !== null ? new Set(Object.keys(value)) : new Set();
    return req;
  }
}

function _buildEcho(input) {
  const anchorUsec = input.anchor.kind === ExplorerAnchorKind.Realtime ? Number(input.anchor.realtimeUsec) : 0;
  const dirName = input.direction === ExplorerDirection.Forward ? 'forward' : 'backward';
  const afterSeconds = input.afterUsec != null ? Math.floor(input.afterUsec / 1_000_000) : 0;
  const beforeSeconds = input.beforeUsec != null ? Math.floor(input.beforeUsec / 1_000_000) : 0;
  const out = {
    info: input.info, slice: true, data_only: input.dataOnly, delta: input.delta, tail: input.tail,
    sampling: input.sampling, source_type: input.sourceType,
    after: afterSeconds, before: beforeSeconds, if_modified_since: input.ifModified,
    anchor: anchorUsec, direction: dirName, last: input.limit,
    query: input.query, histogram: input.histogram,
  };
  if (input.requestedFacets != null) out.facets = [...input.requestedFacets];
  if (typeof input.selections === 'object' && input.selections !== null && !Array.isArray(input.selections)) {
    const copy = { ...input.selections };
    const sources = copy.__logs_sources;
    if (Array.isArray(sources)) copy.__logs_sources = sources.map(() => null);
    out.selections = copy;
  }
  return out;
}

// ---------------------------------------------------------------------------
// LocatedRow + CombinedResult
// ---------------------------------------------------------------------------

class LocatedRow {
  constructor(filePath, realtimeUsec, cursor, payloads) {
    this.filePath = filePath;
    this.realtimeUsec = realtimeUsec;
    this.cursor = cursor;
    this.payloads = payloads;
  }
}

export class CombinedResult {
  constructor() {
    this.rows = [];
    this.facets = new Map();
    this.histogram = null;
    this.columnFields = new Set();
    this.stats = new ExplorerStats();
    this.matchedFiles = 0;
    this.matchedPaths = [];
    this.skippedFiles = 0;
    this.fileErrors = [];
    this.partial = false;
    this.timedOut = false;
    this.cancelled = false;
    this.samplingEnabled = false;
  }

  merge(path, result, direction, limit) {
    if (result.histogram != null) this._mergeHistogram(result.histogram);
    this._mergeStats(result.stats);
    for (const row of result.rows) {
      this.rows.push(new LocatedRow(path, Number(row.realtimeUsec), row.cursor, row.payloads));
    }
    for (const f of result.columnFields) {
      this.columnFields.add(typeof f === 'string' ? f : Buffer.isBuffer(f) ? f.toString('utf8') : String(f));
    }
    for (const [field, values] of result.facets) {
      const fieldKey = Buffer.isBuffer(field) ? field.toString('hex') : String(field);
      let dest = this.facets.get(fieldKey);
      if (dest == null) { dest = new Map(); this.facets.set(fieldKey, dest); }
      for (const [value, count] of values) {
        const valueKey = Buffer.isBuffer(value) ? value.toString('hex') : String(value);
        const existing = dest.get(valueKey);
        dest.set(valueKey, (existing ?? 0n) + (typeof count === 'bigint' ? count : BigInt(count)));
      }
    }
    this._sortAndLimit(direction, limit);
  }

  _mergeHistogram(source) {
    if (this.histogram == null) {
      this.histogram = {
        field: Buffer.isBuffer(source.field) ? Buffer.from(source.field) : source.field,
        buckets: source.buckets.map(b => ({
          startRealtimeUsec: b.startRealtimeUsec,
          endRealtimeUsec: b.endRealtimeUsec,
          values: new Map([...b.values].map(([k, v]) => [k, v])),
        })),
      };
      return;
    }
    if (this.histogram.buckets.length !== source.buckets.length) return;
    for (let i = 0; i < source.buckets.length; i++) {
      const dst = this.histogram.buckets[i];
      const src = source.buckets[i];
      if (dst.startRealtimeUsec !== src.startRealtimeUsec || dst.endRealtimeUsec !== src.endRealtimeUsec) return;
      for (const [value, count] of src.values) {
        const existing = dst.values.get(value);
        dst.values.set(value, (existing ?? 0n) + (typeof count === 'bigint' ? count : BigInt(count)));
      }
    }
  }

  _mergeStats(stats) {
    const s = this.stats;
    s.rowsExamined += stats.rowsExamined;
    s.rowsMatched += stats.rowsMatched;
    s.facetRowsMatched += stats.facetRowsMatched;
    s.rowsReturned += stats.rowsReturned;
    s.rowsUnsampled += stats.rowsUnsampled;
    s.rowsEstimated += stats.rowsEstimated;
    s.samplingSampled += stats.samplingSampled;
    s.samplingUnsampled += stats.samplingUnsampled;
    s.samplingEstimated += stats.samplingEstimated;
    if (stats.lastRealtimeUsec > s.lastRealtimeUsec) s.lastRealtimeUsec = stats.lastRealtimeUsec;
    if (stats.maxSourceRealtimeDeltaUsec > s.maxSourceRealtimeDeltaUsec) s.maxSourceRealtimeDeltaUsec = stats.maxSourceRealtimeDeltaUsec;
    s.dataRefsSeen += stats.dataRefsSeen;
    s.dataRefsSkipped += stats.dataRefsSkipped;
    s.dataPayloadsLoaded += stats.dataPayloadsLoaded;
    s.dataObjectsClassified += stats.dataObjectsClassified;
    s.dataCacheHits += stats.dataCacheHits;
    s.dataCacheMisses += stats.dataCacheMisses;
    s.payloadsDecompressed += stats.payloadsDecompressed;
    s.ftsScans += stats.ftsScans;
    s.facetUpdates += stats.facetUpdates;
    s.histogramUpdates += stats.histogramUpdates;
    s.returnedRowExpansions += stats.returnedRowExpansions;
    s.earlyStopOpportunities += stats.earlyStopOpportunities;
    s.earlyStops += stats.earlyStops;
  }

  add_zero_count_facet_values_from_files(fields) {
    if (!fields || fields.length === 0 || this.matchedPaths.length === 0) return;
    for (const pathStr of this.matchedPaths) {
      let reader;
      try { reader = FileReader.open(pathStr); }
      catch { continue; }
      try {
        for (const field of fields) {
          if (!field) continue;
          const fieldName = Buffer.isBuffer(field) ? field.toString('utf8') : field;
          if (!fieldName) continue;
          let values;
          try { values = reader.queryUnique(fieldName); }
          catch { continue; }
          if (!values || values.length === 0) continue;
          const fieldHex = Buffer.isBuffer(field) ? field.toString('hex') : Buffer.from(field).toString('hex');
          let target = this.facets.get(fieldHex);
          if (!target) { target = new Map(); this.facets.set(fieldHex, target); }
          for (const value of values) {
            if (!value || value.length === 0) continue;
            const valueStr = value.toString('utf8');
            if (valueStr === '-' || valueStr === '') continue;
            const valueHex = value.toString('hex');
            if (!target.has(valueHex)) target.set(valueHex, 0n);
          }
        }
      } finally {
        try { reader.close(); } catch {}
      }
    }
  }

  add_zero_count_selected_filter_values(request) {
    if (!request || !request.filters || request.filters.length === 0) return;
    const reportFields = new Set();
    for (const f of request.facets) {
      const fieldBytes = Buffer.isBuffer(f) ? f : Buffer.from(f);
      reportFields.add(fieldBytes.toString('hex'));
    }
    if (request.histogram != null) {
      reportFields.add(Buffer.from(request.histogram, 'utf8').toString('hex'));
    }
    for (const f of request.filters) {
      if (!f || !f.field || !f.values || f.values.length === 0) continue;
      const fieldHex = f.field.toString('hex');
      if (!reportFields.has(fieldHex)) continue;
      let target = this.facets.get(fieldHex);
      if (!target) { target = new Map(); this.facets.set(fieldHex, target); }
      for (const value of f.values) {
        if (!value || value.length === 0) continue;
        const valueStr = value.toString('utf8');
        if (valueStr === '-' || valueStr === '') continue;
        const valueHex = value.toString('hex');
        if (!target.has(valueHex)) target.set(valueHex, 0n);
      }
    }
  }

  _sortAndLimit(direction, limit) {
    if (direction === ExplorerDirection.Forward) {
      this.rows.sort((a, b) => a.realtimeUsec - b.realtimeUsec);
    } else {
      this.rows.sort((a, b) => b.realtimeUsec - a.realtimeUsec);
    }
    let lastFrom = 0, lastTo = 0, initialized = false;
    for (const located of this.rows) {
      const ts = located.realtimeUsec;
      if (initialized && ts >= lastFrom && ts <= lastTo) {
        if (direction === ExplorerDirection.Backward) {
          lastFrom = Math.max(0, lastFrom - 1);
          located.realtimeUsec = lastFrom;
        } else {
          lastTo += 1;
          located.realtimeUsec = lastTo;
        }
      } else {
        lastFrom = ts;
        lastTo = ts;
        initialized = true;
      }
    }
    if (limit > 0 && this.rows.length > limit) this.rows.length = limit;
    this.stats.rowsReturned = BigInt(this.rows.length);
  }
}

// ---------------------------------------------------------------------------
// Source summary + envelope builders
// ---------------------------------------------------------------------------

function _humanBinarySize(numBytes) {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = numBytes, unit = 0;
  while (value >= 1024 && unit + 1 < units.length) { value /= 1024; unit += 1; }
  if (unit === 0) return `${numBytes}${units[unit]}`;
  if (value === Math.floor(value)) return `${Math.floor(value)}${units[unit]}`;
  let text = value.toFixed(2);
  while (text.includes('.') && text.endsWith('0')) text = text.slice(0, -1);
  if (text.endsWith('.')) text = text.slice(0, -1);
  return `${text}${units[unit]}`;
}

function _humanDurationSeconds(seconds) {
  let r = Math.floor(seconds);
  const years = Math.floor(r / (365 * 86400)); r %= (365 * 86400);
  const months = Math.floor(r / (30 * 86400)); r %= (30 * 86400);
  const days = Math.floor(r / 86400); r %= 86400;
  const hours = Math.floor(r / 3600); r %= 3600;
  const minutes = Math.floor(r / 60); const secs = r % 60;
  const parts = [];
  if (years) parts.push(`${years}y`);
  if (months) parts.push(`${months}mo`);
  if (days) parts.push(`${days}d`);
  if (hours) parts.push(`${hours}h`);
  if (minutes) parts.push(`${minutes}m`);
  if (secs || !parts.length) parts.push(`${secs}s`);
  return parts.join(' ');
}

function _formatLastEntryRfc3339Usec(lastRealtimeUsec) {
  if (lastRealtimeUsec == null) return 'unknown';
  const seconds = Math.floor(lastRealtimeUsec / 1_000_000);
  try {
    const d = new Date(seconds * 1000);
    if (isNaN(d.getTime())) return 'unknown';
    return d.toISOString().replace(/\.\d+Z$/, 'Z');
  } catch { return 'unknown'; }
}

function _summaryToSourceOption(name, summary) {
  if (summary.files === 0) return null;
  const first = summary.firstRealtimeUsec;
  const last = summary.lastRealtimeUsec;
  let coverage = 'off';
  if (first != null && last != null && last > first && (last - first) >= 1_000_000) {
    coverage = _humanDurationSeconds(Math.floor((last - first) / 1_000_000));
  }
  return {
    id: name, name,
    info: `${summary.files} files, total size ${_humanBinarySize(summary.totalSize)}, covering ${coverage}, last entry at ${_formatLastEntryRfc3339Usec(last)}`,
    pill: _humanBinarySize(summary.totalSize),
  };
}

function _buildSourceSummary(paths, state) {
  const all = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null };
  const local = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null };
  const ns = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null };
  const sys = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null };
  const user = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null };
  const remote = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null };
  const other = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null };
  const exact = new Map();
  for (const p of paths) {
    let stat;
    try { stat = statSync(p); } catch { continue; }
    const sz = stat.size;
    const st = journalFileSourceType(p);
    const metadata = _stateFileMetadata(state, p);
    _addSummaryPath(all, p, sz, metadata);
    if (st & NETDATA_SOURCE_TYPE_LOCAL_ALL) _addSummaryPath(local, p, sz, metadata);
    if (st & NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE) _addSummaryPath(ns, p, sz, metadata);
    if (st & NETDATA_SOURCE_TYPE_LOCAL_SYSTEM) _addSummaryPath(sys, p, sz, metadata);
    if (st & NETDATA_SOURCE_TYPE_LOCAL_USER) _addSummaryPath(user, p, sz, metadata);
    if (st & NETDATA_SOURCE_TYPE_REMOTE_ALL) _addSummaryPath(remote, p, sz, metadata);
    if (st & NETDATA_SOURCE_TYPE_LOCAL_OTHER) _addSummaryPath(other, p, sz, metadata);
    const exactName = _journalFileExactSourceName(p);
    if (exactName != null) {
      let bucket = exact.get(exactName);
      if (!bucket) { bucket = { files: 0, totalSize: 0, firstRealtimeUsec: null, lastRealtimeUsec: null }; exact.set(exactName, bucket); }
      _addSummaryPath(bucket, p, sz, metadata);
    }
  }
  const options = [];
  for (const [label, summary] of [['all', all], ['all-local-logs', local], ['all-local-namespaces', ns], ['all-local-system-logs', sys], ['all-local-user-logs', user], ['all-remote-systems', remote], ['all-uncategorized', other]]) {
    const opt = _summaryToSourceOption(label, summary);
    if (opt) options.push(opt);
  }
  for (const [name, summary] of exact) {
    const opt = _summaryToSourceOption(name, summary);
    if (opt) options.push(opt);
  }
  return { id: '__logs_sources', options };
}

function _minRealtime(current, candidate) {
  if (current == null) return candidate;
  return current < candidate ? current : candidate;
}

function _maxRealtime(current, candidate) {
  if (current == null) return candidate;
  return current > candidate ? current : candidate;
}

function _addSummaryPath(summary, path, size, metadata) {
  summary.files += 1;
  summary.totalSize += size;
  if (metadata != null) {
    const metadataFirst = metadata.msgFirstRealtimeUsec;
    const metadataLast = metadata.msgLastRealtimeUsec;
    if (metadataFirst != null) {
      summary.firstRealtimeUsec = _minRealtime(summary.firstRealtimeUsec, metadataFirst);
    }
    if (metadataLast != null) {
      summary.lastRealtimeUsec = _maxRealtime(summary.lastRealtimeUsec, metadataLast);
    }
    if (metadataFirst != null && metadataLast != null) return;
  }
  try {
    const header = readFileHeader(path);
    if (header.head_entry_realtime != null) {
      const head = Number(header.head_entry_realtime);
      if (head !== 0) summary.firstRealtimeUsec = _minRealtime(summary.firstRealtimeUsec, head);
    }
    if (header.tail_entry_realtime != null) {
      const tail = Number(header.tail_entry_realtime);
      if (tail !== 0) summary.lastRealtimeUsec = _maxRealtime(summary.lastRealtimeUsec, tail);
    }
  } catch {
    // File header unreadable; bounds stay unchanged, file still contributes files+totalSize.
  }
}

function _buildInfoResponse(echo, paths, config, state) {
  const sourceSummary = _buildSourceSummary(paths, state);
  return {
    _request: { ...echo },
    versions: { netdata_function_api: 1, sdk: '0.1.0' },
    v: 3,
    accepted_params: [...NETDATA_ACCEPTED_PARAMS],
    required_params: [{
      id: '__logs_sources',
      name: config.sourceSelectorName,
      help: config.sourceSelectorHelp,
      type: 'multiselect',
      options: sourceSummary.options,
    }],
    show_ids: true,
    has_history: true,
    pagination: { enabled: true, key: 'anchor', column: 'timestamp', units: 'timestamp_usec' },
    status: 200,
    type: 'table',
    help: 'Netdata-compatible journal log function backed by the systemd journal SDK',
  };
}

function _buildLogsSourcesResponse(echo, paths, config, state) {
  return {
    _request: { ...echo },
    status: 200,
    type: 'multiselect',
    id: '__logs_sources',
    name: config.sourceSelectorName,
    help: config.sourceSelectorHelp,
    options: _buildSourceSummary(paths, state).options,
  };
}

function _buildColumnOrder(request, config, combined) {
  const order = ['timestamp', 'rowOptions'];
  for (const key of config.defaultViewKeys) { if (!order.includes(key)) order.push(key); }
  for (const field of request.facets) {
    const name = Buffer.isBuffer(field) ? field.toString('utf8') : String(field);
    if (!order.includes(name)) order.push(name);
  }
  if (request.histogram != null && !order.includes(request.histogram)) order.push(request.histogram);
  const sortedFields = [...combined.columnFields].sort();
  for (const f of sortedFields) { if (!order.includes(f)) order.push(f); }
  return order;
}

function _buildColumnsMetadata(order) {
  const out = {};
  for (let idx = 0; idx < order.length; idx++) {
    const key = order[idx];
    let visible = false, filter = 'none', fullWidth = false, columnType = 'string', visualization = 'value';
    let transform = 'none', defaultValue = '-';
    if (key === 'timestamp') {
      visible = true; filter = 'range'; columnType = 'timestamp'; transform = 'datetime_usec'; defaultValue = null;
    } else if (key === 'rowOptions') {
      columnType = 'none'; visualization = 'rowOptions'; defaultValue = null;
    } else if (key === '_HOSTNAME') {
      visible = true; filter = 'facet';
    } else if (key === 'ND_JOURNAL_PROCESS' || key === 'MESSAGE') {
      visible = true;
      if (key === 'MESSAGE') fullWidth = true;
    } else if (key === 'ND_JOURNAL_FILE' || key === '_SOURCE_REALTIME_TIMESTAMP') {
      // hidden by default
    } else {
      const isFacet = key === 'MESSAGE_ID' || (!key.includes('MESSAGE') && !key.includes('TIMESTAMP') && !key.startsWith('__'));
      if (isFacet) filter = 'facet';
    }
    const meta = {
      index: idx, unique_key: key === 'timestamp', name: key === 'timestamp' ? 'Timestamp' : key,
      visible, type: columnType, visualization,
      value_options: { transform, decimal_points: 0, default_value: defaultValue },
      sort: 'ascending', sortable: false, sticky: false, summary: 'count', filter, full_width: fullWidth,
      wrap: key !== 'rowOptions',
      default_expanded_filter: key === 'PRIORITY' || key === 'SYSLOG_FACILITY' || key === 'MESSAGE_ID',
    };
    if (key === 'rowOptions') meta.dummy = true;
    out[key] = meta;
  }
  return out;
}

function _dynamicProcessName(fields) {
  let base = '';
  for (const key of ['CONTAINER_NAME', 'SYSLOG_IDENTIFIER', '_COMM']) {
    const vals = fields[key];
    if (vals && vals.length > 0) { base = bytesToText(vals[0]); break; }
  }
  if (!base) return '-';
  const pidVals = fields._PID;
  if (pidVals && pidVals.length > 0 && pidVals[0].length > 0) return `${base}[${bytesToText(pidVals[0])}]`;
  if (pidVals != null) return base;
  return `${base}[-]`;
}

function _rowFieldsMap(located) {
  const fields = {};
  for (const payload of located.payloads) {
    const buf = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
    const eq = buf.indexOf(0x3d);
    if (eq < 0) continue;
    const field = buf.toString('utf8', 0, eq);
    const value = buf.subarray(eq + 1);
    if (!fields[field]) fields[field] = [];
    fields[field].push(value);
  }
  fields.ND_JOURNAL_FILE = [Buffer.from(located.filePath, 'utf8')];
  if (!fields.ND_JOURNAL_PROCESS) {
    const process = _dynamicProcessName(fields);
    if (process) fields.ND_JOURNAL_PROCESS = [Buffer.from(process, 'utf8')];
  }
  return fields;
}

function _buildDataRow(located, columnOrder, _direction, _config, profile, context) {
  const fields = _rowFieldsMap(located);
  const row = [];
  for (const column of columnOrder) {
    if (column === 'timestamp') {
      row.push(located.realtimeUsec);
    } else if (column === 'rowOptions') {
      const strFields = {};
      for (const [k, v] of Object.entries(fields)) strFields[k] = v;
      row.push(profile.rowOptions(strFields));
    } else {
      const values = fields[column];
      if (!values || values.length === 0) { row.push(null); continue; }
      const value = values[0];
      let rendered;
      try { rendered = profile.fieldDisplayValue(context, DisplayScope.Data, column, value); }
      catch { rendered = bytesToText(value); }
      row.push(rendered);
    }
  }
  return row;
}

// Mirror Rust `sort_facet_options` (netdata.rs:3258). PRIORITY sorts by the
// numeric severity ascending (non-numeric ids first, matching Option ordering);
// every other field sorts by count descending, then id ascending by code unit.
function _parsePriorityForSort(id) {
  if (!/^\d+$/.test(id)) return null;
  const value = Number(id);
  return Number.isInteger(value) && value >= 0 && value <= 255 ? value : null;
}

function _sortFacetOptions(field, options) {
  options.sort((a, b) => {
    if (field === 'PRIORITY') {
      const pa = _parsePriorityForSort(a.id);
      const pb = _parsePriorityForSort(b.id);
      if (pa === null && pb === null) return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
      if (pa === null) return -1;
      if (pb === null) return 1;
      return pa - pb;
    }
    if (b.count !== a.count) return b.count - a.count;
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
}

function _buildFacetsPayload(request, _config, combined, profile) {
  const context = new DisplayContext();
  const out = [];
  for (let orderIndex = 0; orderIndex < request.facets.length; orderIndex++) {
    const fieldBuf = request.facets[orderIndex];
    const fieldHex = Buffer.isBuffer(fieldBuf) ? fieldBuf.toString('hex') : String(fieldBuf);
    const fieldName = Buffer.isBuffer(fieldBuf) ? fieldBuf.toString('utf8') : String(fieldBuf);
    const values = combined.facets.get(fieldHex) || new Map();
    const options = [];
    for (const [valueHex, count] of values) {
      const valueBytes = Buffer.from(valueHex, 'hex');
      const text = valueBytes.toString('utf8');
      if (!text || text === '-') continue;
      const name = profile.facetOptionName(context, fieldName, valueBytes);
      options.push({ id: text, name, count: Number(count) });
    }
    _sortFacetOptions(fieldName, options);
    for (let idx = 0; idx < options.length; idx++) options[idx].order = idx + 1;
    out.push({ id: fieldName, name: fieldName, order: orderIndex + 1, options });
  }
  return out;
}

function _histogramUpdateEverySeconds(histogram) {
  if (!histogram || !histogram.buckets || histogram.buckets.length === 0) return 1;
  const first = histogram.buckets[0];
  const width = first.endRealtimeUsec - first.startRealtimeUsec;
  if (width <= 0n) return 1;
  return Math.max(1, Number(width / 1_000_000n));
}

function _histogramAfterSeconds(histogram) {
  if (!histogram || !histogram.buckets || histogram.buckets.length === 0) return 0;
  return Number(histogram.buckets[0].startRealtimeUsec / 1_000_000n);
}

function _histogramBeforeSeconds(histogram) {
  if (!histogram || !histogram.buckets || histogram.buckets.length === 0) return 0;
  return Number(histogram.buckets[histogram.buckets.length - 1].endRealtimeUsec / 1_000_000n);
}

function _histogramDimensionStats(histogram, actualDimensions, dimensionHex) {
  if (!actualDimensions.has(dimensionHex)) return [0, 0, 0, false];
  let dMin = 0, dMax = 0, dSum = 0;
  for (let i = 0; i < histogram.buckets.length; i++) {
    const count = Number(histogram.buckets[i].values.get(dimensionHex) || 0n);
    if (i === 0 || count < dMin) dMin = count;
    if (count > dMax) dMax = count;
    dSum += count;
  }
  return [dMin, dMax, dSum, true];
}

function _emptyHistogramChartEnvelope(field) {
  const sts = { min: [], max: [], avg: [], arp: [], con: [] };
  return {
    id: field, name: field,
    chart: {
      summary: { nodes: [], contexts: [], instances: [], dimensions: [], labels: [], alerts: [] },
      totals: { nodes: { sl: 1, qr: 1 } },
      result: { labels: ['time'], point: { value: 0, arp: 1, pa: 2 }, data: [] },
      db: {
        tiers: 1, update_every: 1, units: 'events',
        dimensions: { ids: [], names: [], units: [], sts },
        per_tier: [{ tier: 0, queries: 1, points: 0, update_every: 1 }],
      },
      view: {
        title: `Events Distribution by ${field}`, update_every: 1, after: 0, before: 0,
        units: 'events', chart_type: 'stackedBar',
        dimensions: { grouped_by: ['dimension'], ids: [], names: [], colors: [], units: [], sts },
        min: 0, max: 0,
      },
      agents: [{ mg: 'default', nm: 'facets.histogram', now: Math.floor(Date.now() / 1000), ai: 0 }],
    },
  };
}

function _histogramChartMetadata(field, histogram, dimensionIds, profile) {
  const context = new DisplayContext();
  const actualDimensions = new Set();
  for (const bucket of histogram.buckets) {
    for (const valueKey of bucket.values.keys()) {
      actualDimensions.add(valueKey);
    }
  }
  const ids = [];
  const names = [];
  const colors = new Array(dimensionIds.length).fill(null);
  const minValues = [];
  const maxValues = [];
  const avgValues = [];
  const arpValues = [];
  const conValues = [];
  let points = 0;
  let overallMin = 0;
  let overallMax = 0;
  for (const dimHex of dimensionIds) {
    const dimBuf = Buffer.from(dimHex, 'hex');
    const idStr = dimBuf.toString('utf-8');
    const display = profile.fieldDisplayValue(context, DisplayScope.Histogram, field, dimBuf);
    const nameStr = (typeof display === 'string') ? display : idStr;
    const [dMin, dMax, dSum, actual] = _histogramDimensionStats(histogram, actualDimensions, dimHex);
    const dAvg = actual && histogram.buckets.length > 0 ? dSum / histogram.buckets.length : 0.0;
    if (actual) {
      if (points === 0 || dMin < overallMin) overallMin = dMin;
      if (dMax > overallMax) overallMax = dMax;
      points += histogram.buckets.length;
    }
    let total = 0n;
    for (let i = 0; i < histogram.buckets.length; i++) {
      total += (histogram.buckets[i].values.get(dimHex) || 0n);
    }
    const contribution = total > 0n ? (dSum * 100.0 / Number(total)) : 0.0;
    ids.push(idStr);
    names.push(nameStr);
    minValues.push(dMin);
    maxValues.push(dMax);
    avgValues.push(dAvg);
    arpValues.push(0);
    conValues.push(contribution);
  }
  const totalPoints = points;
  const summaryStats = {
    min: overallMin, max: overallMax,
    avg: totalPoints > 0 ? overallMin / totalPoints : 0.0,
    con: 100.0,
  };
  const totals = { nodes: { sl: 1, qr: 1 } };
  if (dimensionIds.length > 0) {
    totals.contexts = { sl: 1, qr: 1 };
    totals.instances = { sl: 1, qr: 1 };
    totals.dimensions = { sl: dimensionIds.length, qr: dimensionIds.length };
  }
  const ds = dimensionIds.length > 0 ? { sl: dimensionIds.length, qr: dimensionIds.length } : {};
  const is = dimensionIds.length > 0 ? { sl: 1, qr: 1 } : {};
  const summary = {
    nodes: [{ mg: 'default', nm: 'facets.histogram', ni: 0, st: { ai: 0, code: 200, msg: '' }, ds, is, sts: totalPoints > 0 ? summaryStats : {} }],
    contexts: [{ id: 'facets.histogram', ds, is, sts: totalPoints > 0 ? summaryStats : {} }],
    instances: [{ id: 'facets.histogram', ni: 0, ds, sts: totalPoints > 0 ? summaryStats : {} }],
    dimensions: ids.map((idStr, idx) => ({
      id: idStr, nm: names[idx],
      ds: { sl: actualDimensions.has(dimensionIds[idx]) ? 1 : 0, qr: actualDimensions.has(dimensionIds[idx]) ? 1 : 0 },
      sts: { min: minValues[idx], max: maxValues[idx], avg: avgValues[idx], con: conValues[idx] },
      pri: idx,
    })),
    labels: [],
    alerts: [],
  };
  const stats = { min: minValues, max: maxValues, avg: avgValues, arp: arpValues, con: conValues };
  return {
    ids_decoded: ids, names, colors, units: ['events'], stats, summary, totals,
    points: totalPoints, min: overallMin, max: overallMax, actual_dimensions: actualDimensions,
  };
}

function _buildHistogramPayload(field, histogram, combined, _request, profile) {
  if (!histogram || !histogram.buckets || histogram.buckets.length === 0) {
    return _emptyHistogramChartEnvelope(field);
  }
  const dimensionIdsSet = [];
  const seen = new Set();
  for (const bucket of histogram.buckets) {
    for (const valueKey of bucket.values.keys()) {
      if (seen.has(valueKey)) continue;
      seen.add(valueKey);
      dimensionIdsSet.push(valueKey);
    }
  }
  const fieldHex = Buffer.from(field, 'utf8').toString('hex');
  const knownValues = combined.facets.get(fieldHex);
  if (knownValues) {
    for (const valueKey of knownValues.keys()) {
      const decoded = Buffer.from(valueKey, 'hex').toString('utf8');
      if (!decoded || decoded === '-') continue;
      if (seen.has(valueKey)) continue;
      seen.add(valueKey);
      dimensionIdsSet.push(valueKey);
    }
  }
  const metadata = _histogramChartMetadata(field, histogram, dimensionIdsSet, profile);
  const data = [];
  for (const bucket of histogram.buckets) {
    const point = [Number(bucket.startRealtimeUsec / 1000n)];
    for (const dimHex of dimensionIdsSet) {
      const count = bucket.values.get(dimHex);
      if (count != null && count !== 0n) {
        point.push([Number(count), 0, 0]);
      } else if (metadata.actual_dimensions.has(dimHex)) {
        point.push([0, 0, 0]);
      } else {
        point.push([null, 0, 0]);
      }
    }
    data.push(point);
  }
  const updateEvery = _histogramUpdateEverySeconds(histogram);
  return {
    id: field, name: field,
    chart: {
      summary: metadata.summary, totals: metadata.totals,
      result: {
        labels: ['time', ...metadata.names],
        point: { value: 0, arp: 1, pa: 2 },
        data,
      },
      db: {
        tiers: 1, update_every: updateEvery, units: 'events',
        dimensions: {
          ids: metadata.ids_decoded, names: metadata.names,
          units: metadata.units, sts: metadata.stats,
        },
        per_tier: [{ tier: 0, queries: 1, points: metadata.points, update_every: updateEvery }],
      },
      view: {
        title: `Events Distribution by ${field}`, update_every: updateEvery,
        after: _histogramAfterSeconds(histogram), before: _histogramBeforeSeconds(histogram),
        units: 'events', chart_type: 'stackedBar',
        dimensions: {
          grouped_by: ['dimension'], ids: metadata.ids_decoded, names: metadata.names,
          colors: metadata.colors, units: metadata.units, sts: metadata.stats,
        },
        min: metadata.min, max: metadata.max,
      },
      agents: [{
        mg: 'default', nm: 'facets.histogram',
        now: Math.floor(Date.now() / 1000), ai: 0,
      }],
    },
  };
}

function _buildItemsPayload(request, combined, returned) {
  const unsampled = Number(combined.stats.rowsUnsampled);
  const estimated = Number(combined.stats.rowsEstimated);
  const evaluated = Number(combined.stats.rowsExamined) + unsampled + estimated;
  const matched = Number(combined.stats.rowsMatched) + unsampled + estimated;
  const rowsMatchedN = Number(combined.stats.rowsMatched);
  let rawAfter = rowsMatchedN > returned ? rowsMatchedN - returned : 0;
  const tailAnchor = request.tail && request.delta &&
    request.anchor.kind === ExplorerAnchorKind.Realtime;
  if (tailAnchor) rawAfter += 1;
  return {
    evaluated, matched, unsampled, estimated, returned,
    max_to_return: request.limit, before: 0, after: rawAfter,
  };
}

function _buildMessagePayload(combined) {
  if (!combined.timedOut && combined.stats.rowsUnsampled === 0n && combined.stats.rowsEstimated === 0n) return 'OK';
  const total = Math.max(1, Number(combined.stats.rowsExamined) + Number(combined.stats.rowsUnsampled) + Number(combined.stats.rowsEstimated));
  const realPct = Number(combined.stats.rowsExamined) * 100.0 / total;
  const titleParts = [], descParts = [];
  let status = 'notice';
  if (combined.timedOut) { titleParts.push('Query timed-out, incomplete data. '); descParts.push('QUERY TIMEOUT: The query timed out and may not include all the data of the selected window. '); status = 'warning'; }
  if (combined.stats.rowsUnsampled !== 0n || combined.stats.rowsEstimated !== 0n) {
    titleParts.push(`${realPct.toFixed(2)}% real data`);
    descParts.push(`ACTUAL DATA: The filters counters reflect ${realPct.toFixed(2)}% of the data. `);
  }
  return { title: titleParts.join(''), status, description: descParts.join('') };
}

function _netdataFunctionError(code, msg) {
  return { status: code, errorMessage: msg };
}


// ---------------------------------------------------------------------------
// State hook classes (mirror Rust L274-300)
// ---------------------------------------------------------------------------

export class NetdataJournalFileMetadata {
  constructor({
    sourceType = null,
    sourceName = null,
    fileLastModifiedUsec = null,
    msgFirstRealtimeUsec = null,
    msgLastRealtimeUsec = null,
    journalVsRealtimeDeltaUsec = null,
  } = {}) {
    this.sourceType = sourceType;
    this.sourceName = sourceName;
    this.fileLastModifiedUsec = fileLastModifiedUsec;
    this.msgFirstRealtimeUsec = msgFirstRealtimeUsec;
    this.msgLastRealtimeUsec = msgLastRealtimeUsec;
    this.journalVsRealtimeDeltaUsec = journalVsRealtimeDeltaUsec;
  }
}

export class NetdataFunctionState {
  fileMetadata(_path) { return null; }
  updateFileJournalVsRealtimeDeltaUsec(_path, _deltaUsec) {}
}

export class NetdataFunctionProgress {
  constructor({ currentFile, totalFiles, matchedFiles, skippedFiles, stats, elapsed }) {
    this.currentFile = currentFile;
    this.totalFiles = totalFiles;
    this.matchedFiles = matchedFiles;
    this.skippedFiles = skippedFiles;
    this.stats = stats;
    this.elapsed = elapsed;
  }
}

export class NetdataFunctionRunOptions {
  constructor({
    timeout = null,
    progressCallback = null,
    cancellationCallback = null,
    state = null,
    progressInterval = 0.25,
  } = {}) {
    this.timeout = timeout;
    this.progressCallback = progressCallback;
    this.cancellationCallback = cancellationCallback;
    this.state = state;
    this.progressInterval = progressInterval;
  }

  static fromTimeoutSeconds(seconds) {
    if (seconds === 0) return new NetdataFunctionRunOptions({ timeout: null });
    return new NetdataFunctionRunOptions({ timeout: seconds });
  }
}

function _buildQueryResponse(request, config, combined, _paths, profile) {
  const columnsOrder = _buildColumnOrder(request, config, combined);
  const columnsMeta = _buildColumnsMetadata(columnsOrder);
  const context = new DisplayContext();
  const dataRows = [];
  let rowsIter = [...combined.rows];
  if (request.direction === ExplorerDirection.Forward) rowsIter = rowsIter.reverse();
  for (const located of rowsIter) {
    dataRows.push(_buildDataRow(located, columnsOrder, request.direction, config, profile, context));
  }
  let histogramField = request.histogram;
  if (request.dataOnly && !request._explicitKeys.has('histogram')) histogramField = null;
  const accepted = [...NETDATA_ACCEPTED_PARAMS];
  const seenParams = new Set(accepted);
  for (const fieldBuf of request.facets) {
    const name = Buffer.isBuffer(fieldBuf) ? fieldBuf.toString('utf8') : String(fieldBuf);
    if (!seenParams.has(name)) { seenParams.add(name); accepted.push(name); }
  }
  const body = {
    _request: { ...request.echo },
    versions: { netdata_function_api: 1, sdk: '0.1.0' },
    _journal_files: { matched: combined.matchedFiles, skipped: combined.skippedFiles, errors: [...combined.fileErrors] },
    status: 200, partial: combined.partial, type: 'table', show_ids: true, has_history: true,
    pagination: { enabled: true, key: 'anchor', column: 'timestamp', units: 'timestamp_usec' },
    columns: columnsMeta, data: dataRows,
    _stats: { sdk_explorer: combined.stats.toJson() },
    expires: request.dataOnly ? _unixNowSeconds() + 3600 : 0,
  };
  if (!request.dataOnly) {
    body.message = _buildMessagePayload(combined);
    body.update_every = 1;
    body.help = null;
    body.accepted_params = accepted;
    body.default_sort_column = 'timestamp';
    body.default_charts = [];
    body.available_histograms = _buildAvailableHistograms(request, combined);
  } else if (histogramField != null) {
    body.available_histograms = _buildAvailableHistograms(request, combined);
  }
  if (!request.dataOnly || request.delta) {
    body.facets = _buildFacetsPayload(request, config, combined, profile);
    if (histogramField != null) {
      let histogram = combined.histogram;
      if (histogram == null) {
        const emptyQuery = _requestToExplorerQuery(request, combined.matchedFiles, null);
        emptyQuery.histogram = Buffer.from(histogramField, 'utf8');
        histogram = _newHistogram(Buffer.from(histogramField, 'utf8'), emptyQuery);
      }
      body.histogram = _buildHistogramPayload(histogramField, histogram, combined, request, profile);
    } else {
      body.histogram = null;
    }
    body.items = _buildItemsPayload(request, combined, combined.rows.length);
  }
  if (!request.dataOnly || request.tail) {
    body.last_modified = Number(combined.stats.lastRealtimeUsec);
  }
  if (combined.samplingEnabled) {
    body._sampling = {
      enabled: true,
      sampled: Number(combined.stats.samplingSampled),
      unsampled: Number(combined.stats.samplingUnsampled),
      estimated: Number(combined.stats.samplingEstimated),
    };
  }
  if (request.dataOnly && request.delta) {
    if ('facets' in body) { body.facets_delta = body.facets; delete body.facets; }
    if ('histogram' in body) { body.histogram_delta = body.histogram; delete body.histogram; }
    if ('items' in body) { body.items_delta = body.items; delete body.items; }
  }
  return body;
}

// ---------------------------------------------------------------------------
// Available histograms builder (mirror Rust L1225-1251)
// ---------------------------------------------------------------------------

function _netdataReorderKey(value) {
  const trimmed = value.replace(/^[!-/:-@[-`{-~]+/, '');
  return trimmed.toLowerCase();
}

function _buildAvailableHistograms(request, _combined) {
  const seenFields = new Set();
  const fields = [];
  for (const field of request.facets) {
    const name = Buffer.isBuffer(field) ? field.toString('utf8') : String(field);
    if (!seenFields.has(name)) { seenFields.add(name); fields.push(name); }
  }
  if (request.dataOnly && request.histogram != null) {
    if (!seenFields.has(request.histogram)) {
      seenFields.add(request.histogram);
      fields.push(request.histogram);
    }
  }
  const sortable = fields.map(name => ({ key: _netdataReorderKey(name), name }));
  sortable.sort((a, b) => a.key < b.key ? -1 : a.key > b.key ? 1 : 0);
  const orderByField = new Map();
  for (let i = 0; i < sortable.length; i++) orderByField.set(sortable[i].name, i + 1);
  const out = [];
  for (const name of fields) {
    out.push({ id: name, name, order: orderByField.get(name) || 0 });
  }
  return out;
}

// ---------------------------------------------------------------------------
// Sampling budget (mirror Rust L1536-1590)
// ---------------------------------------------------------------------------

function _applySamplingBudget(combined, budget) {
  if (budget <= 0) return;
  const s = combined.stats;
  const rows = Number(s.rowsMatched);
  if (rows <= 0) {
    s.samplingSampled = 0n; s.samplingUnsampled = 0n; s.samplingEstimated = 0n;
    return;
  }
  if (rows <= budget) {
    s.samplingSampled = BigInt(rows); s.samplingUnsampled = 0n; s.samplingEstimated = 0n;
    return;
  }
  s.samplingSampled = BigInt(budget);
  const remaining = rows - budget;
  const limit = Math.max(1, Number(s.rowsReturned) || 0);
  const unsampled = Math.min(remaining, limit);
  const estimated = remaining - unsampled;
  s.samplingUnsampled = BigInt(unsampled);
  s.samplingEstimated = BigInt(estimated);
  s.rowsUnsampled += BigInt(unsampled);
  s.rowsEstimated += BigInt(estimated);
}

// ---------------------------------------------------------------------------
// State hook helpers
// ---------------------------------------------------------------------------

function _stateFileMetadata(state, path) {
  if (state == null) return null;
  try { return state.fileMetadata(path); } catch { return null; }
}

function _normalizeJournalVsRealtimeDeltaUsec(deltaUsec) {
  if (deltaUsec <= 0) return NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC;
  if (deltaUsec < NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC) return NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC;
  if (deltaUsec > NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC) return NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC;
  return deltaUsec;
}

function _updateLearnedRealtimeDelta(state, path, orderDeltaUsec, stats) {
  if (state == null) return;
  const learned = Number(stats.maxSourceRealtimeDeltaUsec || 0n);
  if (learned === 0) return;
  if (learned <= orderDeltaUsec) return;
  const normalized = _normalizeJournalVsRealtimeDeltaUsec(learned);
  if (normalized <= orderDeltaUsec) return;
  try { state.updateFileJournalVsRealtimeDeltaUsec(path, normalized); } catch {}
}

// ---------------------------------------------------------------------------
// 304 pre-scan short-circuit (mirror Rust L2677-2689 + L2938-2967)
// ---------------------------------------------------------------------------

function _notModifiedBeforeScanResponse(request, paths, state) {
  if (request.ifModifiedSinceUsec === 0) return null;
  for (const pathStr of paths) {
    if (!_pathMatchesRequest(pathStr, request, state)) continue;
    const metadata = _stateFileMetadata(state, pathStr);
    let first, last;
    if (metadata != null && metadata.msgFirstRealtimeUsec != null && metadata.msgLastRealtimeUsec != null) {
      first = metadata.msgFirstRealtimeUsec;
      last = metadata.msgLastRealtimeUsec;
    } else {
      try {
        const header = readFileHeader(pathStr);
        first = metadata?.msgFirstRealtimeUsec ?? Number(header.head_entry_realtime || 0);
        const tail = Number(header.tail_entry_realtime || 0);
        if (metadata?.msgLastRealtimeUsec != null) {
          last = metadata.msgLastRealtimeUsec;
        } else if (metadata?.fileLastModifiedUsec != null) {
          last = metadata.fileLastModifiedUsec;
        } else if (tail === 0) {
          try { const st = statSync(pathStr); last = Math.floor(st.mtimeMs * 1000); } catch { last = 0; }
        } else {
          last = tail;
        }
      } catch { first = 0; last = 0; }
    }
    if (first === 0 && last === 0) continue;
    if (!_journalFileOrderMayOverlapRequest(first, last, request.afterRealtimeUsec, request.beforeRealtimeUsec)) continue;
    if (last > request.ifModifiedSinceUsec) return null;
  }
  return _netdataFunctionError(304, 'No new data since the previous call.');
}

// ---------------------------------------------------------------------------
// Deadline + cancellation helpers
// ---------------------------------------------------------------------------

function _computeDeadline(options) {
  if (options?.timeout == null) return null;
  return Date.now() + options.timeout * 1000;
}

function _shouldStopBeforeFile(combined, deadline, options) {
  if (options?.cancellationCallback) {
    try {
      if (options.cancellationCallback()) { combined.partial = true; combined.cancelled = true; return true; }
    } catch {}
  }
  if (deadline != null && Date.now() >= deadline) { combined.partial = true; combined.timedOut = true; return true; }
  return false;
}

function _emitProgressForCombined(options, combined, currentFile, totalFiles, deadline) {
  if (!options?.progressCallback) return;
  const elapsed = deadline != null && options.timeout != null
    ? Math.max(0, (Date.now() - (deadline - options.timeout * 1000)) / 1000)
    : 0;
  const progress = new NetdataFunctionProgress({
    currentFile, totalFiles,
    matchedFiles: combined.matchedFiles,
    skippedFiles: combined.skippedFiles,
    stats: { ...combined.stats },
    elapsed,
  });
  try { options.progressCallback(progress); } catch {}
}

// ---------------------------------------------------------------------------
// File time-window pre-filter (mirror Rust L2997-3026, Python L3959-3983)
// ---------------------------------------------------------------------------

function _journalFileOrderMayOverlapRequest(firstUsec, lastUsec, afterUsec, beforeUsec) {
  if (lastUsec === 0 || lastUsec == null) return true;
  const first = (firstUsec ?? 0) - NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC;
  const last = lastUsec + NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC;
  if (afterUsec != null && last < afterUsec) return false;
  if (beforeUsec != null && first > beforeUsec) return false;
  return true;
}

function _fileOverlapsRequestWindow(pathStr, metadata, afterUsec, beforeUsec) {
  if (metadata != null && metadata.msgLastRealtimeUsec != null && metadata.msgLastRealtimeUsec !== 0) {
    return _journalFileOrderMayOverlapRequest(
      metadata.msgFirstRealtimeUsec, metadata.msgLastRealtimeUsec, afterUsec, beforeUsec);
  }
  let h;
  try { h = readFileHeader(pathStr); } catch (_) { return true; }
  try {
    const last = Number(h.tail_entry_realtime ?? 0);
    if (last === 0) return true;
    const first = Number(h.head_entry_realtime ?? 0);
    return _journalFileOrderMayOverlapRequest(first, last, afterUsec, beforeUsec);
  } catch (_) {
    return true;
  }
}

// ---------------------------------------------------------------------------
// NetdataJournalFunction class
// ---------------------------------------------------------------------------

export class NetdataJournalFunction {
  constructor(config, profile) {
    if (!config.sourceSelectorName) config.sourceSelectorName = DEFAULT_SOURCE_SELECTOR_NAME;
    if (!config.sourceSelectorHelp) config.sourceSelectorHelp = DEFAULT_SOURCE_SELECTOR_HELP;
    this._config = config;
    this._profile = profile;
  }

  static systemdJournal() {
    return new NetdataJournalFunction(NetdataFunctionConfig.systemdJournal(), new SystemdJournalProfile());
  }

  static systemdJournalPluginCompatible() {
    return new NetdataJournalFunction(NetdataFunctionConfig.systemdJournal(), new SystemdJournalPluginProfile());
  }

  static new(config, profile) {
    return new NetdataJournalFunction(config, profile);
  }

  runDirectoryRequestJson(directory, request) {
    return this.runDirectoryRequestJsonWithOptions(directory, request, new NetdataFunctionRunOptions());
  }

  runDirectoryRequestJsonWithOptions(directory, request, options) {
    const injectableNow = options?._injectableNow ?? null;
    const parsed = NetdataRequest.parse(request || {}, this._config, injectableNow);
    const collection = collectJournalFiles(directory);
    const paths = collection.files;
    if (parsed.info) return _buildInfoResponse(parsed.echo, paths, this._config, options?.state);
    if (request && request.__logs_sources) return _buildLogsSourcesResponse(parsed.echo, paths, this._config, options?.state);
    const notModified = _notModifiedBeforeScanResponse(parsed, paths, options?.state);
    if (notModified != null) return notModified;
    const combined = this._exploreFiles(paths, parsed, options, collection.skipped, collection.errors);
    combined.skippedFiles += collection.skipped;
    combined.fileErrors.push(...collection.errors);
    const body = _buildQueryResponse(parsed, this._config, combined, paths, this._profile);
    if (combined.cancelled) return _netdataFunctionError(499, 'Request cancelled.');
    return body;
  }

  runDirectoryRequestBytes(directory, request) {
    return this.runDirectoryRequestBytesWithOptions(directory, request, new NetdataFunctionRunOptions());
  }

  runDirectoryRequestBytesWithOptions(directory, request, options) {
    const text = typeof request === 'string' ? request
      : Buffer.isBuffer(request) ? request.toString('utf8')
      : String(request);
    let obj;
    try { obj = JSON.parse(text); }
    catch (err) { throw new Error(`invalid Netdata function JSON: ${err.message}`); }
    return this.runDirectoryRequestJsonWithOptions(directory, obj, options);
  }

  _exploreFiles(paths, request, options, initialSkipped, initialErrors) {
    const combined = new CombinedResult();
    combined.skippedFiles = initialSkipped || 0;
    combined.fileErrors = (initialErrors || []).map(String);
    if (!paths || paths.length === 0) return combined;
    const deadline = _computeDeadline(options);
    const state = options?.state;
    let matchedPaths = paths;
    matchedPaths = matchedPaths.filter(p => _pathMatchesRequest(p, request, state));
    const matchedFilesCount = matchedPaths.length;
    const totalFiles = matchedFilesCount;
    for (const pathStr of matchedPaths) {
      if (_shouldStopBeforeFile(combined, deadline, options)) break;
      const metadata = _stateFileMetadata(state, pathStr);
      if (!_fileOverlapsRequestWindow(pathStr, metadata, request.afterRealtimeUsec, request.beforeRealtimeUsec)) {
        combined.skippedFiles += 1;
        _emitProgressForCombined(options, combined, combined.matchedPaths.length + 1, totalFiles, deadline);
        continue;
      }
      let realtimeSlack = NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC;
      if (metadata != null && metadata.journalVsRealtimeDeltaUsec != null) {
        realtimeSlack = metadata.journalVsRealtimeDeltaUsec;
      }
      let reader;
      try { reader = FileReader.open(pathStr); }
      catch (err) { combined.skippedFiles += 1; combined.fileErrors.push(`${pathStr}: ${err.message}`); _emitProgressForCombined(options, combined, combined.matchedPaths.length + 1, totalFiles, deadline); continue; }
      try {
        const fileQuery = _requestToExplorerQuery(request, matchedFilesCount, reader, realtimeSlack);
        const control = new ExplorerControl();
        if (deadline != null) control.setDeadline(deadline);
        if (options?.cancellationCallback) control.setCancellationCallback(options.cancellationCallback);
        if (options?.progressInterval != null) control.setProgressIntervalMs(options.progressInterval * 1000);
        let result;
        try {
          result = exploreWithStrategyAndControl(reader, fileQuery, ExplorerStrategy.Traversal, control);
        } catch (err) { combined.skippedFiles += 1; combined.fileErrors.push(`${pathStr}: ${err.message}`); continue; }
        if (control.stopReason === ExplorerStopReason.Cancelled) combined.cancelled = true;
        else if (control.stopReason === ExplorerStopReason.TimedOut) combined.timedOut = true;
        if (typeof reader._enumerateFieldsIndexed === 'function') {
          for (const f of reader._enumerateFieldsIndexed()) combined.columnFields.add(f);
        }
        _updateLearnedRealtimeDelta(state, pathStr, realtimeSlack, result.stats);
        combined.matchedFiles += 1;
        combined.matchedPaths.push(pathStr);
        combined.merge(pathStr, result, request.direction, request.limit);
        _emitProgressForCombined(options, combined, combined.matchedPaths.length, totalFiles, deadline);
        if (combined.cancelled) break;
      } finally {
        try { reader.close(); } catch {}
      }
    }
    if (!request.dataOnly && !combined.cancelled) {
      combined.add_zero_count_facet_values_from_files(request.facets);
      combined.add_zero_count_selected_filter_values(request);
    }
    const analysisEnabled = !request.dataOnly || request.delta;
    if (analysisEnabled && request.sampling !== 0 && matchedFilesCount !== 0) {
      combined.samplingEnabled = true;
      _applySamplingBudget(combined, request.sampling);
    }
    return combined;
  }
}

function _pathMatchesRequest(pathStr, request, state) {
  if (request.sourceType === NETDATA_SOURCE_TYPE_ALL && request.exactSources.length === 0) return true;
  if (request.sourceType & NETDATA_SOURCE_TYPE_ALL) return true;
  const metadata = _stateFileMetadata(state, pathStr);
  let st;
  if (metadata != null && metadata.sourceType != null) {
    st = metadata.sourceType;
  } else {
    st = journalFileSourceType(pathStr);
  }
  if (st & request.sourceType) return true;
  if (!request.exactSources.length) return false;
  let name;
  if (metadata != null && metadata.sourceName != null) {
    name = metadata.sourceName;
  } else {
    name = _journalFileExactSourceName(pathStr);
  }
  return name != null && request.exactSources.includes(name);
}

function _tailAfterRealtimeBound(afterRealtimeUsec, anchor) {
  if (anchor.kind !== ExplorerAnchorKind.Realtime) return afterRealtimeUsec;
  const tailAfter = Number(anchor.realtimeUsec) + 1;
  if (afterRealtimeUsec == null) return tailAfter;
  return Math.max(Number(afterRealtimeUsec), tailAfter);
}

function _beforeRealtimeBoundExcludingAnchor(beforeRealtimeUsec, anchor) {
  if (anchor.kind !== ExplorerAnchorKind.Realtime) return beforeRealtimeUsec;
  const beforeAnchor = Math.max(0, Number(anchor.realtimeUsec) - 1);
  if (beforeRealtimeUsec == null) return beforeAnchor;
  return Math.min(Number(beforeRealtimeUsec), beforeAnchor);
}

// Exported for internal tests only; not part of the public src/index.js surface.
// `_matchedFiles` and `_reader` are reserved for the sampling-budget query
// construction (Rust ExplorerSamplingState, explorer.rs:456-590), which is
// not yet ported here or in the Python explorer; tracked by SOW-0107.
export function _requestToExplorerQuery(
  request, _matchedFiles, _reader,
  realtimeSlackUsec = NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC,
) {
  const tailAnchor = request.tail
    && request.anchor.kind === ExplorerAnchorKind.Realtime;
  const backwardPageAnchor = request.dataOnly
    && !tailAnchor
    && request.direction === ExplorerDirection.Backward
    && request.anchor.kind === ExplorerAnchorKind.Realtime;

  let afterUsec = request.afterRealtimeUsec;
  let beforeUsec = request.beforeRealtimeUsec;

  if (tailAnchor) {
    afterUsec = _tailAfterRealtimeBound(afterUsec, request.anchor);
  }
  if (backwardPageAnchor) {
    beforeUsec = _beforeRealtimeBoundExcludingAnchor(beforeUsec, request.anchor);
  }

  const anchorForQuery = (tailAnchor || backwardPageAnchor)
    ? ExplorerAnchorClass.auto()
    : request.anchor;

  const query = new ExplorerQuery();
  query.afterRealtimeUsec = afterUsec != null ? BigInt(afterUsec) : null;
  query.beforeRealtimeUsec = beforeUsec != null ? BigInt(beforeUsec) : null;
  query.anchor = anchorForQuery;
  query.direction = request.direction;
  query.limit = request.limit;
  query.filters = [...request.filters];
  const analysisEnabled = !request.dataOnly || request.delta;
  if (analysisEnabled) {
    query.facets = request.facets.map(f => Buffer.isBuffer(f) ? f : Buffer.from(f));
    if (request.histogram != null) query.histogram = Buffer.from(request.histogram, 'utf8');
  }
  query.histogramAfterRealtimeUsec = request.afterRealtimeUsec != null ? BigInt(request.afterRealtimeUsec) : null;
  query.histogramBeforeRealtimeUsec = request.beforeRealtimeUsec != null ? BigInt(request.beforeRealtimeUsec) : null;
  query.histogramTargetBuckets = DEFAULT_HISTOGRAM_BUCKETS;
  query.fieldMode = ExplorerFieldMode.FirstValue;
  query.excludeFacetFieldFilters = new Set(request.filters.map(f => f.field.toString('hex'))).size > 1;
  query.useSourceRealtime = true;
  // Per-file learned journal-vs-realtime delta (Rust netdata.rs:1625),
  // defaulting to the global default on the first request for a file.
  query.realtimeSlackUsec = BigInt(
    _normalizeJournalVsRealtimeDeltaUsec(Number(realtimeSlackUsec)));
  // Data-only requests early-stop a file once its page is full (Rust
  // to_explorer_query, netdata.rs:1609-1610). A tail anchor already
  // bounds the window, and a delta re-scan must read the whole file
  // (Rust file_query override, netdata.rs:1627-1629), so neither
  // early-stops. The combined guard equals Rust's two-step result.
  query.stopWhenRowsFull = request.dataOnly && !tailAnchor && !request.delta;
  query.stopWhenRowsFullCheckEvery = DATA_ONLY_CHECK_EVERY_ROWS;
  return query;
}
