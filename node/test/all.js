#!/usr/bin/env node

import { run as runChunk1 } from './chunks/header_hash_writer.js';
import { run as runChunk2 } from './chunks/compact_directory_basic.js';
import { run as runChunk3 } from './chunks/directory_lifecycle.js';
import { run as runChunk4 } from './chunks/directory_retention_policy.js';
import { run as runChunk5 } from './chunks/facade_reader_verify.js';
import { run as runChunk6 } from './chunks/seal_conformance.js';
import { run as runChunk7 } from './chunks/explorer.js';
import { run as runChunk8 } from './chunks/netdata.js';
import { run as runChunk9 } from './chunks/netdata-chunk2b.js';
import { run as runChunk10 } from './chunks/netdata-chunk2c.js';
import { run as runChunk11 } from './chunks/wrapper.js';
import { run as runChunk12 } from './chunks/header-only-read.js';
import { run as runChunk13 } from './chunks/sow0105-fixes.js';

for (const runChunk of [runChunk1, runChunk2, runChunk3, runChunk4, runChunk5, runChunk6, runChunk7, runChunk8, runChunk9, runChunk10, runChunk11, runChunk12, runChunk13]) {
  await runChunk();
}

console.log('PASS node package tests');
