import * as support from '../support.js';

export async function run() {
  const { mkdtempSync, rmSync, tmpdir, join, assert, Log, FileReader, fsprgGenMK, fsprgGenState0, fsprgEvolve, fsprgSeek, fsprgGetKey, fsprgGetEpoch, verifyFile, VerificationError, safeReadFileSync, safeReaddirSync, packageRoot, repoRoot, listJavaScriptFiles, run, journalFiles, verifyJournalFileIfAvailable, journalctlDirectoryRowsIfAvailable } = support;

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const config = {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 1,
        maxFiles: 0,
      };
      const first = new Log(tempDir, config);
      for (let i = 0; i < 2; i++) {
        first.append([{ name: 'MESSAGE', value: `construction-retention-${i}` }]);
      }
      first.close();
      const journalDir = first.journalDirectory();
      const before = journalFiles(journalDir);
      assert.equal(before.length, 2);

      const events = [];
      const second = new Log(tempDir, { ...config, maxFiles: 1, lifecycle: (event) => events.push(event) });
      const after = journalFiles(journalDir);
      assert.deepEqual(after, before);
      second.append([
        { name: 'MESSAGE', value: 'construction-retention-open' },
        { name: 'TEST_ID', value: 'node-retention-on-open' },
      ]);
      const afterAppend = journalFiles(journalDir);
      assert.deepEqual(afterAppend, [second.activeFile()]);
      assert.ok(events.some((event) => event.type === 'deleted'));
      verifyJournalFileIfAvailable(second.activeFile());
      const rows = journalctlDirectoryRowsIfAvailable(journalDir, 'TEST_ID=node-retention-on-open');
      if (rows !== null) assert.equal(rows.length, 1);
      second.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  for (const retentionCase of [
    { name: 'files', options: { maxFiles: 1 } },
    { name: 'bytes', options: { maxRetentionBytes: 1 }, artifact: true },
    { name: 'age', options: { maxRetentionAgeUsec: 1n } },
  ]) {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const config = {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 1,
        maxFiles: 0,
      };
      const first = new Log(tempDir, config);
      for (let i = 0; i < 3; i++) {
        first.append(
          [{ name: 'MESSAGE', value: `open-retention-${retentionCase.name}-${i}` }],
          { realtimeUsec: BigInt(1_700_002_276_000_000 + i), monotonicUsec: BigInt(i + 1) },
        );
      }
      first.close();
      const journalDir = first.journalDirectory();
      assert.equal(journalFiles(journalDir).length, 3);

      const events = [];
      const artifactCalls = [];
      const retained = new Log(tempDir, {
        ...config,
        maxEntries: 0,
        openMode: 'eager',
        ...retentionCase.options,
        lifecycle: (event) => events.push(event),
        artifactSizer: retentionCase.artifact
          ? (path) => {
            artifactCalls.push(path);
            return 4096;
          }
          : undefined,
      });
      const files = journalFiles(journalDir);
      assert.deepEqual(files, [retained.activeFile()]);
      assert.ok(events.some((event) => event.type === 'created' && event.reason === 'eager_open'));
      assert.ok(events.some((event) => event.type === 'deleted'));
      if (retentionCase.artifact) assert.ok(artifactCalls.length > 0);
      verifyJournalFileIfAvailable(retained.activeFile());
      retained.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        strictSystemdNaming: true,
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 100,
        maxFiles: 10,
      });
      log.append([{ name: 'MESSAGE', value: 'strict naming' }]);
      assert.equal(log.activeFile(), join(log.journalDirectory(), 'system.journal'));
      log.close();
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
      assert.equal(files.length, 1);
      assert.match(files.at(0), /^system@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 1,
        maxFiles: 1,
        maxRetentionBytes: 1024 * 1024 * 1024,
      });
      for (let i = 0; i < 3; i++) {
        log.append([{ name: 'MESSAGE', value: `retention-active-${i}` }]);
      }
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
      assert.equal(files.length, 1);
      log.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      });
      assert.equal(log.bootID().length, 16);
      log.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const machineId = Buffer.from('00112233445566778899aabbccddeeff', 'hex');
      const bootId = Buffer.from('ffeeddccbbaa99887766554433221100', 'hex');
      assert.throws(
        () => new Log(tempDir, { identityMode: 'strict', machineId }),
        /strict identity requires boot id/,
      );

      const events = [];
      const log = new Log(tempDir, {
        source: 'system',
        openMode: 'eager',
        identityMode: 'strict',
        machineId,
        bootId,
        lifecycle: (event) => events.push(event),
      });
      assert.equal(log.configuredDirectory(), tempDir);
      assert.equal(log.journalDirectory(), join(tempDir, '00112233445566778899aabbccddeeff'));
      assert.equal(log.machineID().toString('hex'), machineId.toString('hex'));
      assert.equal(log.bootID().toString('hex'), bootId.toString('hex'));
      assert.equal(log.sourceName(), 'system');
      assert.notEqual(log.activeFilePath(), '');
      assert.equal(events.length, 1);
      assert.equal(events[0].type, 'created');
      assert.equal(events[0].reason, 'eager_open');
      assert.equal(events[0].activePath, log.activeFilePath());

      log.append(
        [{ name: 'MESSAGE', value: 'timestamp-0' }],
        { realtimeUsec: 1_700_000_100_000_000n, monotonicUsec: 10n, sourceRealtimeUsec: 999n },
      );
      log.append(
        [{ name: 'MESSAGE', value: 'timestamp-1' }],
        { realtimeUsec: 1_700_000_100_000_000n, monotonicUsec: 10n, sourceRealtimeUsec: 1000n },
      );
      log.append(
        [{ name: 'MESSAGE', value: 'timestamp-2' }],
        { realtimeUsec: 1_700_000_100_000_000n, monotonicUsec: 0n, sourceRealtimeUsec: 1001n },
      );
      log.append(
        [{ name: 'MESSAGE', value: 'timestamp-3' }],
        { realtimeUsec: 0n, monotonicUsec: 13n, sourceRealtimeUsec: 1002n },
      );
      const path = log.activeFilePath();
      log.close();
      verifyJournalFileIfAvailable(path);

      const reader = FileReader.open(path);
      const entries = [];
      while (reader.step()) entries.push(reader.getEntry());
      reader.close();
      assert.equal(entries.length, 4);
      assert.deepEqual(entries.map((entry) => entry.realtime), [
        1_700_000_100_000_000n,
        1_700_000_100_000_001n,
        1_700_000_100_000_002n,
        1_700_000_100_000_003n,
      ]);
      assert.deepEqual(entries.map((entry) => entry.monotonic), [10n, 11n, 12n, 13n]);
      assert.deepEqual(
        entries.map((entry) => entry.fields._SOURCE_REALTIME_TIMESTAMP.toString('utf8')),
        ['999', '1000', '1001', '1002'],
      );
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      assert.throws(() => new Log(tempDir, { rotationPolicy: { maxEntries: 0 } }), /rotation max entries/);
      assert.throws(() => new Log(tempDir, { retentionPolicy: { maxFiles: 0 } }), /retention max files/);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const events = [];
      const artifactCalls = [];
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 1,
        retentionPolicy: { maxBytes: 1 },
        lifecycle: (event) => events.push(event),
        artifactSizer: (path) => {
          artifactCalls.push(path);
          return 4096;
        },
      });
      log.append([{ name: 'MESSAGE', value: 'artifact-retention-0' }]);
      log.append([{ name: 'MESSAGE', value: 'artifact-retention-1' }]);
      assert.ok(artifactCalls.length > 0);
      assert.ok(events.some((event) => event.type === 'created' && event.reason === 'append'));
      assert.ok(events.some((event) => event.type === 'rotated'));
      const deleted = events.find((event) => event.type === 'deleted');
      assert.ok(deleted);
      assert.equal(deleted.deletedPaths.length, 1);
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal'));
      assert.equal(files.length, 1);
      log.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        strictSystemdNaming: true,
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 100,
        maxFiles: 10,
        maxRetentionBytes: 1,
      });
      log.append([
        { name: 'MESSAGE', value: 'strict byte retained' },
        { name: 'TEST_ID', value: 'node-strict-byte-retention' },
      ]);
      log.close();
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
      assert.equal(files.length, 1);
      const reader = FileReader.open(join(log.journalDirectory(), files.at(0)));
      assert.equal(reader.step(), true);
      assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'strict byte retained');
      reader.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 100,
        maxFiles: 10,
      });
      log.append([{ name: 'MESSAGE', value: 'archive failure cleanup' }]);
      const originalArchiveTo = log.writer.archiveTo.bind(log.writer);
      log.writer.archiveTo = (path) => {
        originalArchiveTo(path);
        throw new Error('synthetic post-archive failure');
      };
      assert.throws(() => log.close(), /synthetic post-archive failure/);
      assert.equal(log.closed, true);
      assert.equal(log.writer, null);
      assert.doesNotThrow(() => log.close());
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 1,
        maxFiles: 10,
      });
      log.append([{ name: 'MESSAGE', value: 'rotation failure first' }]);
      const originalArchiveTo = log.writer.archiveTo.bind(log.writer);
      log.writer.archiveTo = (path) => {
        originalArchiveTo(path);
        throw new Error('synthetic post-rotation failure');
      };
      assert.throws(
        () => log.append([{ name: 'MESSAGE', value: 'rotation failure second' }]),
        /synthetic post-rotation failure/,
      );
      assert.equal(log.closed, false);
      assert.equal(log.writer, null);
      log.append([{ name: 'MESSAGE', value: 'rotation failure second' }]);
      log.close();

      const seqnums = [];
      for (const name of safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort()) {
        const reader = FileReader.open(join(log.journalDirectory(), name));
        while (reader.step()) seqnums.push(reader.getEntry().seqnum);
        reader.close();
      }
      assert.deepEqual(seqnums, [1n, 2n]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const options = {
        source: 'system',
        strictSystemdNaming: true,
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 100,
        maxFiles: 10,
      };
      const first = new Log(tempDir, options);
      first.append([{ name: 'MESSAGE', value: 'strict-reopen-0' }]);
      first.close();
      const second = new Log(tempDir, options);
      second.append([{ name: 'MESSAGE', value: 'strict-reopen-1' }]);
      second.close();
      const seqnums = [];
      for (const name of safeReaddirSync(second.journalDirectory()).filter((name) => name.endsWith('.journal')).sort()) {
        const reader = FileReader.open(join(second.journalDirectory(), name));
        while (reader.step()) seqnums.push(reader.getEntry().seqnum);
        reader.close();
      }
      assert.deepEqual(seqnums, [1n, 2n]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const options = {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 0,
        maxBytes: 0,
        maxFiles: 10,
      };
      const first = new Log(tempDir, options);
      first.append([{ name: 'MESSAGE', value: 'chain-reopen-0' }]);
      first.append([{ name: 'MESSAGE', value: 'chain-reopen-1' }]);
      first.close();

      const second = new Log(tempDir, options);
      second.append([{ name: 'MESSAGE', value: 'chain-reopen-2' }]);
      second.close();

      const seqnums = [];
      for (const name of safeReaddirSync(second.journalDirectory()).filter((name) => name.endsWith('.journal')).sort()) {
        const reader = FileReader.open(join(second.journalDirectory(), name));
        while (reader.step()) seqnums.push(reader.getEntry().seqnum);
        reader.close();
      }
      assert.deepEqual(seqnums, [1n, 2n, 3n]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 0,
        maxBytes: 0,
        maxFiles: 10,
      });
      for (let i = 0; i < 3; i++) log.append([{ name: 'MESSAGE', value: `no-rotation-${i}` }]);
      log.close();
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal'));
      assert.equal(files.length, 1);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  assert.throws(() => new Log('/tmp', { source: '../bad' }), /invalid journal source/);

  for (const file of listJavaScriptFiles(packageRoot).sort()) {
    run(process.execPath, ['--check', file], { cwd: repoRoot });
  }

  run(process.execPath, ['-e', "import './node/src/index.js'"], { cwd: repoRoot });

  {
    // FSPRG vector tests against committed systemd v260.1 fixture.
    const vectorsPath = join(repoRoot, 'tests/fss/fixtures/fsprg-vectors-v01.json');
    const vectorsData = JSON.parse(safeReadFileSync(vectorsPath, 'utf8'));
    const secpar = vectorsData.fsprg_params.secpar;
    for (const vec of vectorsData.vectors) {
      const seed = Buffer.from(vec.seed_hex, 'hex');
      const expectedMsk = Buffer.from(vec.msk_hex, 'hex');
      const expectedMpk = Buffer.from(vec.mpk_hex, 'hex');
      const expectedState0 = Buffer.from(vec.state0_hex, 'hex');

      const { msk, mpk } = fsprgGenMK(seed, secpar);
      assert.deepEqual(msk, expectedMsk, `msk mismatch for ${vec.seed_desc}`);
      assert.deepEqual(mpk, expectedMpk, `mpk mismatch for ${vec.seed_desc}`);

      const state0 = fsprgGenState0(mpk, seed);
      assert.deepEqual(state0, expectedState0, `state0 mismatch for ${vec.seed_desc}`);
      assert.equal(fsprgGetEpoch(state0), 0n, `epoch0 mismatch for ${vec.seed_desc}`);

      for (const ep of vec.epochs) {
        let evolved = state0;
        for (let e = 0n; e < BigInt(ep.epoch); e++) {
          evolved = fsprgEvolve(evolved);
        }
        assert.deepEqual(evolved, Buffer.from(ep.state_hex, 'hex'), `evolve mismatch for ${vec.seed_desc} epoch ${ep.epoch}`);

        const seeked = fsprgSeek(state0, BigInt(ep.epoch), msk, seed);
        assert.deepEqual(seeked, Buffer.from(ep.seek_state_hex, 'hex'), `seek mismatch for ${vec.seed_desc} epoch ${ep.epoch}`);

        for (const k of ep.keys) {
          const key = fsprgGetKey(evolved, k.keylen, k.idx);
          assert.deepEqual(key, Buffer.from(k.key_hex, 'hex'), `key mismatch for ${vec.seed_desc} epoch ${ep.epoch} idx ${k.idx}`);
        }
      }
    }
  }

  // Verification tests
  {
    const path = join(repoRoot, 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst');
    try {
      verifyFile(path);
      throw new Error('expected VerificationError for truncated zstd frame');
    } catch (err) {
      if (!(err instanceof VerificationError)) throw new Error(`expected VerificationError, got ${err.constructor.name}`);
      if (!err.message.includes('corrupt')) throw new Error(`expected error to contain 'corrupt', got: ${err.message}`);
    }
  }

  {
    const path = join(repoRoot, 'fixtures/systemd/test-data/no-rtc/system.journal.zst');
    verifyFile(path); // should not throw
  }
}
