import * as support from '../support.js';

export async function run() {
  const { mkdtempSync, rmSync, createRequire, tmpdir, join, spawnSync, createHash, assert, uuidToString, DEFAULT_COMPRESS_THRESHOLD, MIN_COMPRESS_THRESHOLD, Writer, Log, FileReader, parseDataObject, exportEntry, jsonEntry, DATA_OBJECT_HEADER_SIZE, HEADER_SIZE, INCOMPATIBLE_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_ZSTD, OBJECT_TYPE_DATA, FILE_SIZE_INCREASE, STATE_ARCHIVED, parseFileHeader, writeObjectHeader, compressLz4DataPayload, compressXzDataPayload, decompressXzDataPayload, WriterLock, safeExistsSync, safeReadFileSync, safeReaddirSync, packageRoot, run, disposedJournalFiles, clearKeyedHashFlag, writeHeaderSize, verifyJournalFileIfAvailable, journalHasDataObjectFlag } = support;

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'compact-grown.journal');
      const writer = Writer.create(journalPath, { compact: true });
      for (let i = 0; i < 10; i++) {
        writer.append([
          { name: 'BLOB', value: Buffer.alloc(1024 * 1024, i) },
        ], {
          realtimeUsec: 1_700_000_050_000_000n + BigInt(i),
          monotonicUsec: BigInt(i + 1),
        });
      }
      writer.close();

      const header = parseFileHeader(safeReadFileSync(journalPath).subarray(0, HEADER_SIZE));
      assert.ok(
        header.arena_size + BigInt(HEADER_SIZE) > BigInt(FILE_SIZE_INCREASE),
        'arena size must grow past initial allocation',
      );
      const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
      if (journalctl.status === 0) run('journalctl', ['--verify', '--file', journalPath]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'large-hash-table.journal');
      const writer = Writer.create(journalPath, {
        compact: true,
        dataHashTableBuckets: 600000,
        fieldHashTableBuckets: 1023,
      });
      writer.append([
        { name: 'MESSAGE', value: Buffer.from('large hash table') },
      ], {
        realtimeUsec: 1_700_000_060_000_000n,
        monotonicUsec: 1n,
      });
      writer.close();

      const header = parseFileHeader(safeReadFileSync(journalPath).subarray(0, HEADER_SIZE));
      assert.ok(
        header.arena_size + BigInt(HEADER_SIZE) > BigInt(FILE_SIZE_INCREASE),
        'initial arena must cover large hash tables',
      );
      const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
      if (journalctl.status === 0) run('journalctl', ['--verify', '--file', journalPath]);
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
      first.append([{ name: 'MESSAGE', value: 'chain-online-reopen-0' }]);
      first.append([{ name: 'MESSAGE', value: 'chain-online-reopen-1' }]);
      const activePath = first.activeFile();
      first.writer.close();
      first.writer = null;
      first.closed = true;

      const second = new Log(tempDir, { ...options, headSeqnum: 99 });
      assert.equal(second.activeFile(), activePath);
      assert.notEqual(second.writer, null);
      assert.equal(second.nextSeqnum, 3n);
      second.append([{ name: 'MESSAGE', value: 'chain-online-reopen-2' }]);
      second.close();

      const reader = FileReader.open(activePath);
      const seqnums = [];
      while (reader.step()) seqnums.push(reader.getEntry().seqnum);
      reader.close();
      assert.deepEqual(seqnums, [1n, 2n, 3n]);
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
      first.append([{ name: 'MESSAGE', value: 'strict-migrate-0' }], { realtimeUsec: 1700002271000000n, monotonicUsec: 1n });
      first.append([{ name: 'MESSAGE', value: 'strict-migrate-1' }], { realtimeUsec: 1700002271000001n, monotonicUsec: 2n });
      const chainPath = first.activeFile();
      first.writer.close();
      first.writer = null;
      first.closed = true;

      const strict = new Log(tempDir, { ...options, strictSystemdNaming: true });
      const chainReader = FileReader.open(chainPath);
      assert.equal(chainReader.header.state, STATE_ARCHIVED);
      chainReader.close();

      strict.append([{ name: 'MESSAGE', value: 'strict-migrate-2' }], { realtimeUsec: 1700002271000002n, monotonicUsec: 3n });
      assert.equal(strict.activeFile(), join(strict.journalDirectory(), 'system.journal'));
      const activeReader = FileReader.open(strict.activeFile());
      assert.equal(activeReader.header.head_entry_seqnum, 3n);
      assert.equal(activeReader.header.tail_entry_seqnum, 3n);
      activeReader.close();
      strict.close();
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
      first.append([{ name: 'MESSAGE', value: 'replace-chain-0' }], { realtimeUsec: 1700002272000000n, monotonicUsec: 1n });
      first.append([{ name: 'MESSAGE', value: 'replace-chain-1' }], { realtimeUsec: 1700002272000001n, monotonicUsec: 2n });
      const activePath = first.activeFile();
      first.writer.close();
      first.writer = null;
      first.closed = true;

      clearKeyedHashFlag(activePath);
      assert.throws(() => Writer.open(activePath), /keyed hash required/);

      const second = new Log(tempDir, options);
      assert.equal(safeExistsSync(activePath), false);
      assert.equal(disposedJournalFiles(second.journalDirectory()).length, 1);
      second.append([{ name: 'MESSAGE', value: 'replace-chain-2' }], { realtimeUsec: 1700002272000002n, monotonicUsec: 3n });
      assert.notEqual(second.activeFile(), activePath);
      const reader = FileReader.open(second.activeFile());
      assert.equal(reader.header.head_entry_seqnum, 3n);
      assert.equal(reader.header.tail_entry_seqnum, 3n);
      reader.close();
      second.close();
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
        strictSystemdNaming: true,
        maxEntries: 0,
        maxBytes: 0,
        maxFiles: 10,
      };
      const first = new Log(tempDir, options);
      first.append([{ name: 'MESSAGE', value: 'replace-strict-0' }], { realtimeUsec: 1700002273000000n, monotonicUsec: 1n });
      first.append([{ name: 'MESSAGE', value: 'replace-strict-1' }], { realtimeUsec: 1700002273000001n, monotonicUsec: 2n });
      const activePath = first.activeFile();
      first.writer.close();
      first.writer = null;
      first.closed = true;

      writeHeaderSize(activePath, HEADER_SIZE - 8);
      assert.throws(() => Writer.open(activePath), /outdated header/);

      const second = new Log(tempDir, options);
      assert.equal(safeExistsSync(activePath), false);
      assert.equal(disposedJournalFiles(second.journalDirectory()).length, 1);
      second.append([{ name: 'MESSAGE', value: 'replace-strict-2' }], { realtimeUsec: 1700002273000002n, monotonicUsec: 3n });
      assert.equal(second.activeFile(), join(second.journalDirectory(), 'system.journal'));
      const reader = FileReader.open(second.activeFile());
      assert.equal(reader.header.head_entry_seqnum, 3n);
      assert.equal(reader.header.tail_entry_seqnum, 3n);
      reader.close();
      second.close();
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
      first.append([{ name: 'MESSAGE', value: 'empty-reopen-0' }, { name: 'TEST_ID', value: 'node-empty-online-reopen' }]);
      first.append([{ name: 'MESSAGE', value: 'empty-reopen-1' }, { name: 'TEST_ID', value: 'node-empty-online-reopen' }]);
      first.close();

      const journalDir = first.journalDirectory();
      const names = safeReaddirSync(journalDir).filter((name) => name.endsWith('.journal')).sort();
      assert.equal(names.length, 1);
      const reader = FileReader.open(join(journalDir, names.at(0)));
      const seqnumId = Buffer.from(reader.header.seqnum_id);
      const nextSeqnum = reader.header.tail_entry_seqnum + 1n;
      reader.close();

      const emptyPath = join(
        journalDir,
        `system@${uuidToString(seqnumId)}-${nextSeqnum.toString(16).padStart(16, '0')}-00060a24181e040a.journal`,
      );
      const empty = Writer.create(emptyPath, {
        machineId: options.machineId,
        seqnumId,
        headSeqnum: nextSeqnum,
      });
      empty.close();

      const second = new Log(tempDir, options);
      second.append([{ name: 'MESSAGE', value: 'empty-reopen-2' }, { name: 'TEST_ID', value: 'node-empty-online-reopen' }]);
      second.close();
      assert.equal(safeExistsSync(emptyPath), false);

      const seqnums = [];
      for (const name of safeReaddirSync(journalDir).filter((name) => name.endsWith('.journal')).sort()) {
        const fileReader = FileReader.open(join(journalDir, name));
        while (fileReader.step()) seqnums.push(fileReader.getEntry().seqnum);
        fileReader.close();
      }
      assert.deepEqual(seqnums, [1n, 2n, 3n]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const unlockedPath = join(tempDir, 'writer-unlocked-default.journal');
      const unlocked = Writer.create(unlockedPath);
      unlocked.close();
      assert.equal(safeExistsSync(`${unlockedPath}.lock`), false);

      const journalPath = join(tempDir, 'writer-lock.journal');
      const lock = WriterLock.acquire(journalPath);
      const writer = Writer.create(journalPath);
      try {
        writer.append([{ name: 'MESSAGE', value: 'held' }]);
        assert.ok(safeExistsSync(`${journalPath}.lock`));
        assert.throws(() => WriterLock.acquire(journalPath), /journal writer lock held/);
      } finally {
        writer.close();
        lock.release();
      }
      assert.equal(safeExistsSync(`${journalPath}.lock`), false);
      const reopened = Writer.open(journalPath);
      reopened.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    const value = Buffer.from('cafe\u0301', 'utf8');
    const entry = {
      fields: { MESSAGE: value },
      fieldValues: { MESSAGE: [value] },
    };
    assert.deepEqual(jsonEntry(entry).MESSAGE, value.toString('utf8'));
    assert.match(exportEntry(entry).toString('utf8'), /^MESSAGE=cafe\u0301\n\n$/u);
  }

  {
    const value = Buffer.from([0xff, 0xfe, 0xfd]);
    const entry = {
      fields: { FIELD: value },
      fieldValues: { FIELD: [value] },
    };
    const output = exportEntry(entry);
    const marker = Buffer.from('FIELD\n', 'utf8');
    const markerOffset = output.indexOf(marker);
    assert.notEqual(markerOffset, -1);
    const sizeOffset = markerOffset + marker.length;
    assert.equal(output.readBigUInt64LE(sizeOffset), BigInt(value.length));
    assert.deepEqual(output.subarray(sizeOffset + 8, sizeOffset + 8 + value.length), value);
  }

  {
    const value = Buffer.from('line1\nline2', 'utf8');
    const entry = {
      fields: { FIELD: value },
      fieldValues: { FIELD: [value] },
    };
    const output = exportEntry(entry);
    const marker = Buffer.from('FIELD\n', 'utf8');
    const markerOffset = output.indexOf(marker);
    assert.notEqual(markerOffset, -1);
    const sizeOffset = markerOffset + marker.length;
    assert.equal(output.readBigUInt64LE(sizeOffset), BigInt(value.length));
    assert.deepEqual(output.subarray(sizeOffset + 8, sizeOffset + 8 + value.length), value);
  }

  {
    const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + 3);
    const unsupportedFlag = 0x80;
    writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, unsupportedFlag, BigInt(DATA_OBJECT_HEADER_SIZE + 3));
    Buffer.from('A=x').copy(buf, DATA_OBJECT_HEADER_SIZE);
    assert.throws(() => parseDataObject(buf, 0), /unsupported DATA object flags/);
  }

  {
    const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + 3);
    writeObjectHeader(
      buf,
      0,
      OBJECT_TYPE_DATA,
      OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_XZ,
      BigInt(DATA_OBJECT_HEADER_SIZE + 3),
    );
    Buffer.from('A=x').copy(buf, DATA_OBJECT_HEADER_SIZE);
    assert.throws(() => parseDataObject(buf, 0), /unsupported DATA object compression flags/);
  }

  {
    const payload = Buffer.from(`MESSAGE=${'lz4-data-object'.repeat(16)}`);
    const compressed = compressLz4DataPayload(payload);
    assert.ok(compressed);
    const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + compressed.length);
    writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, OBJECT_COMPRESSED_LZ4, BigInt(DATA_OBJECT_HEADER_SIZE + compressed.length));
    compressed.copy(buf, DATA_OBJECT_HEADER_SIZE);
    const parsed = parseDataObject(buf, 0);
    assert.deepEqual(parsed.name, Buffer.from('MESSAGE'));
    assert.deepEqual(parsed.value, Buffer.from('lz4-data-object'.repeat(16)));
  }

  {
    const payload = Buffer.from(`MESSAGE=${'xz-data-object'.repeat(16)}`);
    const compressed = compressXzDataPayload(payload);
    assert.ok(compressed);
    const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + compressed.length);
    writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, OBJECT_COMPRESSED_XZ, BigInt(DATA_OBJECT_HEADER_SIZE + compressed.length));
    compressed.copy(buf, DATA_OBJECT_HEADER_SIZE);
    const parsed = parseDataObject(buf, 0);
    assert.deepEqual(parsed.name, Buffer.from('MESSAGE'));
    assert.deepEqual(parsed.value, Buffer.from('xz-data-object'.repeat(16)));
  }

  {
    // XZ CHECK_NONE: verify the emitted stream header uses check type 0 (None).
    const payload = Buffer.from(`MESSAGE=${'xz-check-none-test'.repeat(16)}`);
    const compressed = compressXzDataPayload(payload);
    assert.ok(compressed);
    // systemd XZ magic: fd 37 7a 58 5a 00 (6 bytes), then stream flags (2 bytes).
    assert.equal(compressed.subarray(0, 6).toString('hex'), 'fd377a585a00');
    assert.equal(compressed[6], 0x00, 'XZ stream flag byte 0 must be zero');
    assert.equal(compressed[7], 0x00, 'XZ stream must use CHECK_NONE (0)');
  }

  {
    // XZ rejects payloads below minimum compression threshold (80 bytes).
    const smallPayload = Buffer.from('short');
    assert.equal(compressXzDataPayload(smallPayload), null);
  }

  {
    // XZ decompression rejects corrupt/invalid payloads.
    const corrupt = Buffer.alloc(32, 0xff);
    assert.throws(() => decompressXzDataPayload(corrupt), /xz decompression/);
  }

  {
    // Verify the supported runtime path does not load native addons.
    const req = createRequire(import.meta.url);
    const nativeAddonKeys = Object.keys(req.cache).filter(
      (k) => k.startsWith(packageRoot) && k.endsWith('.node'),
    );
    assert.equal(nativeAddonKeys.length, 0, 'SDK runtime path must not load .node native addons');
  }

  {
    // Verify production dependencies do not require native install hooks.
    const lock = JSON.parse(safeReadFileSync(join(packageRoot, 'package-lock.json'), 'utf8'));
    const packages = Object.entries(lock.packages ?? {});
    const installScriptPackages = packages
      .filter(([, metadata]) => metadata.hasInstallScript === true)
      .map(([name]) => name);
    assert.deepEqual(installScriptPackages, [], 'package-lock must not include install-script dependencies');
    assert.equal(lock.packages?.['node_modules/node-liblzma'], undefined, 'full node-liblzma package must not be a dependency');
    assert.equal(lock.packages?.['node_modules/node-gyp-build'], undefined, 'node-gyp-build must not be a dependency');
  }

  {
    // Keep vendored WASM provenance executable, not only documented.
    const expected = new Map([
      ['liblzma.js', 'f33997f0c680a29fd307d18b8336325949811c78bb00ad9a038bf8f205623e02'],
      ['liblzma.wasm', 'a9216b509c9bf0006f306e85f696bd67d31e4ca1972b9e35307aef8650fe705c'],
      ['LICENSE', 'f97bc4bb9b7ae8a653941073678b5c7775e8de44a01c3bcc21e7cdc148b90e61'],
    ]);
    const vendorDir = join(packageRoot, 'vendor/node-liblzma-wasm');
    for (const [fileName, expectedHash] of expected) {
      const actualHash = createHash('sha256')
        .update(safeReadFileSync(join(vendorDir, fileName)))
        .digest('hex');
      assert.equal(actualHash, expectedHash, `${fileName} vendor hash mismatch`);
    }
  }

  {
    // Verify package.json "files" array includes index.d.ts so the
    // declared "types" field actually ships in the npm tarball.
    const pkg = JSON.parse(safeReadFileSync(join(packageRoot, 'package.json'), 'utf8'));
    assert.ok(Array.isArray(pkg.files), 'package.json must have a "files" array');
    assert.ok(pkg.files.includes('index.d.ts'),
      'package.json "files" must include index.d.ts for the "types" field');
  }

  {
    // Node writer -> Node reader round-trip with XZ-compressed DATA.
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, 'xz-writer-reader-roundtrip.journal');
      const writer = Writer.create(journalPath, { compression: 'xz', compressionThresholdBytes: 80 });
      const value = 'xz-roundtrip-test-value-'.repeat(16);
      writer.append([{ name: 'MESSAGE', value }]);
      writer.close();
      assert.ok(
        journalHasDataObjectFlag(journalPath, OBJECT_COMPRESSED_XZ),
        'journal must contain at least one XZ-compressed DATA object',
      );
      const reader = FileReader.open(journalPath);
      assert.ok(reader.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ, 'header must have XZ incompatible flag');
      assert.ok(reader.next());
      const entry = reader.getEntry();
      assert.ok(entry);
      assert.deepEqual(entry.fields.MESSAGE, Buffer.from(value));
      assert.equal(reader.next(), false);
      reader.close();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  {
    // Compression threshold policy follows systemd: default 512 bytes, minimum 8 bytes.
    for (const testCase of [
      {
        name: 'default below threshold',
        options: {},
        payloadLength: DEFAULT_COMPRESS_THRESHOLD - 1,
        wantThreshold: DEFAULT_COMPRESS_THRESHOLD,
        wantCompressed: false,
      },
      {
        name: 'default exact threshold',
        options: {},
        payloadLength: DEFAULT_COMPRESS_THRESHOLD,
        wantThreshold: DEFAULT_COMPRESS_THRESHOLD,
        wantCompressed: true,
      },
      {
        name: 'minimum clamp',
        options: { compressionThresholdBytes: 1 },
        payloadLength: MIN_COMPRESS_THRESHOLD - 1,
        wantThreshold: MIN_COMPRESS_THRESHOLD,
        wantCompressed: false,
      },
      {
        name: 'minimum clamp eligible payload',
        options: { compressionThresholdBytes: 1 },
        payloadLength: DEFAULT_COMPRESS_THRESHOLD,
        wantThreshold: MIN_COMPRESS_THRESHOLD,
        wantCompressed: true,
      },
    ]) {
      const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
      try {
        const journalPath = join(tempDir, `zstd-threshold-${testCase.name.replaceAll(' ', '-')}.journal`);
        const writer = Writer.create(journalPath, { compression: 'zstd', ...testCase.options });
        assert.equal(writer.compressThreshold, testCase.wantThreshold);
        writer.append([{ name: 'F', value: Buffer.alloc(testCase.payloadLength - 2, 0x41) }]);
        writer.close();
        assert.equal(
          journalHasDataObjectFlag(journalPath, OBJECT_COMPRESSED_ZSTD),
          testCase.wantCompressed,
          testCase.name,
        );
        verifyJournalFileIfAvailable(journalPath);
      } finally {
        rmSync(tempDir, { recursive: true, force: true });
      }
    }
  }
}
