# Journalctl CLI

Each product SDK language ships a file-backed `journalctl` rewrite. It is the fifth
consumption surface: a command-line tool, not a library API. Use it when an
operator or a script needs stock-like `journalctl` behavior against journal
files or directories without systemd, libsystemd, or a running journald.

The Rust and Go rewrites implement the same file-backed contract and are
validated against stock `journalctl` by the shared interoperability matrices: directory
traversal, mixed-format directories, query/follow behavior, and verification.

## Scope

The rewrites cover file-backed query behavior only:

- input selection: `--file <path-or-glob>`/`-i <path-or-glob>` or
  `--directory <path>`; repeated `--file` inputs and glob expansion match
  stock file-backed behavior, while `--file=-` is explicitly unsupported
  because portable stdin journals would need seekable mmap-capable descriptors;
- output: all stock v262 file-backed modes: `short`, `short-full`,
  `short-iso`, `short-iso-precise`, `short-precise`, `short-monotonic`,
  `short-delta`, `short-unix`, `verbose`, `export`, `json`,
  `json-pretty`, `json-sse`, `json-seq`, `cat`, and `with-unit`;
  `--output=help` prints the official mode list without opening a journal;
- short-style labels: hostname, identifier/unit, and PID are rendered from
  journal fields like stock file-backed `journalctl`; `--no-hostname`
  suppresses only the hostname component;
- field projection: `--output-fields` for `verbose`, `export`, `json*`, and
  `cat`, with stock metadata retention for JSON/export modes;
- output controls: full-width text output is the default; `--no-full`, `--all`,
  and empty-result messages follow stock v262 behavior, including JSON large
  field `null` thresholding without `--all`;
- metadata: `--list-boots`, `--list-invocations`, `--fields`,
  `--field <NAME>` (unique values), and `--header`;
- filtering: `FIELD=value` matches, repeated same-field matches as OR,
  different fields as AND, and the `+` disjunction separator; positional
  matches apply to the show action only, and non-show actions reject extra
  positional arguments like stock `journalctl`;
- journalctl filters: `--identifier`, `--exclude-identifier`, `--priority`,
  `--facility`, `--grep`, `--case-sensitive`, `--dmesg`, `--unit`,
  `--user-unit`, `--invocation`, and `-I`;
- unit filters: exact and glob system/user units use the same journal fields
  and disjunction groups as stock file-backed `journalctl`; `--user --unit=`
  is treated as `--user-unit=` unless `--system` is also present;
- time and boot windows: `--since`, `--until`, `--boot [ID]`; bare `--boot`
  selects the current boot, while explicit empty `--boot=` is rejected like
  stock `journalctl`;
- cursors: `--cursor`, `--after-cursor`, `--cursor-file`, and
  `--show-cursor`, using official systemd cursor strings;
- paging: stock `--lines[=[+]N]`, including bare `--lines`/`-n` defaulting to
  10 entries and explicit empty `--lines=` being rejected, `--no-tail`, and
  `--pager-end` implicit 1000-line tail behavior, plus SDK extension aliases
  `--head <n>` and `--tail <n>`;
- live reads: `--follow` on actively appended files and directories;
- utility actions: `--new-id128` and `--disk-usage` for explicit
  `--file`/`--directory` inputs;
- maintenance actions: `--vacuum-size`, `--vacuum-files`, and `--vacuum-time`
  for explicit `--directory` inputs, using stock archived-file deletion rules;
- integrity: `--verify`, and `--verify-key <key>` for sealed files, where
  `<key>` is the systemd-style verification key `<seed>/<start>-<interval>`
  produced when sealing was set up (the same value stock
  `journalctl --verify-key` accepts).

Host and daemon operations are out of scope by design. `--sync`, `--flush`,
`--rotate`, `--relinquish-var`, and `--setup-keys` return a controlled error
in every language.

## v262 Compatibility And Migration

The command target is systemd v262. The SDK journal-format baseline remains
v260.1; this command update does not change journal bytes or compiler minimums.

- `-F FIELD` and `-N` reject time bounds, entry matches, cursor restrictions,
  grep and explicit boot/invocation restrictions. Earlier versions ignored
  some of these combinations. Use unrestricted enumeration for the complete
  file, or the SDK to enumerate values from filtered entries.
- Required option values remain literal, even when they look like flags.
  Setters validate in encounter order; terminal help/version/introspection
  stops parsing immediately. The last action controls dispatch.
  Integer options follow native base-prefix, octal and range rules; priority
  names use the canonical lowercase spellings.
- Grep uses PCRE2 byte semantics, including lookarounds and backreferences.
  It matches the first raw MESSAGE value. No matches means exit 1 with empty
  stderr; an invalid pattern or matching error includes a diagnostic.
- Unit wildcards use indexed field values. When all system/user unit selectors
  are wildcards and none expands, the command exits 1 with a no-data error.
  An exact unit name remains a valid selector even when it has no entries.
  Native unit naming supports suffixless globs, escaped names, and logical
  device/mount paths without inspecting the host filesystem. Empty names are
  invalid. POSIX named character classes also work on FreeBSD. Follow keeps
  the initial wildcard expansion.
- Forward cursor limits start at the cursor. Cursor-selected follow history,
  explicit all/no-tail and since-only follow include the complete selected
  backlog. `-f` means follow; use `-i` or `--file` to select a file.
  Follow defaults to ten existing rows without since, including with `-e`.
  With both since and `-e`, its default is one thousand existing rows.
- Cursor output and cursor files retain the final visited journal position,
  including grep misses and a crossed time bound. A zero line limit still
  positions the cursor, without evaluating MESSAGE patterns.
- Repeated output projections form a union. Field-name spaces are literal.
  Cat writes first raw values, including embedded NULs and newlines.
  Boot/invocation tables support JSON, pretty JSON, SSE and JSON sequences.
  A zero table limit reports no data with exit 1. Quiet suppresses informational
  diagnostics, including successful verification messages.
- On Unix, exact matches, identifiers and projection options preserve raw
  argument bytes. A projection that names no field still restricts the output.
- Unix file and directory names preserve their original bytes. File patterns
  use the platform's glob rules and environment locale, including brace
  alternatives, brackets, escaping and explicit leading dots for hidden names.
  Windows supports Unicode filenames, brace alternatives and Unicode wildcards.
- Colors follow `SYSTEMD_COLORS`, `NO_COLOR`, terminal detection, `TERM` and
  `COLORTERM`. Priority text, grep matches, verbose fields, JSON values, boot
  separators and table headers follow native styles. Fixed color modes and
  explicit true override `NO_COLOR`; automatic modes respect it. Plain output
  avoids color formatting and additional regex matching.
  Embedded documentation and configuration-file links follow `SYSTEMD_URLIFY`.
  Windows file links encode drive and UNC paths as file URIs, including spaces
  and Unicode; unsupported device paths remain plain text.
  Portable commands do not consult host catalog databases for implicit links.
- Short output uses the last repeated MESSAGE and header values. `--no-full` shortens
  each line independently, retains the native prefix/suffix split and respects
  line/byte thresholds and terminal character widths. Valid `COLUMNS` values
  override stdout's terminal width, with 80 columns as the final fallback.
- Display timestamps use the last source-realtime value when native integer
  parsing yields a value greater than zero and less than 2^55. Invalid values
  fall back to the ENTRY timestamp; exported ENTRY metadata stays unchanged.
- Short output removes completed color/OSC sequences, expands tabs to eight
  spaces and removes trailing or pre-newline carriage returns. `--all` keeps
  raw text. JSON preserves repeated binary values as separate byte arrays.
- Vacuum sizes accept ordered uppercase compound units, such as `1G512M`;
  lowercase and `KiB` suffixes are rejected.
  Retention considers the oldest available filename or filesystem timestamp,
  including access and creation times. Linux also recognizes the legacy
  `user.crtime_usec` attribute and preserves atime on header reads when allowed.
  Creation metadata adds filesystem work during age-based cleanup. The measured
  warm control costs about 7 ms extra in Go and 3.8 ms in Rust for 1,000 recent
  archive names. This accepted maintenance cost does not apply to ordinary
  queries or SDK reads/writes; actual timings depend on the filesystem.
- Grep does not hide crossed boot separators. Initial follow-tail selection
  counts journal rows before grep; subsequent polls resume after the last
  visited row, including rows that grep did not select.
- Timestamp bounds parse once per occurrence. Calendar fractions round to
  microseconds; duration/epoch fractions follow native checked arithmetic.
  UTC, numeric offsets, IANA zones, DST folds/gaps and TZif leap records are
  covered by the stock-oracle controls. Available named zones follow the
  installed database, with a bundled fallback when it is absent. Calendar
  output also honors POSIX `TZ` rules, custom `TZDIR` data and TZif leap records.
  Rules that omit daylight-saving dates use the configured `posixrules` file
  before the native fallback dates.
  As in stock journalctl, `--utc` changes entry timestamps; header and
  boot/invocation tables retain local time.

## Archive Retention Ordering

Vacuum preserves sequence-number order within each archived writer history.
Across histories it selects the oldest adjusted timestamp among the next
available files. Equal sequence numbers use filename order; equal head times
use sequenced files first, then sequence ID and filename. Archives without
sequence metadata are independent candidates. This makes Rust and Go agree
regardless of directory enumeration order.

This is an intentional exception to stock behavior when clock corrections make
its sequence/time comparisons contradictory. Stock's survivor selection then
depends on its sorting implementation and enumeration order. These commands
use the deterministic rule above instead of reproducing that incidental order.
Age retention still stops when the next eligible file meets the age limit and
size/count limits are satisfied; it does not skip a file to remove a later
sequence from the same history. Empty-archive cleanup and protected active
files retain their existing behavior.

## Build Prerequisites

Both commands require PCRE2-8 development files (10.42 or later), `pkg-config`
and a C toolchain. The Go command additionally requires `CGO_ENABLED=1`.
SDK-only applications require neither PCRE2 nor CGO.

| Platform | Native command build inputs |
|---|---|
| Linux | C compiler, `pkg-config`, `libpcre2-dev` or equivalent |
| macOS | Apple command-line tools, Homebrew `pcre2` and `pkgconf` |
| Windows | MSYS2 CLANG64 compiler, pkgconf and PCRE2; Rust GNU LLVM target |
| FreeBSD | Native compiler, `pcre2` and `pkgconf` packages |

The maintained native build and smoke commands are in the `Portable journalctl`
workflow. Windows uses the CLANG64 PCRE2 import library and needs the matching
DLL on PATH. Other dynamically linked builds also need the matching runtime
library. Native CI checks complement Linux's complete v262 oracle matrix;
SDK cross-compilation alone does not validate these command dependencies.

Native dependencies add a small fixed command startup cost. On the measured
tiny Linux command, Go changes from about 3.0ms to 3.8ms and Rust from 1.5ms
to 1.8ms. This CLI-only allowance does not relax SDK or sustained query
performance requirements.
These measurements describe that fixture and host, not a cross-platform timing
guarantee.

## Portable Mode Limits

The rewrites are file-backed tools. They never connect to journald, read the
host journal by default, or use daemon wait APIs. `--follow` therefore watches
explicit files and directories by portable polling. Correctness is covered by
the shared stock-oracle follow matrix, but very large inputs may cost more than
daemon-backed Linux `journalctl --follow`.

`--list-invocations` follows indexed unit and invocation matches. Its time range
includes every row carrying the selected ID within the boot context, including
rows without unit metadata. Invocation show queries retain identifier,
priority, facility and transport filters; follow keeps its initially selected
invocation. Broader large-archive performance work remains tracked internally.

## Running Each Rewrite

<!-- illustrative-only: build/run commands depend on local toolchains and paths -->
```sh
# Rust (builds the journalctl binary from the workspace)
cargo run --manifest-path rust/Cargo.toml --bin journalctl -- \
    --file ./fixtures/system.journal --output json PRIORITY=3

# Go
CGO_ENABLED=1 go -C go run ./cmd/journalctl --directory ../journal-dir --list-boots
CGO_ENABLED=1 go -C go run ./cmd/journalctl --file ../fixtures/system.journal --disk-usage
CGO_ENABLED=1 go -C go run ./cmd/journalctl --directory ../journal-dir --vacuum-time=30d
CGO_ENABLED=1 go -C go run ./cmd/journalctl --new-id128

```

The Rust and Go rewrites accept the same flags and produce matching output for
the shared matrices' covered behavior. When outputs disagree, treat it as an SDK
bug and report it.

## When To Use The CLI Versus The APIs

| Need | Use |
|---|---|
| operator inspection of journal files | journalctl rewrite |
| scripted text/JSON/export extraction | journalctl rewrite |
| integrity check in a pipeline | journalctl rewrite `--verify` |
| application reading rows programmatically | reader APIs |
| log explorer queries (facets, histogram, FTS) | Explorer API |
| Netdata-shaped function output | Netdata function boundary |

The CLI consumes SDK reader paths and adds command parsing, rendering and
PCRE2 filtering. Applications use SDK APIs directly and choose their own
filtering and presentation dependencies.
