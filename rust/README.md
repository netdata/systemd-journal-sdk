# Rust Journal SDK

This workspace contains pure-Rust systemd journal reader and writer components.
It does not link to libsystemd or other system journal libraries for SDK
behavior.

Current writer scope:

- regular, non-compact journal files;
- uncompressed DATA objects by default;
- optional zstd-compressed DATA object writing through `JournalFileOptions` and
  `journal::Config`;
- keyed hash tables using the journal file ID;
- byte-safe field values through `&[u8]` field payloads;
- direct-file writing through `journal_core`;
- high-level directory writing through `journal::Log`;
- systemd-compatible active/archive file naming;
- entry-count and file-size rotation;
- archived file-count and byte-size retention;
- live stock-reader validation for the current writer slice with `journalctl
  --file`, `journalctl --file --follow --no-tail --boot=all`, and libsystemd
  reader APIs, including live sequence-order checks.

Deferred scope:

- xz/lz4 DATA object writing;
- Forward Secure Sealing and TAG objects;
- compact-format writer support;
- appending to arbitrary historical or systemd-created journal variants;
- duration-based directory rotation and retention;
- full journal verification/FSS validation.

Current reader scope:

- regular non-compact journal files;
- `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst` files;
- zstd-compressed fixture files;
- zstd, lz4, and xz-compressed DATA objects through pure-Rust dependencies;
- directory reading across active and archived files;
- forward/backward iteration, cursors, realtime timestamps, field enumeration,
  binary field values, and export/json/text formatting;
- `--output export` uses systemd's size-prefixed binary field encoding and
  blank-line entry separator;
- JSON output includes realtime and monotonic timestamps, preserves valid UTF-8
  strings, and encodes binary values as arrays of unsigned bytes;
- libsystemd-style match behavior: AND between different fields, OR between
  values for the same field, `SdJournalAddDisjunction()` for `+`, and
  `SdJournalAddConjunction()` for explicit AND groups;
- a file-backed `journalctl` command under `src/cmd/journalctl`;
- a conformance adapter under `src/adapter`.

Reader limitations:

- compact-format journal files are not part of the accepted feature slice;
- directory iteration is sequential by journal file and intended for
  non-overlapping rotated files in this slice; realtime interleaving across
  overlapping multi-file directories is tracked with the broader
  interoperability phase;
- `list_boots` uses file-level boot metadata in this slice;
- full journal verification, FSS validation, and daemon-only journalctl
  operations are not implemented.

Basic directory writer usage:

```rust
use journal::{Config, Log, Origin, RetentionPolicy, RotationPolicy, Source};

let origin = Origin {
    machine_id: None,
    namespace: None,
    source: Source::System,
};
let config = Config::new(
    origin,
    RotationPolicy::default(),
    RetentionPolicy::default(),
);
let mut log = Log::new("/var/log/journal-sdk", config)?;

log.write_entry(
    &[
        b"MESSAGE=plugin started".as_slice(),
        b"PRIORITY=6".as_slice(),
        b"SYSLOG_IDENTIFIER=netdata-plugin".as_slice(),
    ],
    None,
)?;
log.sync()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Binary-safe values:

```rust
log.write_entry(
    &[
        b"MESSAGE=sample with binary payload".as_slice(),
        b"BINARY_PAYLOAD=\x00\x01\x02\xff".as_slice(),
    ],
    None,
)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Basic reader usage:

```rust
use journal::FileReader;

let mut reader = FileReader::open("/path/to/system.journal")?;
reader.add_match(b"PRIORITY=6");
reader.seek_head();

while let Some(entry) = reader.next()? {
    if let Some(message) = entry.get_str("MESSAGE") {
        println!("{message}");
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

File-backed journalctl:

```sh
cargo run --manifest-path rust/Cargo.toml -p journalctl -- \
  --file fixtures/systemd/test-data/no-rtc/system.journal.zst \
  --head 1 \
  --output json
```

Repeated matches for the same field are OR alternatives. Matches for different
fields are ANDed. A separate `+` argument creates an explicit disjunction:

```sh
cargo run --manifest-path rust/Cargo.toml -p journalctl -- \
  --file ./sample.journal \
  PRIORITY=3 PRIORITY=4 + MESSAGE=boot
```
