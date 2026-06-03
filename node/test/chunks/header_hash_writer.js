import * as support from '../support.js';

export async function run() {
  const { mkdtempSync, rmSync, tmpdir, join, assert, jenkinsHash64, sipHash24, FIELD_NAME_POLICY_RAW, Writer, Log, FileReader, DirectoryReader, parseDataObject, parseEntryObject, exportEntry, jsonEntry, SdJournalOpen, SdJournalOpenFiles, SdJournalNext, SdJournalPrevious, SdJournalSeekRealtimeUsec, SdJournalSeekCursor, SdJournalGetEntry, SdJournalGetCursor, SdJournalTestCursor, SdJournalGetSeqnum, SdJournalGetMonotonicUsec, SdJournalRestartData, SdJournalEnumerateAvailableData, SdJournalGetData, SdJournalQueryUniqueState, SdJournalEnumerateAvailableUnique, SdJournalRestartFields, SdJournalEnumerateField, DATA_OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, INCOMPATIBLE_COMPRESSED_LZ4, INCOMPATIBLE_KEYED_HASH, OBJECT_TYPE_DATA, OBJECT_TYPE_ENTRY, DEFAULT_FIELD_HASH_BUCKETS, dataHashBucketsForMaxFileSize, parseFileHeader, writeObjectHeader, UNKNOWN_PROCESS_START_TIME, lockOwnerIsActive, parseLinuxProcStatStartTime, readHostBootId, readHostBootIdText, safeReadFileSync, safeWriteFileSync, journalFiles, collectNullable, makeHistoricalHeaderFixture } = support;
  const historicalHeaderCases = [
    { headerSize: 208 },
    { headerSize: 216, n_data: 11n },
    { headerSize: 220, n_data: 11n },
    { headerSize: 224, n_data: 11n, n_fields: 22n },
    { headerSize: 232, n_data: 11n, n_fields: 22n, n_tags: 33n },
    { headerSize: 240, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n },
    { headerSize: 248, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n },
    { headerSize: 250, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n },
    { headerSize: 256, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n },
    { headerSize: 260, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77 },
    { headerSize: 264, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88 },
    { headerSize: 268, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88 },
    { headerSize: 272, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88, tail_entry_offset: 99n },
    { headerSize: 300, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88, tail_entry_offset: 99n },
  ];

  for (const expected of historicalHeaderCases) {
    const header = parseFileHeader(makeHistoricalHeaderFixture(expected.headerSize));
    assert.equal(header.n_data, expected.n_data ?? 0n, `n_data header_size=${expected.headerSize}`);
    assert.equal(header.n_fields, expected.n_fields ?? 0n, `n_fields header_size=${expected.headerSize}`);
    assert.equal(header.n_tags, expected.n_tags ?? 0n, `n_tags header_size=${expected.headerSize}`);
    assert.equal(header.n_entry_arrays, expected.n_entry_arrays ?? 0n, `n_entry_arrays header_size=${expected.headerSize}`);
    assert.equal(header.data_hash_chain_depth, expected.data_hash_chain_depth ?? 0n, `data_hash_chain_depth header_size=${expected.headerSize}`);
    assert.equal(header.field_hash_chain_depth, expected.field_hash_chain_depth ?? 0n, `field_hash_chain_depth header_size=${expected.headerSize}`);
    assert.equal(header.tail_entry_array_offset, expected.tail_entry_array_offset ?? 0, `tail_entry_array_offset header_size=${expected.headerSize}`);
    assert.equal(header.tail_entry_array_n_entries, expected.tail_entry_array_n_entries ?? 0, `tail_entry_array_n_entries header_size=${expected.headerSize}`);
    assert.equal(header.tail_entry_offset, expected.tail_entry_offset ?? 0n, `tail_entry_offset header_size=${expected.headerSize}`);
  }

  assert.throws(
    () => parseFileHeader(makeHistoricalHeaderFixture(300).subarray(0, 208)),
    /header buffer too small/,
    'future header with truncated known prefix should be rejected',
  );

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-historical-unkeyed-'));
    const journalPath = join(tempDir, 'unkeyed-lz4.journal');
    try {
      safeWriteFileSync(journalPath, makeHistoricalHeaderFixture(240, INCOMPATIBLE_COMPRESSED_LZ4));
      const reader = FileReader.open(journalPath);
      assert.equal(reader.header.incompatible_flags & INCOMPATIBLE_KEYED_HASH, 0);
      assert.ok(reader.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4);
      reader.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  const jenkinsVectors = [
    ['', 0xdeadbeefdeadbeefn],
    ['SYSLOG_IDENTIFIER=netdata', 0x45ccd0e9ed13614an],
    ['_SYSTEMD_UNIT=netdata.service', 0x1013c5df11a983f0n],
    ['PRIORITY=6', 0x80f09f19808d26a3n],
    ['MESSAGE=Test message', 0x8ed53fb52aa5c55dn],
  ];

  for (const [data, expected] of jenkinsVectors) {
    assert.equal(jenkinsHash64(Buffer.from(data)), expected, `jenkinsHash64(${JSON.stringify(data)})`);
  }

  const sipKey = Buffer.from(Array.from({ length: 16 }, (_, i) => i));
  const sipMessage = Buffer.from(Array.from({ length: 64 }, (_, i) => i));
  const sipVectors = [
    [0, 0x726fdb47dd0e0e31n],
    [1, 0x74f839c593dc67fdn],
    [2, 0x0d6c8009d9a94f5an],
    [3, 0x85676696d7fb7e2dn],
    [4, 0xcf2794e0277187b7n],
    [5, 0x18765564cd99a68dn],
    [6, 0xcbc9466e58fee3cen],
    [7, 0xab0200f58b01d137n],
  ];

  for (const [length, expected] of sipVectors) {
    assert.equal(sipHash24(sipKey, sipMessage.subarray(0, length)), expected, `sipHash24(length=${length})`);
  }

  {
    const key = Buffer.from('de5f2812d87b89e81af97cfe8e1423e9', 'hex');
    const payload = Buffer.concat([
      Buffer.from('COMPRESSED_PAYLOAD='),
      Buffer.from(Array.from({ length: 256 }, (_, i) => (i % 26) + 0x41)),
    ]);
    assert.equal(sipHash24(key, payload), 0xf9a795df589b5204n);
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-live-publish-'));
    try {
      const writeFile = (name, every) => {
        const journalPath = join(tempDir, `${name}.journal`);
        const writer = Writer.create(journalPath, {
          fileId: Buffer.from('40000000000000000000000000000000', 'hex'),
          machineId: Buffer.from('10000000000000000000000000000000', 'hex'),
          bootId: Buffer.from('20000000000000000000000000000000', 'hex'),
          seqnumId: Buffer.from('30000000000000000000000000000000', 'hex'),
          dataHashTableBuckets: 64,
          fieldHashTableBuckets: 16,
          livePublishEveryEntries: every,
        });
        for (let i = 0; i < 5; i++) {
          writer.append([
            { name: 'MESSAGE', value: `row-${String(i).padStart(2, '0')}` },
            { name: 'SYSLOG_IDENTIFIER', value: 'node-live-publish-test' },
          ], { realtimeUsec: 1_700_000_100_000_000n + BigInt(i), monotonicUsec: BigInt(i + 1) });
        }
        const pending = writer.entriesSinceLivePublication;
        writer.close();
        return { bytes: safeReadFileSync(journalPath), pending };
      };

      const immediate = writeFile('immediate', 1);
      const disabled = writeFile('disabled', 0);
      const everyThree = writeFile('every-three', 3);
      assert.equal(immediate.pending, 0);
      assert.equal(disabled.pending, 0);
      assert.equal(everyThree.pending, 2);
      assert.deepEqual(disabled.bytes, immediate.bytes);
      assert.deepEqual(everyThree.bytes, immediate.bytes);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const statFields = Array.from({ length: 20 }, (_, i) => String(i + 1));
    statFields[19] = '424242';
    assert.equal(parseLinuxProcStatStartTime(`123 (node worker) ${statFields.join(' ')}`), '424242');

    assert.equal(lockOwnerIsActive(
      { pid: 123, bootId: 'boot-a', startTime: '1' },
      { bootId: 'boot-b', processStartTime: () => '1', processAlive: () => true },
    ), false);
    assert.equal(lockOwnerIsActive(
      { pid: 123, bootId: '', startTime: '1' },
      { bootId: '', processStartTime: () => '2', processAlive: () => true },
    ), false);
    assert.equal(lockOwnerIsActive(
      { pid: 123, bootId: '', startTime: UNKNOWN_PROCESS_START_TIME },
      { bootId: '', processStartTime: () => null, processAlive: () => false },
    ), false);
    assert.equal(lockOwnerIsActive(
      { pid: 123, bootId: '', startTime: UNKNOWN_PROCESS_START_TIME },
      { bootId: '', processStartTime: () => null, processAlive: () => true },
    ), true);
    assert.equal(lockOwnerIsActive(
      { pid: 123, bootId: '', startTime: UNKNOWN_PROCESS_START_TIME },
      { bootId: '', processStartTime: () => null, processAlive: () => null },
    ), true);

    const hostBootId = readHostBootId();
    if (hostBootId !== null) assert.equal(hostBootId.length, 16);
    const hostBootIdText = readHostBootIdText();
    if (hostBootIdText) {
      assert.match(hostBootIdText, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'jf-facade.journal');
      const writer = Writer.create(journalPath);
      writer.append([
        { name: 'MESSAGE', value: 'first' },
        { name: 'REPEAT', value: 'one' },
        { name: 'REPEAT', value: 'two' },
        { name: 'BIN', value: Buffer.from([0x00, 0xff]) },
      ], { realtimeUsec: 1000n, monotonicUsec: 11n });
      writer.append([
        { name: 'MESSAGE', value: 'second' },
        { name: 'REPEAT', value: 'three' },
      ], { realtimeUsec: 1001n, monotonicUsec: 12n });
      writer.close();

      const journal = SdJournalOpenFiles([journalPath], 0);
      assert.equal(SdJournalNext(journal), 1);
      const seqnum = SdJournalGetSeqnum(journal);
      assert.equal(seqnum.seqnum, 1n);
      assert.ok(seqnum.seqnum_id);
      const monotonic = SdJournalGetMonotonicUsec(journal);
      assert.equal(monotonic.monotonic, 11n);
      assert.ok(Buffer.isBuffer(monotonic.boot_id));

      SdJournalRestartData(journal);
      const payloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
      assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=one'))));
      assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=two'))));
      assert.ok(payloads.some(p => p.equals(Buffer.from([0x42, 0x49, 0x4e, 0x3d, 0x00, 0xff]))));
      assert.deepEqual(SdJournalGetData(journal, 'REPEAT'), Buffer.from('REPEAT=one'));

      SdJournalQueryUniqueState(journal, 'REPEAT');
      const unique = collectNullable(() => SdJournalEnumerateAvailableUnique(journal));
      assert.ok(unique.some(p => p.equals(Buffer.from('REPEAT=one'))));
      assert.ok(unique.some(p => p.equals(Buffer.from('REPEAT=two'))));
      assert.ok(unique.some(p => p.equals(Buffer.from('REPEAT=three'))));

      SdJournalRestartFields(journal);
      const fields = new Set(collectNullable(() => SdJournalEnumerateField(journal)));
      assert.ok(fields.has('MESSAGE'));
      assert.ok(fields.has('REPEAT'));
      assert.ok(fields.has('BIN'));

      SdJournalSeekRealtimeUsec(journal, 1001n);
      assert.equal(SdJournalNext(journal), 1);
      assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'second');
      SdJournalSeekRealtimeUsec(journal, 1001n);
      assert.equal(SdJournalPrevious(journal), 1);
      assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'second');
      const cursor = SdJournalGetCursor(journal);
      assert.equal(SdJournalTestCursor(journal, cursor), true);
      assert.equal(SdJournalTestCursor(journal, 'invalid-cursor'), false);
      SdJournalSeekRealtimeUsec(journal, 1000n);
      assert.equal(SdJournalNext(journal), 1);
      assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'first');
      SdJournalSeekCursor(journal, cursor);
      assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'second');
      const missingCursor = cursor.replace(/n=\d+$/, 'n=999999');
      assert.doesNotThrow(() => SdJournalSeekCursor(journal, missingCursor));
      journal.close();

      const journalPath2 = join(tempDir, 'jf-facade-second.journal');
      const writer2 = Writer.create(journalPath2);
      writer2.append([
        { name: 'MESSAGE', value: 'third' },
        { name: 'REPEAT', value: 'four' },
      ], { realtimeUsec: 1002n, monotonicUsec: 21n });
      writer2.close();

      const multi = SdJournalOpenFiles([journalPath2, journalPath], 0);
      const messages = [];
      while (SdJournalNext(multi) === 1) {
        messages.push(SdJournalGetEntry(multi).fields.MESSAGE.toString('utf8'));
      }
      assert.deepEqual(messages, ['first', 'second', 'third']);
      SdJournalSeekRealtimeUsec(multi, 1002n);
      assert.equal(SdJournalPrevious(multi), 1);
      assert.equal(SdJournalGetEntry(multi).fields.MESSAGE.toString('utf8'), 'third');
      SdJournalSeekRealtimeUsec(multi, 999n);
      assert.equal(SdJournalPrevious(multi), 0);
      multi.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'facade-row-lifetime.journal');
      const writer = Writer.create(journalPath);
      writer.append([
        { name: 'MESSAGE', value: 'first' },
        { name: 'REPEAT', value: 'one' },
        { name: 'REPEAT', value: 'two' },
      ], { realtimeUsec: 1000n, monotonicUsec: 11n });
      writer.close();

      const journal = SdJournalOpenFiles([journalPath], 0);
      assert.equal(SdJournalNext(journal), 1);
      SdJournalRestartData(journal);
      const payloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
      assert.ok(payloads.some(p => p.equals(Buffer.from('MESSAGE=first'))));
      assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=one'))));
      assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=two'))));
      journal.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'facade-compressed-row-lifetime.journal');
      const writer = Writer.create(journalPath, { compression: 'zstd', compressionThresholdBytes: 8 });
      const largeValue = Buffer.from('mixed '.repeat(256));
      writer.append([
        { name: 'SMALL', value: 'x' },
        { name: 'LARGE', value: largeValue },
      ], { realtimeUsec: 1000n, monotonicUsec: 11n });
      writer.close();

      const journal = SdJournalOpenFiles([journalPath], 0);
      assert.equal(SdJournalNext(journal), 1);
      SdJournalRestartData(journal);
      const payloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
      assert.ok(payloads.some(p => p.equals(Buffer.from('SMALL=x'))));
      assert.ok(payloads.some(p => p.equals(Buffer.concat([Buffer.from('LARGE='), largeValue]))));
      journal.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'raw-byte-names.journal');
      const invalidUtf8Name = Buffer.from([0xff, 0x52, 0x41, 0x57]);
      const nulName = Buffer.from('RAW\0NAME', 'latin1');
      const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_RAW });
      writer.append([
        { name: 'MESSAGE', value: 'raw byte names' },
        { name: invalidUtf8Name, value: Buffer.from('invalid utf8') },
        { name: nulName, value: Buffer.from('nul name') },
        { name: 'field name', value: Buffer.from('space') },
        { name: 'BINARY', value: Buffer.from('a\0=b', 'latin1') },
      ], { realtimeUsec: 1_700_004_000_000_000n, monotonicUsec: 1n });
      writer.close();

      const reader = FileReader.open(journalPath);
      assert.equal(reader.step(), true);
      const entry = reader.getEntry();
      assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw byte names');
      assert.equal(entry.rawFieldValues.get(invalidUtf8Name.toString('hex'))[0].toString('utf8'), 'invalid utf8');
      assert.equal(entry.rawFieldValues.get(nulName.toString('hex'))[0].toString('utf8'), 'nul name');
      assert.equal(entry.rawFieldValues.get(Buffer.from('field name').toString('hex'))[0].toString('utf8'), 'space');
      assert.equal(reader.getRaw(invalidUtf8Name).toString('utf8'), 'invalid utf8');
      assert.equal(reader.getRaw(nulName).toString('utf8'), 'nul name');
      assert.deepEqual(reader.getRawValues(Buffer.from('field name')).map(v => v.toString('utf8')), ['space']);
      assert.ok(entry.rawFields.some(([name, value]) => name.equals(invalidUtf8Name) && value.equals(Buffer.from('invalid utf8'))));
      assert.ok(entry.payloads.some(p => p.equals(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]))));
      assert.equal(Object.prototype.hasOwnProperty.call(entry.fields, invalidUtf8Name.toString('utf8')), false);

      const payloads = [];
      reader.visitEntryPayloads((payload) => payloads.push(Buffer.from(payload)));
      assert.ok(payloads.some(p => p.equals(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]))));
      reader.close();

      const exported = exportEntry(entry);
      assert.ok(exported.includes(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8\n')])));
      const encoded = jsonEntry(entry);
      assert.equal(Object.prototype.hasOwnProperty.call(encoded, invalidUtf8Name.toString('utf8')), false);

      const dirReader = DirectoryReader.openFiles([journalPath]);
      assert.equal(dirReader.step(), true);
      assert.equal(dirReader.getRaw(invalidUtf8Name).toString('utf8'), 'invalid utf8');
      dirReader.close();

      const journal = SdJournalOpen(journalPath, 0);
      assert.equal(SdJournalNext(journal), 1);
      assert.deepEqual(SdJournalGetData(journal, Buffer.from('BINARY')), Buffer.from('BINARY=a\0=b', 'latin1'));
      const originalGetEntryPayload = journal.reader.getEntryPayload;
      journal.reader.getEntryPayload = undefined;
      assert.deepEqual(SdJournalGetData(journal, invalidUtf8Name), Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]));
      journal.reader.getEntryPayload = originalGetEntryPayload;
      SdJournalRestartData(journal);
      const facadePayloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
      journal.close();
      assert.ok(facadePayloads.some(p => p.equals(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]))));
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'active-reader-refresh.journal');
      const writer = Writer.create(journalPath, { livePublishEveryEntries: 1 });
      writer.append([{ name: 'MESSAGE', value: 'first' }], { realtimeUsec: 1_700_004_010_000_000n, monotonicUsec: 1n });
      const reader = FileReader.open(journalPath);
      reader.seekHead();
      assert.equal(reader.step(), true);
      assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'first');
      assert.equal(reader.step(), false);
      writer.append([{ name: 'MESSAGE', value: 'second' }], { realtimeUsec: 1_700_004_010_000_001n, monotonicUsec: 2n });
      assert.equal(reader.step(), true);
      assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'second');
      reader.close();
      writer.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const dataBuf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + 4);
    writeObjectHeader(dataBuf, 0, OBJECT_TYPE_DATA, 0, BigInt(DATA_OBJECT_HEADER_SIZE + 16));
    assert.throws(() => parseDataObject(dataBuf, 0), /data object exceeds buffer/);

    const entryBuf = Buffer.alloc(ENTRY_OBJECT_HEADER_SIZE);
    writeObjectHeader(entryBuf, 0, OBJECT_TYPE_ENTRY, 0, BigInt(ENTRY_OBJECT_HEADER_SIZE + 16));
    assert.throws(() => parseEntryObject(entryBuf, 0), /entry object exceeds buffer/);
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        retentionPolicy: { maxAgeUsec: 20_000_001n },
      });
      const base = BigInt(Date.now()) * 1000n;
      for (const [i, realtime] of [base, base + 1_000_000n, base + 1_000_001n].entries()) {
        log.append([
          { name: 'MESSAGE', value: `derived-duration-rotation-${i}` },
          { name: 'TEST_ID', value: 'derived-duration-rotation' },
        ], {
          realtimeUsec: realtime,
          monotonicUsec: BigInt(i + 1),
        });
      }
      log.close();
      const files = journalFiles(log.journalDirectory());
      assert.equal(files.length, 2);
      const counts = files.map((path) => {
        const reader = FileReader.open(path);
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
        retentionPolicy: { maxBytes: 1_000_000 },
      });
      assert.equal(log.maxBytes, 512 * 1024);
      log.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const maxSize = 128 * 1024 * 1024;
      const log = new Log(tempDir, {
        source: 'system',
        machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
        retentionPolicy: { maxBytes: maxSize * 20, maxAgeUsec: 20_000_001n },
      });
      assert.equal(log.maxBytes, maxSize);
      assert.equal(log.maxDurationUsec, 1_000_001n);
      log.append([{ name: 'MESSAGE', value: 'derived rotation defaults' }], {
        realtimeUsec: 1_700_002_091_000_000n,
        monotonicUsec: 1n,
      });
      log.close();
      const files = journalFiles(log.journalDirectory());
      assert.equal(files.length, 1);
      const reader = FileReader.open(files[0]);
      try {
        assert.equal(Number(reader.header.data_hash_table_size / 16n), dataHashBucketsForMaxFileSize(maxSize));
        assert.equal(Number(reader.header.field_hash_table_size / 16n), DEFAULT_FIELD_HASH_BUCKETS);
      } finally {
        reader.close();
      }
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }
}
