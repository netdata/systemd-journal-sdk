import * as support from '../support.js';

export async function run() {
  const { closeSync, mkdtempSync, rmSync, writeSync, tmpdir, join, spawnSync, assert, uuidToString, FIELD_NAME_POLICY_JOURNAL_APP, FIELD_NAME_POLICY_RAW, Writer, Log, FileReader, INCOMPATIBLE_COMPACT, INCOMPATIBLE_KEYED_HASH, JOURNAL_COMPACT_SIZE_MAX, dataHashBucketsForMaxFileSize, verifyFile, safeOpenSync, safeReadFileSync, run, journalFiles, verifyJournalFileIfAvailable, verifyJournalFileFailsIfAvailable } = support;

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const maxSize = 16 * 1024 * 1024;
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        retentionPolicy: { maxBytes: maxSize * 20 },
      });
      assert.equal(log.maxBytes, maxSize);
      for (let i = 0; i < 12; i++) {
        log.append([
          { name: 'MESSAGE', value: `derived-size-rotation-${i}` },
          { name: 'PAYLOAD', value: `${String(i).padStart(5, '0')}-${'x'.repeat(2 * 1024 * 1024)}` },
          { name: 'TEST_ID', value: 'derived-size-rotation' },
        ], {
          realtimeUsec: 1_700_002_092_000_000n + BigInt(i),
          monotonicUsec: BigInt(i + 1),
        });
      }
      log.close();
      const files = journalFiles(log.journalDirectory());
      assert.ok(files.length >= 2);
      let entries = 0n;
      for (const path of files) {
        const reader = FileReader.open(path);
        try {
          assert.equal(Number(reader.header.data_hash_table_size / 16n), dataHashBucketsForMaxFileSize(maxSize));
          entries += reader.header.n_entries;
        } finally {
          reader.close();
        }
      }
      assert.equal(entries, 12n);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        compact: true,
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        retentionPolicy: { maxBytes: (Number(JOURNAL_COMPACT_SIZE_MAX) + 4096) * 20 },
      });
      assert.equal(log.maxBytes, Number(JOURNAL_COMPACT_SIZE_MAX));
      log.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const explicitSize = 64 * 1024 * 1024;
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        rotationPolicy: { maxBytes: explicitSize, maxDurationUsec: 2_000_000n },
        retentionPolicy: { maxBytes: 128 * 1024 * 1024 * 20, maxAgeUsec: 20_000_000n },
      });
      assert.equal(log.maxBytes, explicitSize);
      assert.equal(log.maxDurationUsec, 2_000_000n);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const machineId = Buffer.from('00112233445566778899aabbccddeeff', 'hex');
      const bootA = Buffer.from('aa000000000000000000000000000001', 'hex');
      const bootB = Buffer.from('bb000000000000000000000000000002', 'hex');
      const first = new Log(tempDir, {
        source: 'system',
        identityMode: 'strict',
        machineId,
        bootId: bootA,
      });
      first.append(
        [{ name: 'MESSAGE', value: 'cross boot first' }, { name: 'TEST_ID', value: 'cross-boot-monotonic' }],
        { realtimeUsec: 1_700_003_100_000_000n, monotonicUsec: 100n },
      );
      first.close();

      const second = new Log(tempDir, {
        source: 'system',
        identityMode: 'strict',
        machineId,
        bootId: bootB,
      });
      second.append(
        [{ name: 'MESSAGE', value: 'cross boot second' }, { name: 'TEST_ID', value: 'cross-boot-monotonic' }],
        { realtimeUsec: 1_700_003_100_000_001n, monotonicUsec: 1n },
      );
      second.close();

      const entries = [];
      for (const path of journalFiles(join(tempDir, uuidToString(machineId)))) {
        verifyJournalFileIfAvailable(path);
        const reader = FileReader.open(path);
        while (reader.step()) {
          const entry = reader.getEntry();
          if (entry.fields.TEST_ID?.toString('utf8') === 'cross-boot-monotonic') entries.push(entry);
        }
        reader.close();
      }
      entries.sort((a, b) => (a.realtime < b.realtime ? -1 : a.realtime > b.realtime ? 1 : 0));
      assert.equal(entries.length, 2);
      assert.deepEqual(entries.map((entry) => entry.monotonic), [100n, 1n]);
      assert.deepEqual(
        entries.map((entry) => uuidToString(entry.boot_id)),
        [uuidToString(bootA), uuidToString(bootB)],
      );
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'raw-zero-monotonic.journal');
      const writer = Writer.create(journalPath);
      writer.append([{ name: 'MESSAGE', value: 'raw zero monotonic' }], {
        realtimeUsec: 1_700_003_000_100_000n,
        monotonicUsec: 0n,
      });
      writer.close();
      verifyJournalFileIfAvailable(journalPath);

      const reader = FileReader.open(journalPath);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.monotonic, 0n);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    let writer;
    try {
      const journalPath = join(tempDir, 'journald-field-policy.journal');
      writer = Writer.create(journalPath);
      writer.append([
        { name: 'MESSAGE', value: 'trusted fields' },
        { name: '_HOSTNAME', value: 'synthetic-host' },
        { name: '_TRANSPORT', value: 'journal' },
      ], { realtimeUsec: 1_700_002_111_000_000n, monotonicUsec: 1n });
      for (const invalidName of ['lowercase', 'foo.bar', 'A'.repeat(65), '1FIELD']) {
        assert.throws(
          () => writer.append([{ name: invalidName, value: 'invalid' }], {
            realtimeUsec: 1_700_002_111_000_001n,
            monotonicUsec: 2n,
          }),
          /invalid field name/,
        );
      }
      writer.close();
      writer = undefined;
      verifyJournalFileIfAvailable(journalPath);

      const reader = FileReader.open(journalPath);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.fields._HOSTNAME.toString('utf8'), 'synthetic-host');
      assert.equal(entry.fields._TRANSPORT.toString('utf8'), 'journal');
    } finally {
      try {
        if (writer) writer.close();
      } catch {
        // Ignore cleanup errors from already closed writers in failing tests.
      }
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'raw-journald-payloads.journal');
      const writer = Writer.create(journalPath);
      writer.appendRaw([
        Buffer.from('MESSAGE=raw full payload'),
        Buffer.from('_HOSTNAME=synthetic-host'),
        Buffer.from('BINARY=a\x00=b=c', 'utf8'),
      ], { realtimeUsec: 1_700_002_111_100_000n, monotonicUsec: 2n });
      assert.throws(
        () => writer.appendRaw([], {
          realtimeUsec: 1_700_002_111_100_001n,
          monotonicUsec: 3n,
        }),
        /empty entry/,
      );
      assert.throws(
        () => writer.appendRaw([Buffer.from('=value')], {
          realtimeUsec: 1_700_002_111_100_002n,
          monotonicUsec: 4n,
        }),
        /invalid raw field payload/,
      );
      writer.close();
      verifyJournalFileIfAvailable(journalPath);

      const reader = FileReader.open(journalPath);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw full payload');
      assert.equal(entry.fields._HOSTNAME.toString('utf8'), 'synthetic-host');
      assert.deepEqual(entry.fields.BINARY, Buffer.from('a\x00=b=c', 'utf8'));
      assert.equal(entry.fields._BOOT_ID, undefined);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const write = (name, raw) => {
        const journalPath = join(tempDir, `${name}.journal`);
        const writer = Writer.create(journalPath, {
          fileId: Buffer.from('40000000000000000000000000000000', 'hex'),
          machineId: Buffer.from('10000000000000000000000000000000', 'hex'),
          bootId: Buffer.from('20000000000000000000000000000000', 'hex'),
          seqnumId: Buffer.from('30000000000000000000000000000000', 'hex'),
          dataHashTableBuckets: 64,
          fieldHashTableBuckets: 16,
        });
        const opts = { realtimeUsec: 1_700_002_111_200_000n, monotonicUsec: 3n };
        if (raw) {
          writer.appendRaw([
            Buffer.from('MESSAGE=equivalent entry'),
            Buffer.from('PRIORITY=6'),
            Buffer.from('BINARY=a\x00=b=c', 'utf8'),
          ], opts);
        } else {
          writer.append([
            { name: 'MESSAGE', value: 'equivalent entry' },
            { name: 'PRIORITY', value: '6' },
            { name: 'BINARY', value: Buffer.from('a\x00=b=c', 'utf8') },
          ], opts);
        }
        writer.close();
        return safeReadFileSync(journalPath);
      };

      assert.deepEqual(write('structured', false), write('raw', true));
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
      log.appendRaw([
        Buffer.from('MESSAGE=raw journald directory'),
        Buffer.from('TEST_ID=node-log-append-raw-journald'),
        Buffer.from('_HOSTNAME=synthetic-host'),
        Buffer.from(`_BOOT_ID=${bootId}`),
      ], {
        realtimeUsec: 1_700_002_403_500_000n,
        monotonicUsec: 35n,
        sourceRealtimeUsec: 999n,
      });
      const journalDir = log.journalDirectory();
      log.close();

      const path = journalFiles(journalDir)[0];
      const reader = FileReader.open(path);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw journald directory');
      assert.equal(entry.fields.TEST_ID.toString('utf8'), 'node-log-append-raw-journald');
      assert.equal(entry.fields._HOSTNAME.toString('utf8'), 'synthetic-host');
      assert.equal(entry.fields._BOOT_ID.toString('utf8'), bootId);
      assert.equal(entry.fieldValues._BOOT_ID.length, 1);
      assert.equal(entry.fields._SOURCE_REALTIME_TIMESTAMP.toString('utf8'), '999');
      verifyJournalFileIfAvailable(path);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      assert.throws(
        () => Writer.create(join(tempDir, 'alias-systemd.journal'), { fieldNamePolicy: 'systemd' }),
        /unsupported field name policy/,
      );
      assert.throws(
        () => Writer.create(join(tempDir, 'alias-app.journal'), { fieldNamePolicy: 'app' }),
        /unsupported field name policy/,
      );
      assert.throws(
        () => Writer.create(join(tempDir, 'alias-journal-app.journal'), { fieldNamePolicy: 'journal_app' }),
        /unsupported field name policy/,
      );
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'journal-app-field-policy.journal');
      const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP });
      writer.append([
        { name: 'MESSAGE', value: 'app valid' },
        { name: '_HOSTNAME', value: 'drop-host' },
        { name: 'lowercase', value: 'drop-lowercase' },
      ], { realtimeUsec: 1_700_002_112_000_000n, monotonicUsec: 1n });
      assert.throws(
        () => writer.append([{ name: '_HOSTNAME', value: 'drop-only' }], {
          realtimeUsec: 1_700_002_112_000_001n,
          monotonicUsec: 2n,
        }),
        /empty entry/,
      );
      writer.close();
      verifyJournalFileIfAvailable(journalPath);

      const reader = FileReader.open(journalPath);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.fields.MESSAGE.toString('utf8'), 'app valid');
      assert.equal(entry.fields._HOSTNAME, undefined);
      assert.equal(entry.fields.lowercase, undefined);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'raw-journal-app-field-policy.journal');
      const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP });
      writer.appendRaw([
        Buffer.from('MESSAGE=raw app valid'),
        Buffer.from('_HOSTNAME=drop-host'),
        Buffer.from('lowercase=drop-lowercase'),
      ], { realtimeUsec: 1_700_002_112_100_000n, monotonicUsec: 3n });
      assert.throws(
        () => writer.appendRaw([Buffer.from('_HOSTNAME=drop-only')], {
          realtimeUsec: 1_700_002_112_100_001n,
          monotonicUsec: 4n,
        }),
        /empty entry/,
      );
      assert.throws(
        () => writer.appendRaw([Buffer.from('NO_EQUALS')], {
          realtimeUsec: 1_700_002_112_100_002n,
          monotonicUsec: 5n,
        }),
        /invalid raw field payload/,
      );
      writer.close();
      verifyJournalFileIfAvailable(journalPath);

      const reader = FileReader.open(journalPath);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw app valid');
      assert.equal(entry.fields._HOSTNAME, undefined);
      assert.equal(entry.fields.lowercase, undefined);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const longName = 'a'.repeat(1024);
      const journalPath = join(tempDir, 'raw-field-policy.journal');
      const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_RAW });
      writer.append([
        { name: 'lowercase', value: 'ok' },
        { name: 'foo.bar', value: 'dot' },
        { name: 'field name', value: 'space' },
        { name: longName, value: 'long' },
        { name: '__proto__', value: 'proto-safe' },
        { name: 'BINARY', value: Buffer.from([0x61, 0x00, 0x3d, 0x62]) },
      ], { realtimeUsec: 1_700_002_113_000_000n, monotonicUsec: 1n });
      assert.throws(
        () => writer.append([{ name: 'BAD=NAME', value: 'bad' }], {
          realtimeUsec: 1_700_002_113_000_001n,
          monotonicUsec: 2n,
        }),
        /invalid field name/,
      );
      writer.close();

      const reader = FileReader.open(journalPath);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      reader.close();
      assert.equal(entry.fields.lowercase.toString('utf8'), 'ok');
      assert.equal(entry.fields['foo.bar'].toString('utf8'), 'dot');
      assert.equal(entry.fields['field name'].toString('utf8'), 'space');
      assert.equal(Reflect.get(entry.fields, longName).toString('utf8'), 'long');
      assert.equal(Object.getPrototypeOf(entry.fields), null);
      assert.equal(Reflect.get(entry.fields, '__proto__').toString('utf8'), 'proto-safe');
      assert.equal(Object.prototype.protoSafe, undefined);
      assert.deepEqual(entry.fields.BINARY, Buffer.from([0x61, 0x00, 0x3d, 0x62]));
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'raw-backward-monotonic.journal');
      const writer = Writer.create(journalPath);
      writer.append([{ name: 'MESSAGE', value: 'raw monotonic first' }], {
        realtimeUsec: 1_700_003_000_000_000n,
        monotonicUsec: 10n,
      });
      writer.append([{ name: 'MESSAGE', value: 'raw monotonic second' }], {
        realtimeUsec: 1_700_003_000_000_001n,
        monotonicUsec: 5n,
      });
      writer.close();

      assert.throws(() => verifyFile(journalPath), /monotonic/);
      verifyJournalFileFailsIfAvailable(journalPath, 'timestamp out of synchronization');
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'unsupported-flags.journal');
      const writer = Writer.create(journalPath);
      writer.close();

      const fd = safeOpenSync(journalPath, 'r+');
      const flags = Buffer.alloc(4);
      const unsupportedFlag = 1 << 30;
      flags.writeUInt32LE(INCOMPATIBLE_KEYED_HASH | unsupportedFlag, 0);
      writeSync(fd, flags, 0, flags.length, 12);
      closeSync(fd);

      assert.throws(() => Writer.open(journalPath), /unsupported journal: incompatible flags/);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    // Node compact writer -> Node reader and stock journalctl round-trip.
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'compact-writer.journal');
      const writer = Writer.create(journalPath, { compact: true });
      for (let i = 0; i < 3; i++) {
        writer.append([
          { name: 'MESSAGE', value: `compact-${i}` },
          { name: 'TEST_ID', value: 'node-compact' },
          { name: 'REUSED', value: 'same' },
        ], {
          realtimeUsec: 1_700_000_040_000_000n + BigInt(i),
          monotonicUsec: BigInt(i + 1),
        });
      }
      writer.close();

      const reader = FileReader.open(journalPath);
      assert.ok(reader.header.incompatible_flags & INCOMPATIBLE_COMPACT, 'compact flag must be set');
      const messages = [];
      while (reader.next()) messages.push(reader.getEntry().fields.MESSAGE.toString('utf8'));
      reader.close();
      assert.deepEqual(messages, ['compact-0', 'compact-1', 'compact-2']);

      const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
      if (journalctl.status === 0) {
        run('journalctl', ['--verify', '--file', journalPath]);
        const output = run('journalctl', ['--file', journalPath, '--output=json', '--no-pager', 'TEST_ID=node-compact']);
        assert.equal(output.trim().split('\n').filter(Boolean).length, 3);
      }
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }
}
