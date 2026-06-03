import * as support from '../support.js';

export async function run() {
  const { closeSync, mkdtempSync, rmSync, writeSync, createRequire, tmpdir, basename, dirname, join, relative, resolve, fileURLToPath, spawnSync, zstdCompressSync, createHash, assert, jenkinsHash64, sipHash24, uuidToString, DEFAULT_COMPRESS_THRESHOLD, FIELD_NAME_POLICY_JOURNAL_APP, FIELD_NAME_POLICY_RAW, MIN_COMPRESS_THRESHOLD, Writer, Log, FileReader, DirectoryReader, parseDataObject, parseEntryObject, exportEntry, jsonEntry, SdJournalOpen, SdJournalOpenFiles, SdJournalQueryUnique, SdJournalNext, SdJournalPrevious, SdJournalSeekRealtimeUsec, SdJournalSeekCursor, SdJournalGetEntry, SdJournalGetCursor, SdJournalTestCursor, SdJournalGetSeqnum, SdJournalGetMonotonicUsec, SdJournalRestartData, SdJournalEnumerateAvailableData, SdJournalGetData, SdJournalQueryUniqueState, SdJournalEnumerateAvailableUnique, SdJournalRestartFields, SdJournalEnumerateField, DATA_OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE, HEADER_SIZE, INCOMPATIBLE_COMPACT, INCOMPATIBLE_COMPRESSED_LZ4, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_KEYED_HASH, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_ZSTD, OBJECT_TYPE_DATA, OBJECT_TYPE_ENTRY, OBJECT_TYPE_TAG, FILE_SIZE_INCREASE, JOURNAL_COMPACT_SIZE_MAX, STATE_ARCHIVED, DEFAULT_FIELD_HASH_BUCKETS, dataHashBucketsForMaxFileSize, parseFileHeader, parseObjectHeader, writeObjectHeader, compressLz4DataPayload, compressXzDataPayload, decompressXzDataPayload, decompressZstSync, fsprgGenMK, fsprgGenState0, fsprgEvolve, fsprgSeek, fsprgGetKey, fsprgGetEpoch, verifyFile, verifyFileWithKey, VerificationError, SealOptions, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS, WriterLock, UNKNOWN_PROCESS_START_TIME, lockOwnerIsActive, parseLinuxProcStatStartTime, readHostBootId, readHostBootIdText, safeExistsSync, safeMkdirSync, safeOpenSync, safeReadFileSync, safeReaddirSync, safeStatSync, safeSymlinkSync, safeWriteFileSync, here, packageRoot, repoRoot, validFSSVerificationKey, listJavaScriptFiles, run, journalFiles, disposedJournalFiles, clearKeyedHashFlag, writeHeaderSize, collectNullable, journalctlAvailable, verifyJournalFileIfAvailable, verifyJournalFileFailsIfAvailable, journalctlDirectoryRowsIfAvailable, verifyJournalFileWithKeyIfAvailable, verifyJournalFileWithKeyFailsIfAvailable, journalHasDataObjectFlag, makeHistoricalHeaderFixture } = support;

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'binary-unique.journal');
      const writer = Writer.create(journalPath);
      writer.append([{ name: 'FIELD', value: Buffer.from([0xff]) }]);
      writer.append([{ name: 'FIELD', value: Buffer.from([0xef, 0xbf, 0xbd]) }]);
      writer.close();

      const reader = FileReader.open(journalPath);
      const values = reader.queryUnique('FIELD');
      reader.close();
      assert.equal(values.length, 2);

      const journal = SdJournalOpen(journalPath, 0);
      const facadeValues = SdJournalQueryUnique(journal, 'FIELD');
      journal.close();
      assert.equal(facadeValues.length, 2);
      assert.ok(facadeValues.some(([, value]) => value.equals(Buffer.from([0xff]))));
      assert.ok(facadeValues.some(([, value]) => value.equals(Buffer.from([0xef, 0xbf, 0xbd]))));
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'indexed-unique.journal');
      const writer = Writer.create(journalPath);
      for (const priority of ['0', '3', '6', '7']) {
        writer.append([
          { name: 'MESSAGE', value: 'irrelevant' },
          { name: 'PRIORITY', value: priority },
        ]);
      }
      writer.close();

      const reader = FileReader.open(journalPath);
      reader.entryOffsets = [];
      const fields = reader.enumerateFields();
      const values = reader.queryUnique('PRIORITY');
      reader.close();
      assert.ok(fields.has('MESSAGE'));
      assert.ok(fields.has('PRIORITY'));
      assert.deepEqual(
        new Set(values.map(v => v.toString())),
        new Set(['0', '3', '6', '7']),
      );
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const firstPath = join(tempDir, 'unique-first.journal');
      const secondPath = join(tempDir, 'unique-second.journal');
      const first = Writer.create(firstPath);
      first.append([
        { name: 'MESSAGE', value: 'first' },
        { name: 'PRIORITY', value: '6' },
      ]);
      first.close();
      const second = Writer.create(secondPath);
      second.append([
        { name: 'MESSAGE', value: 'second' },
        { name: 'PRIORITY', value: '6' },
      ]);
      second.append([
        { name: 'MESSAGE', value: 'third' },
        { name: 'PRIORITY', value: '3' },
      ]);
      second.close();

      const reader = DirectoryReader.openFiles([firstPath, secondPath]);
      const values = reader.queryUnique('PRIORITY');
      reader.close();
      assert.deepEqual(
        new Set(values.map(v => v.toString())),
        new Set(['3', '6']),
      );
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'flags.journal');
      const writer = Writer.create(journalPath);
      writer.append([{ name: 'MESSAGE', value: 'flags' }]);
      writer.close();

      assert.throws(() => SdJournalOpen(journalPath, 1), /unsupported sd_journal_open flags/);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'custom-source',
        maxEntries: 2,
        maxBytes: 16 * 1024 * 1024,
        maxFiles: 10,
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      });
      for (let i = 0; i < 5; i++) {
        log.append([{ name: 'MESSAGE', value: `entry-${i}` }]);
        if (i === 0) {
          assert.match(basename(log.activeFile()), /^custom-source@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/);
          assert.equal(safeExistsSync(join(log.journalDirectory(), 'custom-source.journal')), false);
        }
      }
      log.close();
      assert.throws(() => log.append([{ name: 'MESSAGE', value: 'after-close' }]), /journal log is closed/);

      assert.equal(log.journalDirectory(), join(tempDir, '00112233445566778899aabbccddeeff'));
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal') || name.endsWith('.journal~')).sort();
      const archived = files.filter((name) => name.endsWith('.journal~'));
      assert.equal(archived.length, 0);
      assert.equal(files.length, 3);
      assert.ok(files.every((name) => /^custom-source@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/.test(name)));

      const seqnums = [];
      for (const name of files) {
        const reader = FileReader.open(join(log.journalDirectory(), name));
        assert.equal(reader.header.state, STATE_ARCHIVED);
        while (reader.step()) {
          seqnums.push(reader.getEntry().seqnum);
        }
        reader.close();
      }
      assert.deepEqual(seqnums, [1n, 2n, 3n, 4n, 5n]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const bootId = '0123456789abcdef0123456789abcdef';
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        bootId: Buffer.from(bootId, 'hex'),
      });
      log.append([
        { name: 'MESSAGE', value: 'journald policy preserves trusted fields' },
        { name: 'TEST_ID', value: 'journald-field-policy' },
        { name: '_HOSTNAME', value: 'synthetic-host' },
        { name: '_TRANSPORT', value: 'snmptrap' },
        { name: '_BOOT_ID', value: bootId },
      ], { realtimeUsec: 1_700_002_401_000_000n, monotonicUsec: 10n });
      log.sync();

      const reader = FileReader.open(log.activeFilePath());
      const entries = [];
      try {
        while (reader.step()) entries.push(reader.getEntry());
      } finally {
        reader.close();
      }
      assert.equal(entries.length, 1);
      assert.equal(entries[0].fields._HOSTNAME.toString('utf8'), 'synthetic-host');
      assert.equal(entries[0].fields._TRANSPORT.toString('utf8'), 'snmptrap');
      assert.equal(entries[0].fields._BOOT_ID.toString('utf8'), bootId);
      assert.equal(entries[0].fieldValues._BOOT_ID.length, 1);

      const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
      if (journalctl.status === 0) {
        const stockRows = run('journalctl', ['--directory', log.journalDirectory(), '--output=json', '--no-pager', 'TEST_ID=journald-field-policy'])
          .trim()
          .split('\n')
          .filter(Boolean)
          .map((line) => JSON.parse(line));
        assert.equal(stockRows.length, 1);
        assert.equal(stockRows[0]._HOSTNAME, 'synthetic-host');
        assert.equal(stockRows[0]._TRANSPORT, 'snmptrap');
        assert.equal(stockRows[0]._BOOT_ID, bootId);
      }
      log.close();
      for (const path of journalFiles(log.journalDirectory())) verifyJournalFileIfAvailable(path);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP,
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      });
      log.append([
        { name: 'MESSAGE', value: 'journal app keeps valid fields' },
        { name: 'TEST_ID', value: 'journal-app-field-policy' },
        { name: '_HOSTNAME', value: 'dropped-host' },
        { name: 'foo.bar', value: 'dropped-dot' },
      ], {
        realtimeUsec: 1_700_002_402_000_000n,
        monotonicUsec: 20n,
      });
      assert.throws(
        () => log.append([{ name: '_HOSTNAME', value: 'drop-only' }], {
          realtimeUsec: 1_700_002_402_000_001n,
          monotonicUsec: 21n,
        }),
        /empty entry/,
      );
      log.close();

      const path = journalFiles(log.journalDirectory())[0];
      const reader = FileReader.open(path);
      const entries = [];
      try {
        while (reader.step()) entries.push(reader.getEntry());
      } finally {
        reader.close();
      }
      assert.equal(entries.length, 1);
      assert.equal(entries[0].fields.MESSAGE.toString('utf8'), 'journal app keeps valid fields');
      assert.match(entries[0].fields._BOOT_ID.toString('utf8'), /^[0-9a-f]{32}$/);
      assert.equal(entries[0].fields._HOSTNAME, undefined);
      assert.equal(entries[0].fields['foo.bar'], undefined);
      verifyJournalFileIfAvailable(path);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const bootId = '0123456789abcdef0123456789abcdef';
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        bootId: Buffer.from(bootId, 'hex'),
        fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP,
      });
      log.appendRaw([
        Buffer.from('MESSAGE=raw directory'),
        Buffer.from('TEST_ID=node-log-append-raw'),
        Buffer.from('_HOSTNAME=drop-host'),
        Buffer.from('lowercase=drop-lowercase'),
      ], {
        realtimeUsec: 1_700_002_404_000_000n,
        monotonicUsec: 40n,
        sourceRealtimeUsec: 1234n,
      });
      assert.throws(
        () => log.appendRaw([Buffer.from('_HOSTNAME=drop-only')], {
          realtimeUsec: 1_700_002_404_000_001n,
          monotonicUsec: 41n,
        }),
        /empty entry/,
      );
      const journalDir = log.journalDirectory();
      log.close();

      const path = journalFiles(journalDir)[0];
      const reader = FileReader.open(path);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw directory');
      assert.equal(entry.fields.TEST_ID.toString('utf8'), 'node-log-append-raw');
      assert.equal(entry.fields._BOOT_ID.toString('utf8'), bootId);
      assert.equal(entry.fields._SOURCE_REALTIME_TIMESTAMP.toString('utf8'), '1234');
      assert.equal(entry.fields._HOSTNAME, undefined);
      assert.equal(entry.fields.lowercase, undefined);

      const rows = journalctlDirectoryRowsIfAvailable(
        journalDir,
        `_BOOT_ID=${bootId}`,
        'TEST_ID=node-log-append-raw',
      );
      if (rows !== null) {
        assert.equal(rows.length, 1);
        assert.equal(rows[0].MESSAGE, 'raw directory');
      }
      verifyJournalFileIfAvailable(path);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const longName = 'a'.repeat(1024);
      const log = new Log(tempDir, {
        source: 'system',
        fieldNamePolicy: FIELD_NAME_POLICY_RAW,
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      });
      log.append([
        { name: 'lowercase', value: 'ok' },
        { name: 'foo.bar', value: 'dot' },
        { name: 'field name', value: 'space' },
        { name: longName, value: 'long' },
        { name: '__proto__', value: 'proto-safe' },
        { name: 'BINARY', value: Buffer.from([0x61, 0x00, 0x3d, 0x62]) },
      ], {
        realtimeUsec: 1_700_002_403_000_000n,
        monotonicUsec: 30n,
      });
      assert.throws(
        () => log.append([{ name: 'BAD=NAME', value: 'bad' }], {
          realtimeUsec: 1_700_002_403_000_001n,
          monotonicUsec: 31n,
        }),
        /invalid field name/,
      );
      log.close();

      const path = journalFiles(log.journalDirectory())[0];
      const reader = FileReader.open(path);
      const entries = [];
      try {
        while (reader.step()) entries.push(reader.getEntry());
      } finally {
        reader.close();
      }
      assert.equal(entries.length, 1);
      assert.equal(entries[0].fields.lowercase.toString('utf8'), 'ok');
      assert.equal(entries[0].fields['foo.bar'].toString('utf8'), 'dot');
      assert.equal(entries[0].fields['field name'].toString('utf8'), 'space');
      assert.equal(Reflect.get(entries[0].fields, longName).toString('utf8'), 'long');
      assert.equal(Object.getPrototypeOf(entries[0].fields), null);
      assert.equal(Reflect.get(entries[0].fields, '__proto__').toString('utf8'), 'proto-safe');
      assert.equal(Object.prototype.protoSafe, undefined);
      assert.deepEqual(entries[0].fields.BINARY, Buffer.from([0x61, 0x00, 0x3d, 0x62]));
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
        maxDurationUsec: 10_000_000n,
        maxFiles: 10,
      });
      const base = 1_700_002_090_000_000n;
      for (const [i, realtime] of [base, base + 9_999_999n, base + 10_000_000n].entries()) {
        log.append(
          [{ name: 'MESSAGE', value: `duration-rotation-${i}` }],
          { realtimeUsec: realtime, monotonicUsec: BigInt(i + 1) },
        );
      }
      log.close();
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
      assert.equal(files.length, 2);
      const counts = files.map((name) => {
        const reader = FileReader.open(join(log.journalDirectory(), name));
        try {
          return reader.header.n_entries;
        } finally {
          reader.close();
        }
      });
      assert.deepEqual(counts, [2n, 1n]);
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
      log.append([{ name: 'MESSAGE', value: 'default system naming' }]);
      assert.equal(log.nextSeqnum, 2n);
      assert.match(basename(log.activeFile()), /^system@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/);
      assert.equal(safeExistsSync(join(log.journalDirectory(), 'system.journal')), false);
      log.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const baseOptions = {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 1,
        maxFiles: 0,
      };
      const first = new Log(tempDir, baseOptions);
      for (let i = 0; i < 3; i++) {
        first.append(
          [{ name: 'MESSAGE', value: `age-retention-${i}` }],
          { realtimeUsec: BigInt(1_000_000 + i), monotonicUsec: BigInt(i + 1) },
        );
      }
      first.close();
      const journalDir = first.journalDirectory();
      assert.equal(safeReaddirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 3);

      const retained = new Log(tempDir, { ...baseOptions, maxEntries: 0, maxRetentionAgeUsec: 1_000_000n });
      assert.equal(safeReaddirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 3);
      retained.enforceRetention();
      assert.equal(safeReaddirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 0);
      retained.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const baseOptions = {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 1,
        maxFiles: 0,
      };
      const first = new Log(tempDir, baseOptions);
      for (let i = 0; i < 2; i++) {
        first.append(
          [{ name: 'MESSAGE', value: `age-active-retention-${i}` }],
          { realtimeUsec: BigInt(1_000_000 + i), monotonicUsec: BigInt(i + 1) },
        );
      }
      first.close();
      const journalDir = first.journalDirectory();
      assert.equal(safeReaddirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 2);

      const retained = new Log(tempDir, { ...baseOptions, maxEntries: 0, maxRetentionAgeUsec: 1_000_000n });
      retained.append(
        [{ name: 'MESSAGE', value: 'age-protected-active' }],
        { realtimeUsec: 1_000_100n, monotonicUsec: 10n },
      );
      const activePath = retained.activeFile();
      retained.enforceRetention();
      const files = safeReaddirSync(journalDir).filter((name) => name.endsWith('.journal')).sort();
      assert.deepEqual(files.map((name) => join(journalDir, name)), [activePath]);
      const reader = FileReader.open(activePath);
      assert.equal(reader.step(), true);
      assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'age-protected-active');
      reader.close();
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
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        maxEntries: 0,
        maxBytes: 0,
        maxFiles: 10,
      });
      assert.throws(() => log.append([]), /empty entry/);
      const files = safeReaddirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal'));
      assert.equal(files.length, 0);
      log.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }
}
