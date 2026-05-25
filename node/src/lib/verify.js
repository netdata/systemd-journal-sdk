// Journal file verification.
// Validates structural integrity of unsealed journal files.
// Sealed FSS tag/HMAC verification is not yet implemented.

import { FileReader } from './reader.js';
import { parseEntryObject, parseDataObject } from './entry.js';
import { INCOMPATIBLE_COMPACT } from './header.js';

export class VerificationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'VerificationError';
  }
}

/**
 * Validate the structural integrity of a journal file.
 *
 * Opens the file (decompressing .zst if needed), validates the header,
 * and walks all entries and their referenced data objects strictly.
 * Any parse or decompression error is reported as a VerificationError.
 *
 * For sealed journals, tag/HMAC verification is not yet implemented.
 */
export function verifyFile(path) {
  let r;
  try {
    r = FileReader.open(path);
  } catch (err) {
    throw new VerificationError(
      `journal verification failed: corrupt or unreadable file: ${err.message}`
    );
  }

  try {
    // Verification walks internal parser state so corrupt data objects fail
    // instead of being skipped by the normal reader tolerance path.
    const buf = r.buffer;
    const compact = (r.header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;

    for (const offset of r.entryOffsets) {
      // Parse entry object strictly
      let e;
      try {
        e = parseEntryObject(buf, Number(offset), compact);
      } catch (err) {
        throw new VerificationError(
          `journal verification failed: corrupt entry object at offset ${offset}: ${err.message}`
        );
      }

      // Parse each referenced data object strictly
      for (const item of e.items) {
        const dataOff = Number(item.offset);
        try {
          parseDataObject(buf, dataOff, compact);
        } catch (err) {
          throw new VerificationError(
            `journal verification failed: corrupt data object at offset ${dataOff} ` +
            `for entry at offset ${offset}: ${err.message}`
          );
        }
      }
    }
  } finally {
    r.close();
  }
}
