import * as support from '../support.js';

const { mkdtempSync, rmSync, tmpdir, join, relative, spawnSync, zstdCompressSync, assert, Writer, DATA_OBJECT_HEADER_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE, INCOMPATIBLE_COMPACT, OBJECT_TYPE_DATA, OBJECT_TYPE_TAG, parseObjectHeader, decompressZstSync, verifyFileWithKey, VerificationError, SealOptions, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS, safeExistsSync, safeMkdirSync, safeReadFileSync, safeStatSync, safeSymlinkSync, safeWriteFileSync, packageRoot, repoRoot, repositoryRoot, validFSSVerificationKey, run: runCommand, verifyJournalFileWithKeyIfAvailable, verifyJournalFileWithKeyFailsIfAvailable } = support;

function runJournalctl(args) {
  return spawnSync(process.execPath, [join(packageRoot, 'cmd/journalctl/index.js'), ...args], { encoding: 'utf8' });
}

function assertCommandPass(result, label, passCount = 1) {
  assert.equal(result.status, 0, `${label}: stderr=${result.stderr}`);
  assert.equal(result.stdout, '', `${label}: expected no stdout`);
  assert.equal((result.stderr.match(/PASS:/g) || []).length, passCount, `${label}: stderr=${result.stderr}`);
  assert.equal(result.stderr.includes('FAIL:'), false, `${label}: expected no FAIL`);
}

function assertCommandFailsWith(result, label, expectedStderr) {
  assert.notEqual(result.status, 0, `${label}: expected command to fail`);
  assert.equal(result.stdout, '', `${label}: expected no stdout`);
  assert.equal(result.stderr.includes(expectedStderr), true, `${label}: stderr=${result.stderr}`);
}

function verifyJournalctlDirectoryCommand(validPath) {
  const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-dir-'));
  try {
    safeSymlinkSync(validPath, join(tmpDir, 'linked.journal.zst'));
    safeMkdirSync(join(tmpDir, 'skip.journal.zst'));
    assertCommandPass(runJournalctl(['--verify', '--directory', tmpDir]), '--verify directory', 1);
  } finally {
    rmSync(tmpDir, { recursive: true });
  }
}

function verifyJournalctlEmptyDirectoryCommand() {
  const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-empty-dir-'));
  try {
    const result = runJournalctl(['--verify', '--directory', tmpDir]);
    assert.equal(result.status, 0, `expected --verify empty directory to succeed: ${result.stderr}`);
    assert.equal(result.stdout, '', 'expected no stdout');
    assert.equal(result.stderr, '', 'expected no stderr');
  } finally {
    rmSync(tmpDir, { recursive: true });
  }
}

function verifyJournalctlSealedWithoutKey(validPath) {
  const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-nokey-'));
  try {
    const tmpPath = join(tmpDir, 'sealed.journal');
    const buf = Buffer.from(decompressZstSync(safeReadFileSync(validPath)));
    buf.writeUInt32LE(buf.readUInt32LE(8) | 1, 8); // set COMPATIBLE_SEALED
    safeWriteFileSync(tmpPath, buf);
    const result = runJournalctl(['--verify', '--file', tmpPath]);
    assertCommandFailsWith(result, '--verify sealed file without key', 'verification key');
    assert.equal(result.stderr.includes('PASS:'), false, 'sealed file without key should not pass');
  } finally {
    rmSync(tmpDir, { recursive: true });
  }
}

function createSealedJournal(tempDir) {
  const journalPath = join(tempDir, 'sealed.journal');
  const writer = Writer.create(journalPath, { seal: testSealOpts() });
  writer.append([{ name: 'MESSAGE', value: 'sealed verify' }], { realtimeUsec: 1500000n });
  writer.close();
  return journalPath;
}

function verifyJournalctlSealedWithKey() {
  const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-valid-'));
  try {
    const key = testVerificationKey(testSealOpts());
    const result = runJournalctl(['--verify-key', key, '--file', createSealedJournal(tmpDir)]);
    assertCommandPass(result, '--verify-key sealed file');
  } finally {
    rmSync(tmpDir, { recursive: true });
  }
}

function verifyJournalctlSealedWrongKey() {
  const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-wrong-'));
  try {
    const result = runJournalctl([
      '--verify-key',
      '000000000000000000000001/1-f4240',
      '--file',
      createSealedJournal(tmpDir),
    ]);
    assertCommandFailsWith(result, '--verify-key sealed file with wrong key', 'FAIL:');
  } finally {
    rmSync(tmpDir, { recursive: true });
  }
}

function verifyJournalctlCommandBehavior() {
  const validPath = join(repositoryRoot, 'fixtures/systemd/test-data/no-rtc/system.journal.zst');
  const corruptPath = join(repositoryRoot, 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst');

  assertCommandPass(runJournalctl(['--verify', '--file', validPath]), '--verify valid file');
  assertCommandPass(runJournalctl(['--verify-only', '--file', validPath]), '--verify-only valid file');
  verifyJournalctlDirectoryCommand(validPath);
  verifyJournalctlEmptyDirectoryCommand();
  assertCommandFailsWith(runJournalctl(['--verify', '--file', corruptPath]), '--verify corrupted file', 'FAIL:');
  assertCommandPass(
    runJournalctl(['--verify-key', validFSSVerificationKey, '--file', validPath]),
    '--verify-key unsealed file',
  );
  assertCommandFailsWith(
    runJournalctl(['--verify-key', 'synthetic-test-key', '--file', validPath]),
    '--verify-key invalid seed',
    'Failed to parse seed.',
  );
  assertCommandFailsWith(
    runJournalctl(['--verify-key=', '--file', validPath]),
    '--verify-key empty seed',
    'Failed to parse seed.',
  );
  verifyJournalctlSealedWithoutKey(validPath);
  verifyJournalctlSealedWithKey();
  verifyJournalctlSealedWrongKey();
}

function testSealOpts() {
  return new SealOptions(Buffer.alloc(12), 1_000_000, 1_000_000);
}

function testVerificationKey(opts) {
  const seedHex = opts.seed.toString('hex').padStart(24, '0');
  const start = Math.floor(Number(opts.startUsec) / Number(opts.intervalUsec));
  return `${seedHex}/${start.toString(16)}-${opts.intervalUsec.toString(16)}`;
}

function tamperDataPayload(path, expectedPayload) {
  const buf = Buffer.from(safeReadFileSync(path));
  const headerSize = Number(buf.readBigUInt64LE(88));
  const tailObjectOffset = Number(buf.readBigUInt64LE(136));
  const compact = (buf.readUInt32LE(12) & INCOMPATIBLE_COMPACT) !== 0;
  const scan = scanTamperTarget(buf, headerSize, tailObjectOffset, compact, expectedPayload);

  assert.notEqual(scan.targetPayloadOffset, 0, `payload not found: ${expectedPayload}`);
  assert.notEqual(scan.secondTagOffset, 0, 'second TAG not found');
  assert.ok(
    scan.targetObjectOffset < scan.secondTagOffset,
    `DATA object ${scan.targetObjectOffset} is not covered by second TAG ${scan.secondTagOffset}`,
  );
  buf.writeUInt8(buf.readUInt8(scan.targetPayloadOffset) ^ 0x01, scan.targetPayloadOffset);
  safeWriteFileSync(path, buf);
}

function scanTamperTarget(buf, headerSize, tailObjectOffset, compact, expectedPayload) {
  let offset = headerSize;
  const scan = {
    secondTagOffset: 0,
    tagCount: 0,
    targetObjectOffset: 0,
    targetPayloadOffset: 0,
  };
  while (offset + 16 <= buf.length) {
    const { aligned, header } = readTamperObjectHeader(buf, offset);
    updateTamperScan(scan, buf, offset, header, compact, expectedPayload);
    if (offset === tailObjectOffset) break;
    offset += aligned;
  }
  return scan;
}

function readTamperObjectHeader(buf, offset) {
  const header = parseObjectHeader(buf, offset);
  if (!header || header.size < 16n) throw new Error(`invalid object at ${offset}`);
  const aligned = Number(((header.size + 7n) / 8n) * 8n);
  if (offset + aligned > buf.length) throw new Error(`object at ${offset} exceeds file`);
  return { aligned, header };
}

function updateTamperScan(scan, buf, offset, header, compact, expectedPayload) {
  if (header.type === OBJECT_TYPE_TAG) {
    scan.tagCount += 1;
    if (scan.tagCount === 2) scan.secondTagOffset = offset;
    return;
  }
  if (header.type !== OBJECT_TYPE_DATA) return;
  const payloadOffset = compact ? COMPACT_DATA_OBJECT_HEADER_SIZE : DATA_OBJECT_HEADER_SIZE;
  if (header.size <= BigInt(payloadOffset)) return;
  const start = offset + payloadOffset;
  const end = offset + Number(header.size);
  if (!buf.slice(start, end).equals(expectedPayload)) return;
  scan.targetPayloadOffset = start;
  scan.targetObjectOffset = offset;
}

export async function run() {
  verifyJournalctlCommandBehavior();

  // Sealed verification API validates HMACs and keeps structural verification.
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sdk-verify-sealed-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'sealed-covered' }], { realtimeUsec: 1_500_000n });
      writer.append([{ name: 'MESSAGE', value: 'later-entry' }], { realtimeUsec: 2_500_000n });
      writer.close();

      const key = testVerificationKey(testSealOpts());
      verifyFileWithKey(journalPath, key);
      const zstPath = `${journalPath}.zst`;
      safeWriteFileSync(zstPath, zstdCompressSync(safeReadFileSync(journalPath)));
      verifyFileWithKey(zstPath, key);
      assert.throws(
        () => verifyFileWithKey(journalPath, '000000000000000000000001/1-f4240'),
        VerificationError,
      );
      assert.throws(
        () => verifyFileWithKey(journalPath, '000000000000000000000000/10000000000000000-f4240'),
        VerificationError,
      );
      assert.throws(
        () => verifyFileWithKey(journalPath, '000000000000000000000000/1-10000000000000000'),
        VerificationError,
      );

      tamperDataPayload(journalPath, Buffer.from('MESSAGE=sealed-covered'));
      assert.throws(() => verifyFileWithKey(journalPath, key), VerificationError);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Sealed writer basic
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-basic-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'hello sealed world' }, { name: 'PRIORITY', value: '6' }], {
        realtimeUsec: 1_500_000n,
      });
      writer.close();

      const key = testVerificationKey(testSealOpts());
      verifyJournalFileWithKeyIfAvailable(journalPath, key);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Sealed writer interval crossing
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-interval-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'epoch0' }], { realtimeUsec: 1_000_000n });
      writer.append([{ name: 'MESSAGE', value: 'epoch1' }], { realtimeUsec: 2_000_000n });
      writer.append([{ name: 'MESSAGE', value: 'epoch2' }], { realtimeUsec: 3_000_000n });
      writer.close();

      const key = testVerificationKey(testSealOpts());
      verifyJournalFileWithKeyIfAvailable(journalPath, key);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Unsealed writer does not set sealed flags
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-unsealed-flags-'));
    try {
      const journalPath = join(tempDir, 'unsealed.journal');
      const writer = Writer.create(journalPath);
      writer.append([{ name: 'MESSAGE', value: 'unsealed' }]);
      writer.close();
      const buf = safeReadFileSync(journalPath);
      assert.equal(buf.length >= 16, true);
      const compatibleFlags = buf.readUInt32LE(8);
      if (compatibleFlags & COMPATIBLE_SEALED) {
        throw new Error('unsealed writer set COMPATIBLE_SEALED flag');
      }
      if (compatibleFlags & COMPATIBLE_SEALED_CONTINUOUS) {
        throw new Error('unsealed writer set COMPATIBLE_SEALED_CONTINUOUS flag');
      }
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Writer file permissions follow systemd defaults and support override
  if (process.platform !== 'win32') {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-file-mode-'));
    try {
      const defaultPath = join(tempDir, 'default.journal');
      const defaultWriter = Writer.create(defaultPath);
      defaultWriter.close();
      assert.equal(safeStatSync(defaultPath).mode & 0o777, 0o640);

      const overridePath = join(tempDir, 'override.journal');
      const overrideWriter = Writer.create(overridePath, { fileMode: 0o600 });
      overrideWriter.close();
      assert.equal(safeStatSync(overridePath).mode & 0o777, 0o600);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Sealed writer first entry in a future epoch
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-first-future-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'future epoch first entry' }], {
        realtimeUsec: 3_000_000n,
      });
      writer.close();

      const key = testVerificationKey(testSealOpts());
      verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify first-entry future-epoch');
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Sealed writer rejects entries before the configured sealing start
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-before-start-'));
    let writer;
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      writer = Writer.create(journalPath, { seal: testSealOpts() });
      let rejected = false;
      try {
        writer.append([{ name: 'MESSAGE', value: 'before sealing start' }], {
          realtimeUsec: 500_000n,
        });
      } catch (_) {
        rejected = true;
      }
      if (!rejected) {
        throw new Error('expected before-start entry to be rejected');
      }
      writer.close();
    } finally {
      if (writer && !writer.closed) writer.close();
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Sealed writer handles a multi-interval epoch gap
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-multi-gap-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'epoch0' }], { realtimeUsec: 1_000_000n });
      writer.append([{ name: 'MESSAGE', value: 'epoch5' }], { realtimeUsec: 6_000_000n });
      writer.close();

      const key = testVerificationKey(testSealOpts());
      verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify multi-interval gap');
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Empty sealed writer produces a stock-verifiable file
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-empty-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.close();

      const key = testVerificationKey(testSealOpts());
      verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify empty sealed file');
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Compact sealed writer passes stock journalctl verify
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-compact-sealed-'));
    try {
      const journalPath = join(tempDir, 'compact-sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts(), compact: true });
      writer.append([{ name: 'MESSAGE', value: 'compact sealed' }, { name: 'PRIORITY', value: '6' }], {
        realtimeUsec: 1_500_000n,
      });
      writer.close();

      const key = testVerificationKey(testSealOpts());
      verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify compact+sealed');
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Sealed writer wrong key fails
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-wrong-key-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'hello' }], { realtimeUsec: 1_500_000n });
      writer.close();

      const wrongKey = '000000000000000000000001/1-f4240';
      verifyJournalFileWithKeyFailsIfAvailable(journalPath, wrongKey);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  // Sealed writer tamper fails
  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-tamper-'));
    try {
      const journalPath = join(tempDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'sealed-covered-stock' }], { realtimeUsec: 1_500_000n });
      writer.append([{ name: 'MESSAGE', value: 'later-entry' }], { realtimeUsec: 2_500_000n });
      writer.close();

      tamperDataPayload(journalPath, Buffer.from('MESSAGE=sealed-covered-stock'));

      const key = testVerificationKey(testSealOpts());
      verifyJournalFileWithKeyFailsIfAvailable(journalPath, key);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  const manifestPath = join(repositoryRoot, 'tests/conformance/manifests/conformance-v01.json');
  if (!safeExistsSync(manifestPath)) {
    throw new Error(`missing conformance manifest: ${manifestPath}`);
  }

  const manifest = JSON.parse(safeReadFileSync(manifestPath, 'utf8'));
  const failures = [];
  const results = [];
  const expectedSkips = new Set();

  for (const testCase of manifest.test_suite.test_cases) {
    const stdout = runCommand(process.execPath, ['node/adapter/index.js', 'run'], {
      cwd: repoRoot,
      input: JSON.stringify(testCase),
    });
    const result = JSON.parse(stdout);
    results.push(result);
    if (result.status === 'FAIL' || result.status === 'ERROR') {
      failures.push(result);
    }
    if (result.status === 'SKIP' && !expectedSkips.has(result.test_name)) {
      failures.push({ ...result, status: 'FAIL', error: `unexpected SKIP: ${result.note || ''}` });
    }
  }

  assert.equal(results.length, manifest.test_suite.test_cases.length);

  if (failures.length > 0) {
    for (const failure of failures) {
      process.stderr.write(`${failure.status}: ${failure.test_name}: ${failure.error || failure.note || ''}\n`);
    }
    throw new Error(`${failures.length} conformance case(s) failed`);
  }

  process.stdout.write(`PASS node package tests (${relative(repoRoot, manifestPath)})\n`);
}
