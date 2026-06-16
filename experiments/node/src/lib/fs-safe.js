// Validated filesystem boundary for caller-provided journal paths.
/* eslint-disable security/detect-non-literal-fs-filename -- The SDK operates on validated caller-provided journal paths. */

import {
  existsSync as fsExistsSync,
  mkdirSync as fsMkdirSync,
  openSync as fsOpenSync,
  readFileSync as fsReadFileSync,
  readdirSync as fsReaddirSync,
  renameSync as fsRenameSync,
  rmdirSync as fsRmdirSync,
  statSync as fsStatSync,
  symlinkSync as fsSymlinkSync,
  unlinkSync as fsUnlinkSync,
  writeFileSync as fsWriteFileSync,
} from 'node:fs';

export function safeExistsSync(path) {
  return fsExistsSync(validatedPath(path));
}

export function safeMkdirSync(path, options) {
  return fsMkdirSync(validatedPath(path), options);
}

export function safeOpenSync(path, flags, mode) {
  return fsOpenSync(validatedPath(path), flags, mode);
}

export function safeReadFileSync(path, options) {
  return fsReadFileSync(validatedPath(path), options);
}

export function safeReaddirSync(path, options) {
  return fsReaddirSync(validatedPath(path), options);
}

export function safeRenameSync(oldPath, newPath) {
  return fsRenameSync(validatedPath(oldPath, 'oldPath'), validatedPath(newPath, 'newPath'));
}

export function safeRmdirSync(path) {
  return fsRmdirSync(validatedPath(path));
}

export function safeStatSync(path) {
  return fsStatSync(validatedPath(path));
}

export function safeSymlinkSync(target, path, type) {
  return fsSymlinkSync(validatedPath(target, 'target'), validatedPath(path), type);
}

export function safeUnlinkSync(path) {
  return fsUnlinkSync(validatedPath(path));
}

export function safeWriteFileSync(path, data, options) {
  return fsWriteFileSync(validatedPath(path), data, options);
}

export function validatedPath(path, label = 'path') {
  if (typeof path === 'string') {
    if (path.includes('\0')) throw new Error(`invalid ${label}: contains NUL byte`);
    return path;
  }
  if (path instanceof URL) {
    if (path.protocol !== 'file:') throw new Error(`invalid ${label}: URL protocol must be file:`);
    return path;
  }
  throw new TypeError(`invalid ${label}: expected string or file URL`);
}
