# node-liblzma WASM Runtime

This directory contains only the WASM runtime files used by the Node.js SDK for
systemd journal XZ DATA object compression and decompression.

## Source

- Package: `node-liblzma`
- Version: `5.0.1`
- Repository: `https://github.com/oorabona/node-liblzma`
- npm integrity: `sha512-YdRP4seOYNpL1hGGC3PHdzDDTNsk0hNhmcL3CJNV3p5YVEm6Vr7bdVtJnzkSk4ESj/MYLS7V+syFRVWf51IBrg==`

## File Integrity

These files are copied byte-for-byte from the `node-liblzma@5.0.1` npm
package:

- `liblzma.js`: `sha256:f33997f0c680a29fd307d18b8336325949811c78bb00ad9a038bf8f205623e02`
- `liblzma.wasm`: `sha256:a9216b509c9bf0006f306e85f696bd67d31e4ca1972b9e35307aef8650fe705c`
- `LICENSE`: `sha256:f97bc4bb9b7ae8a653941073678b5c7775e8de44a01c3bcc21e7cdc148b90e61`

## Included Files

- `liblzma.js`
- `liblzma.wasm`
- `LICENSE`

The full `node-liblzma` npm package also ships native Node.js addon prebuilds
and a native install hook. This SDK does not use those paths. Vendoring only
the WASM runtime files keeps the published SDK package free of native install
hooks while preserving the existing XZ `CHECK_NONE` behavior required by the
journal compatibility tests.
