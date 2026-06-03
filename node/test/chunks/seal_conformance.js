import * as support from '../support.js';

export async function run() {
  const { closeSync, mkdtempSync, rmSync, writeSync, createRequire, tmpdir, basename, dirname, join, relative, resolve, fileURLToPath, spawnSync, zstdCompressSync, createHash, assert, jenkinsHash64, sipHash24, uuidToString, DEFAULT_COMPRESS_THRESHOLD, FIELD_NAME_POLICY_JOURNAL_APP, FIELD_NAME_POLICY_RAW, MIN_COMPRESS_THRESHOLD, Writer, Log, FileReader, DirectoryReader, parseDataObject, parseEntryObject, exportEntry, jsonEntry, SdJournalOpen, SdJournalOpenFiles, SdJournalQueryUnique, SdJournalNext, SdJournalPrevious, SdJournalSeekRealtimeUsec, SdJournalSeekCursor, SdJournalGetEntry, SdJournalGetCursor, SdJournalTestCursor, SdJournalGetSeqnum, SdJournalGetMonotonicUsec, SdJournalRestartData, SdJournalEnumerateAvailableData, SdJournalGetData, SdJournalQueryUniqueState, SdJournalEnumerateAvailableUnique, SdJournalRestartFields, SdJournalEnumerateField, DATA_OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE, HEADER_SIZE, INCOMPATIBLE_COMPACT, INCOMPATIBLE_COMPRESSED_LZ4, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_KEYED_HASH, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_ZSTD, OBJECT_TYPE_DATA, OBJECT_TYPE_ENTRY, OBJECT_TYPE_TAG, FILE_SIZE_INCREASE, JOURNAL_COMPACT_SIZE_MAX, STATE_ARCHIVED, DEFAULT_FIELD_HASH_BUCKETS, dataHashBucketsForMaxFileSize, parseFileHeader, parseObjectHeader, writeObjectHeader, compressLz4DataPayload, compressXzDataPayload, decompressXzDataPayload, decompressZstSync, fsprgGenMK, fsprgGenState0, fsprgEvolve, fsprgSeek, fsprgGetKey, fsprgGetEpoch, verifyFile, verifyFileWithKey, VerificationError, SealOptions, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS, WriterLock, UNKNOWN_PROCESS_START_TIME, lockOwnerIsActive, parseLinuxProcStatStartTime, readHostBootId, readHostBootIdText, safeExistsSync, safeMkdirSync, safeOpenSync, safeReadFileSync, safeReaddirSync, safeStatSync, safeSymlinkSync, safeWriteFileSync, here, packageRoot, repoRoot, validFSSVerificationKey, listJavaScriptFiles, run, journalFiles, disposedJournalFiles, clearKeyedHashFlag, writeHeaderSize, collectNullable, journalctlAvailable, verifyJournalFileIfAvailable, verifyJournalFileFailsIfAvailable, journalctlDirectoryRowsIfAvailable, verifyJournalFileWithKeyIfAvailable, verifyJournalFileWithKeyFailsIfAvailable, journalHasDataObjectFlag, makeHistoricalHeaderFixture } = support;
  // journalctl command verify tests
  {
    const validPath = join(repoRoot, 'fixtures/systemd/test-data/no-rtc/system.journal.zst');
    const corruptPath = join(repoRoot, 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst');
    const cmd = process.execPath;
    const script = join(packageRoot, 'cmd/journalctl/index.js');

    // --verify valid file
    {
      const result = spawnSync(cmd, [script, '--verify', '--file', validPath], { encoding: 'utf8' });
      if (result.status !== 0) {
        throw new Error(`--verify valid file failed: stderr=${result.stderr}`);
      }
      if (result.stdout !== '') {
        throw new Error(`expected no stdout, got: ${result.stdout}`);
      }
      if (!result.stderr.includes('PASS:')) {
        throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
      }
    }

    // --verify-only valid file (no normal output)
    {
      const result = spawnSync(cmd, [script, '--verify-only', '--file', validPath], { encoding: 'utf8' });
      if (result.status !== 0) {
        throw new Error(`--verify-only valid file failed: stderr=${result.stderr}`);
      }
      if (result.stdout !== '') {
        throw new Error(`expected no stdout, got: ${result.stdout}`);
      }
      if (!result.stderr.includes('PASS:')) {
        throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
      }
    }

    // --verify directory follows symlinked journals and skips directories
    {
      const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-dir-'));
      safeSymlinkSync(validPath, join(tmpDir, 'linked.journal.zst'));
      safeMkdirSync(join(tmpDir, 'skip.journal.zst'));

      const result = spawnSync(cmd, [script, '--verify', '--directory', tmpDir], { encoding: 'utf8' });
      rmSync(tmpDir, { recursive: true });
      if (result.status !== 0) {
        throw new Error(`--verify directory failed: stderr=${result.stderr}`);
      }
      if (result.stdout !== '') {
        throw new Error(`expected no stdout, got: ${result.stdout}`);
      }
      if ((result.stderr.match(/PASS:/g) || []).length !== 1) {
        throw new Error(`expected one PASS in stderr, got: ${result.stderr}`);
      }
      if (result.stderr.includes('FAIL:')) {
        throw new Error(`expected no FAIL in stderr, got: ${result.stderr}`);
      }
    }

    // --verify empty directory
    {
      const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-empty-dir-'));
      const result = spawnSync(cmd, [script, '--verify', '--directory', tmpDir], { encoding: 'utf8' });
      rmSync(tmpDir, { recursive: true });
      if (result.status !== 0) {
        throw new Error(`expected --verify empty directory to succeed: ${result.stderr}`);
      }
      if (result.stdout !== '') {
        throw new Error(`expected no stdout, got: ${result.stdout}`);
      }
      if (result.stderr !== '') {
        throw new Error(`expected no stderr, got: ${result.stderr}`);
      }
    }

    // --verify corrupted file
    {
      const result = spawnSync(cmd, [script, '--verify', '--file', corruptPath], { encoding: 'utf8' });
      if (result.status === 0) {
        throw new Error(`expected --verify corrupted file to fail`);
      }
      if (!result.stderr.includes('FAIL:')) {
        throw new Error(`expected FAIL in stderr, got: ${result.stderr}`);
      }
    }

    // --verify-key unsealed file (valid key parsed, normal verification)
    {
      const result = spawnSync(cmd, [script, '--verify-key', validFSSVerificationKey, '--file', validPath], { encoding: 'utf8' });
      if (result.status !== 0) {
        throw new Error(`--verify-key unsealed file failed: stderr=${result.stderr}`);
      }
      if (result.stdout !== '') {
        throw new Error(`expected no stdout, got: ${result.stdout}`);
      }
      if (!result.stderr.includes('PASS:')) {
        throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
      }
    }

    // --verify-key invalid seed
    {
      const result = spawnSync(cmd, [script, '--verify-key', 'synthetic-test-key', '--file', validPath], { encoding: 'utf8' });
      if (result.status === 0) {
        throw new Error(`expected --verify-key invalid seed to fail`);
      }
      if (result.stdout !== '') {
        throw new Error(`expected no stdout, got: ${result.stdout}`);
      }
      if (!result.stderr.includes('Failed to parse seed.')) {
        throw new Error(`expected parse seed error in stderr, got: ${result.stderr}`);
      }
    }

    // --verify-key empty seed
    {
      const result = spawnSync(cmd, [script, '--verify-key=', '--file', validPath], { encoding: 'utf8' });
      if (result.status === 0) {
        throw new Error(`expected --verify-key empty seed to fail`);
      }
      if (result.stdout !== '') {
        throw new Error(`expected no stdout, got: ${result.stdout}`);
      }
      if (!result.stderr.includes('Failed to parse seed.')) {
        throw new Error(`expected parse seed error in stderr, got: ${result.stderr}`);
      }
    }

    // --verify sealed file without key (key required)
    {
      const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-nokey-'));
      const tmpPath = join(tmpDir, 'sealed.journal');
      const src = safeReadFileSync(validPath);
      const decompressed = decompressZstSync(src);
      const buf = Buffer.from(decompressed);
      const flags = buf.readUInt32LE(8);
      buf.writeUInt32LE(flags | 1, 8); // set COMPATIBLE_SEALED
      safeWriteFileSync(tmpPath, buf);

      const result = spawnSync(cmd, [script, '--verify', '--file', tmpPath], { encoding: 'utf8' });
      rmSync(tmpDir, { recursive: true });
      if (result.status === 0) {
        throw new Error(`expected --verify sealed file without key to fail`);
      }
      if (!result.stderr.includes('verification key')) {
        throw new Error(`expected verification key message in stderr, got: ${result.stderr}`);
      }
      if (result.stderr.includes('PASS:')) {
        throw new Error(`sealed file without key should not pass, got: ${result.stderr}`);
      }
    }

    // --verify-key sealed file (valid)
    {
      const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-valid-'));
      const journalPath = join(tmpDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'sealed verify' }], { realtimeUsec: 1500000n });
      writer.close();
      const key = testVerificationKey(testSealOpts());
      const result = spawnSync(cmd, [script, '--verify-key', key, '--file', journalPath], { encoding: 'utf8' });
      rmSync(tmpDir, { recursive: true });
      if (result.status !== 0) {
        throw new Error(`expected --verify-key sealed file to pass, got: ${result.stderr}`);
      }
      if (!result.stderr.includes('PASS:')) {
        throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
      }
    }

    // --verify-key sealed file wrong key
    {
      const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-wrong-'));
      const journalPath = join(tmpDir, 'sealed.journal');
      const writer = Writer.create(journalPath, { seal: testSealOpts() });
      writer.append([{ name: 'MESSAGE', value: 'sealed verify' }], { realtimeUsec: 1500000n });
      writer.close();
      const wrongKey = '000000000000000000000001/1-f4240';
      const result = spawnSync(cmd, [script, '--verify-key', wrongKey, '--file', journalPath], { encoding: 'utf8' });
      rmSync(tmpDir, { recursive: true });
      if (result.status === 0) {
        throw new Error(`expected --verify-key sealed file with wrong key to fail`);
      }
      if (!result.stderr.includes('FAIL:')) {
        throw new Error(`expected FAIL in stderr, got: ${result.stderr}`);
      }
    }
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
    let offset = headerSize;
    let tagCount = 0;
    let secondTagOffset = 0;
    let targetPayloadOffset = 0;
    let targetObjectOffset = 0;

    while (offset + 16 <= buf.length) {
      const header = parseObjectHeader(buf, offset);
      if (!header || header.size < 16n) throw new Error(`invalid object at ${offset}`);
      const aligned = Number(((header.size + 7n) / 8n) * 8n);
      if (offset + aligned > buf.length) throw new Error(`object at ${offset} exceeds file`);

      if (header.type === OBJECT_TYPE_TAG) {
        tagCount += 1;
        if (tagCount === 2) secondTagOffset = offset;
      } else if (header.type === OBJECT_TYPE_DATA) {
        const payloadOffset = compact ? COMPACT_DATA_OBJECT_HEADER_SIZE : DATA_OBJECT_HEADER_SIZE;
        if (header.size > BigInt(payloadOffset)) {
          const start = offset + payloadOffset;
          const end = offset + Number(header.size);
          if (buf.slice(start, end).equals(expectedPayload)) {
            targetPayloadOffset = start;
            targetObjectOffset = offset;
          }
        }
      }

      if (offset === tailObjectOffset) break;
      offset += aligned;
    }

    if (targetPayloadOffset === 0) throw new Error(`payload not found: ${expectedPayload}`);
    if (secondTagOffset === 0) throw new Error('second TAG not found');
    if (targetObjectOffset >= secondTagOffset) {
      throw new Error(`DATA object ${targetObjectOffset} is not covered by second TAG ${secondTagOffset}`);
    }
    buf.writeUInt8(buf.readUInt8(targetPayloadOffset) ^ 0x01, targetPayloadOffset);
    safeWriteFileSync(path, buf);
  }

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

  const manifestPath = join(repoRoot, 'tests/conformance/manifests/conformance-v01.json');
  if (!safeExistsSync(manifestPath)) {
    throw new Error(`missing conformance manifest: ${manifestPath}`);
  }

  const manifest = JSON.parse(safeReadFileSync(manifestPath, 'utf8'));
  const failures = [];
  const results = [];
  const expectedSkips = new Set();

  for (const testCase of manifest.test_suite.test_cases) {
    const stdout = run(process.execPath, ['node/adapter/index.js', 'run'], {
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
