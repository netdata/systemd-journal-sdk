# journalctl v262 Portable Parity Matrix

## Scope

This matrix defines the Rust and Go portable `journalctl` target for
SOW-0121.

Baseline authority:

- `systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492` (`v262`)
- `src/journal/journalctl.c`: OPTION/OPTION_FULL parser records, help and ordered setters
- `src/shared/options.h`: common option macros
- `src/journal/journalctl.h`: action enum
- `src/shared/output-mode.c`: output mode names
- `src/journal/journalctl-filter.c`: file-backed filter construction
- `src/journal/journalctl-show.c`: seek, cursor, follow, and output loop
- `src/shared/pcre2-util.c`: PCRE2-8 byte matching and automatic casing
- `src/basic/time-util.c`: timestamp/duration parsing

The SDK journal-format baseline remains v260.1. The historical matrix and
manifest filenames are retained so existing links and validation commands work.
Both CLIs require PCRE2-8 >=10.42; Go enables CGO only in the CLI. SDK consumers
remain independent of PCRE2 and CGO.

The portable Rust and Go commands must recognize 100% of the official v262
command-line option and action surface. Unknown-option errors for official
v262 options are compatibility failures.

Calendar rendering and bound parsing share native-compatible timezone
resolution, including POSIX TZ rules, TZDIR, TZif leap records and pre-transition
footer behavior. POSIX rules without daylight transition dates use the configured
`posixrules` file before the native US-date fallback. Header/boot/invocation tables retain local time with --utc,
matching stock; entry calendar output honors --utc while retaining leap records.
Short-unix preserves raw epoch values and the native ten-column seconds width.
Rendering timezone data is loaded lazily once per command process.

## Classification Values

- `file-backed-required`: implement operational parity for explicit
  `--file=`/`--directory=` inputs.
- `file-backed-maintenance-required`: implement only for explicit
  `--directory=` inputs because the action mutates or removes journal files.
  Without explicit `--directory=`, return portable unsupported behavior.
- `portable-utility-required`: implement because the feature does not require
  journald, host journal state, or system journal libraries.
- `recognized-no-op`: parse and accept. Preserve parser side effects when they
  affect file-backed output, otherwise do nothing.
- `recognized-unsupported`: parse, validate arguments when practical, then fail
  intentionally with a portable unsupported message. Do not inspect live host
  journal state, invoke systemd services, or mutate host journal/catalog paths.
- `parser-required`: parser behavior that must match official v262 before any
  action dispatch.

## Error Contract

Unsupported portable-mode features must fail with a message shaped like:

```text
journalctl portable mode does not support <feature>: <reason>
```

The exact feature and reason should be specific enough for users to understand
whether the operation is daemon-only, host-source-only, catalog-database-only,
disk-image/rootfs-only, or unsafe without an explicit file/directory target.

## Source Options

| Option | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `--system` | none | recognized-no-op | Recognize. With explicit file input it has no effect. With explicit directory input, filter system journal files only if repository directory metadata supports it; otherwise accept as no-op and record test evidence. Without file/directory input, return unsupported because default host journal discovery is not portable. |
| `--user` | none | recognized-no-op | Recognize. With explicit file input it has no effect. With explicit directory input, filter current-user journal files only if repository directory metadata supports it; otherwise accept as no-op and record test evidence. Without file/directory input, return unsupported because current-user host journal discovery is not portable. |
| `-M`, `--machine=` | string | recognized-unsupported | Requires local container or machine journal access. Never connect to a host or container. |
| `-m`, `--merge` | none | recognized-no-op | Recognize. Interleaving multiple explicit file/directory inputs is already the portable directory/file behavior. Preserve official conflicts with specific `--boot` selectors and `--list-boots`; allow `--boot=all` with `--merge` like systemd v262. |
| `-D`, `--directory=` | path | file-backed-required | Open journal files from the supplied directory, including supported repository `.journal.zst` directory files. Reject regular-file input passed through `--directory=` instead of silently treating it like `--file=`. |
| `-i`, `--file=` | glob/path | file-backed-required | Open one or more explicit journal files. Multiple `--file=` occurrences and glob expansion must be supported. `--file=-` must be recognized; support only if a seekable stdin-backed implementation exists, otherwise return a specific unsupported message. |
| `--root=` | path | recognized-unsupported | Requires alternate root filesystem discovery and catalog hierarchy access. Do not inspect host rootfs. |
| `--image=` | path | recognized-unsupported | Requires disk image dissection/mounting. Do not mount or inspect images. |
| `--image-policy=` | policy | recognized-unsupported | Only meaningful with `--image=`; parse and reject with the same portable image unsupported reason. |
| `--namespace=` | string | recognized-unsupported | Requires systemd journal namespaces. Do not discover or open host namespaces. |

## Filtering Options

| Option | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `-S`, `--since=` | timestamp | file-backed-required | Inclusive realtime lower bound. Parse native calendar/epoch/relative forms, checked compound durations, fractions, UTC/offset/IANA zones and command-local DST state once, in argument order. |
| `-U`, `--until=` | timestamp | file-backed-required | Inclusive realtime upper bound. Reject `--since` later than `--until`. Parse native calendar/epoch/relative forms, checked compound durations, fractions, UTC/offset/IANA zones and command-local DST state once, in argument order. |
| `-c`, `--cursor=` | cursor | file-backed-required | Seek to the specified cursor and include that entry when it matches filters. Match stock cursor seek ordering, including ignoring the optional `x=` hash for locating candidate entries while preserving exact cursor matching where required. |
| `--after-cursor=` | cursor | file-backed-required | Seek to the specified cursor and start after that entry when it is present. Match stock cursor seek ordering, including ignoring the optional `x=` hash for locating candidate entries while preserving exact cursor matching where required. |
| `--cursor-file=` | path | file-backed-required | Read cursor from the caller-provided file if present, start after it, and atomically update the file with the final cursor. |
| `-b`, `--boot[=ID]` | optional descriptor | file-backed-required | Support no argument, `all`, numeric offsets, boot IDs, and boot ID plus offset from explicit file/directory entries. Do not use the host boot ID. |
| `--this-boot` | none | file-backed-required | Deprecated setter equivalent to `--boot=0`; last occurrence wins over other boot setters. Resolve from explicit file/directory input, not host state. |
| `-u`, `--unit=` | unit/glob | file-backed-required | Add system unit match expansion using journal fields. Support exact names and globs through indexed unique field discovery. If neither system nor user selectors add any unit, return the native no-data error. An exact absent name remains a valid zero-row selector. |
| `--user-unit=` | unit/glob | file-backed-required | Add user unit match expansion using journal fields. Support exact names and globs through indexed unique field discovery, with the same combined empty-expansion rule as `--unit`. |
| `--invocation=` | ID/offset descriptor | file-backed-required | Match explicit invocation IDs and resolve offsets from explicit file/directory input when unit context is required. |
| `-I` | none | file-backed-required | Equivalent to `--invocation=0`; require the same unit-context validation as official v262. |
| `-t`, `--identifier=` | string | file-backed-required | Add `SYSLOG_IDENTIFIER=` alternatives, with repeated values ORed. |
| `-T`, `--exclude-identifier=` | string | file-backed-required | Preserve v262 file-backed behavior. Short-family outputs (`short*` and `with-unit`) skip entries whose `SYSLOG_IDENTIFIER=` matches an excluded value; JSON, export, verbose, and cat outputs remain unchanged. |
| `-p`, `--priority=` | level/range | file-backed-required | Support numeric and named priorities plus `from..to` ranges, matching official inclusive expansion. |
| `--facility=` | list | file-backed-required | Support comma-separated numeric/named syslog facilities and `help`. |
| `-g`, `--grep=` | pattern | file-backed-required | Match the first raw `MESSAGE=` value using native PCRE2-8 byte semantics, including lookarounds, backreferences, embedded NUL and invalid UTF-8 bytes. Compile the final pattern after parser interaction checks, propagate match errors, and exit 1 with empty stderr when the show action finds no grep-selected entry. If `--lines` searches from tail and `--follow` is not set, preserve official reverse-search behavior. |
| `--case-sensitive[=BOOL]` | optional bool | file-backed-required | Affect `--grep=` only. No argument means case-sensitive. |
| `-k`, `--dmesg` | none | file-backed-required | Add `_TRANSPORT=kernel`. Do not read the kernel ring buffer or host journal. |
| path match argument | path | recognized-unsupported | Official path arguments inspect current filesystem metadata, executables, scripts, and devices. In portable mode, only `FIELD=VALUE` matches and `+` are supported; path matches must return a specific unsupported message. |
| `FIELD=VALUE` match | string | file-backed-required | Different fields are ANDed, repeated same fields are OR alternatives. |
| `+` disjunction | token | file-backed-required | Preserve official disjunction-group semantics. |

## Output Control Options

| Option | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `-o`, `--output=` | mode | file-backed-required | Support every official v262 output mode listed below. `--output=help` prints the official mode list. |
| `--output-fields=` | comma list | file-backed-required | Union repeated comma-separated projections without trimming field-name bytes; an empty list retains default/prior projection. Cat uses the first raw value of each requested field and preserves embedded NUL/newlines; invalid cat field names fail only when rendering a row. |
| `-n`, `--lines[=[+]N]` | optional int | file-backed-required | Support default, tail count, and `+N` oldest semantics. Preserve conflicts with reverse/follow. |
| `-r`, `--reverse` | none | file-backed-required | Show newest matching entries first. Conflict with follow. |
| `--show-cursor` | none | file-backed-required | Print final cursor after entries. |
| `--utc` | none | file-backed-required | Render applicable timestamps in UTC. |
| `-x`, `--catalog` | none | recognized-no-op | Recognize. Portable commands do not read host catalog databases. If no portable catalog database is configured, output entries without explanations. |
| `-W`, `--no-hostname` | none | file-backed-required | Suppress hostname in short-style output modes. |
| `--no-full` | none | file-backed-required | Disable full-width output. Short-style text output uses stock ellipsization; verbose output hides long or non-printable values as blob data. |
| `-l`, `--full` | none | file-backed-required | Hidden/legacy alias that enables full-width output. Full-width output is the stock default. |
| `-a`, `--all` | none | file-backed-required | Show all fields, including long or non-printable fields where the selected output mode supports them. Preserve stock text-display behavior for NUL-containing values and JSON binary arrays. |
| `-f`, `--follow` | none | file-backed-required | Follow explicit file/directory inputs only. Never open default host journals. |
| `--no-tail` | none | file-backed-required | With follow, show all existing matching entries before following. |
| `--truncate-newline` | none | file-backed-required | Truncate displayed message text at the first newline where official output does. |
| `-q`, `--quiet` | none | file-backed-required | Suppress informational messages, boot separator output, and the stock empty-result `-- No entries --` line where applicable. |
| `--synchronize-on-exit=` | bool | recognized-unsupported | Requires journald Varlink synchronization on signal exit. Parse and reject when true; false may be accepted as no-op. |
| `--no-pager` | none | recognized-no-op | Portable commands do not spawn a pager. |
| `-e`, `--pager-end` | none | file-backed-required | Do not spawn a pager, but preserve official default-line side effects for file-backed output: when no explicit line limit exists, show the last 1000 matching entries, or 10 with follow. |

## Output Modes

Every official v262 output mode is `file-backed-required`:

| Mode | Required portable behavior |
| --- | --- |
| `short` | Default short journal format. |
| `short-full` | Short format with full timestamp. |
| `short-iso` | Short format with ISO timestamp. |
| `short-iso-precise` | Short format with precise ISO timestamp. |
| `short-precise` | Short format with precise timestamp. |
| `short-monotonic` | Short format with monotonic timestamp. |
| `short-delta` | Short format with monotonic delta. |
| `short-unix` | Short format with Unix timestamp. |
| `verbose` | Verbose field listing. |
| `export` | Journal export format. |
| `json` | Newline-delimited JSON. |
| `json-pretty` | Pretty JSON. |
| `json-sse` | Server-sent-event JSON framing. |
| `json-seq` | JSON text sequence framing. |
| `cat` | MESSAGE-only output. |
| `with-unit` | Short output including unit information. |

## FSS Options

| Option | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `--verify-key=` | key | file-backed-required | Verify sealed journal files when combined with `--verify` or implied verification. Invalid seed/key text must fail with official-style parse errors. |
| `--interval=` | duration | recognized-no-op | Parse for `--setup-keys`; no portable behavior exists without `--setup-keys`, so it is accepted as a no-op elsewhere. |
| `--force` | none | recognized-no-op | Parse for `--setup-keys`; no portable behavior exists without `--setup-keys`, so it is accepted as a no-op elsewhere. |
| `--setup-keys` | none | recognized-unsupported | Official v262 checks `/var/log/journal`, reads host machine and boot IDs, and writes `/var/log/journal/<machine-id>/fss`; portable file-backed commands must reject it instead of mutating host journal state. Evidence: `systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492`, `src/journal/journalctl-authenticate.c:80`, `src/journal/journalctl-authenticate.c:89`, `src/journal/journalctl-authenticate.c:93`, `src/journal/journalctl-authenticate.c:97`, `src/journal/journalctl-authenticate.c:139`. |

## Commands And Actions

| Command/action | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `-h`, `--help` | none | portable-utility-required | Print help including the full official v262 option surface. |
| `--version` | none | portable-utility-required | Print portable command version, v262 CLI target and actual PCRE2 version. |
| `--introspect-cli` | none | portable-utility-required | Hidden terminal action. Print native option metadata with truthful project/version/features and requested JSON framing. |
| `--new-id128` | none | portable-utility-required | Deprecated utility action; print a new ID128 without touching host state. |
| `-N`, `--fields` | none | file-backed-required | List field names from explicit file/directory input using FIELD indexes where safe; order is unspecified. Ignore line limits for names. Reject entry/time/cursor/grep/explicit boot or invocation restrictions with the final enumeration action. |
| `-F`, `--field=` | field | file-backed-required | List unique values for a field from explicit file/directory input using FIELD DATA chains where safe, applying value-count limits. Reject entry/time/cursor/grep/explicit boot or invocation restrictions with the final enumeration action. Bare `all` boot/invocation descriptors and implicit follow boot selection do not add a restriction. |
| `--list-boots` | none | file-backed-required | List boots from explicit file/directory entries with official index, boot ID, and first/last timestamp table output. Preserve official `--lines` selection, `--reverse` ordering, and conflict with `--merge`. |
| `--list-invocations` | none | file-backed-required | Discover invocation IDs directionally through indexed unit/ID matches, retaining first-valid ID field precedence and boot-scoped global ID bounds. Oldest/newest limits select discovery direction. SOW-0122 tracks broader large-input envelope work. |
| `--list-namespaces` | none | recognized-unsupported | Requires host journal namespace discovery. |
| `--disk-usage` | none | file-backed-required | With explicit file/directory input, report disk usage for selected journal files. Without explicit input, return unsupported host journal discovery. |
| `--vacuum-size=` | bytes | file-backed-maintenance-required | With explicit `--directory=`, scan that directory's direct regular files, delete recognized archived `.journal`/`.journal~` files until active plus archived journal allocated usage is under the requested size, and protect active/current, non-matching, unknown, and subdirectory files. Without explicit directory, return unsupported. |
| `--vacuum-files=` | int | file-backed-maintenance-required | With explicit `--directory=`, delete recognized archived `.journal`/`.journal~` files until protected active/current files plus remaining archived candidates is at or below the requested count. Without explicit directory, return unsupported. |
| `--vacuum-time=` | duration | file-backed-maintenance-required | With explicit `--directory=`, delete recognized archived `.journal`/`.journal~` files older than the requested age while protecting active/current, non-matching, unknown, and subdirectory files. Without explicit directory, return unsupported. |
| `--verify` | none | file-backed-required | Verify explicit file/directory inputs, including sealed files with `--verify-key`. |
| `--sync` | none | recognized-unsupported | Daemon-only journal synchronization. |
| `--relinquish-var` | none | recognized-unsupported | Daemon-only journald storage transition. |
| `--smart-relinquish-var` | none | recognized-unsupported | Daemon-only journald storage transition plus host mount inspection. |
| `--flush` | none | recognized-unsupported | Daemon-only runtime-to-persistent flush. |
| `--rotate` | none | recognized-unsupported | Daemon-only journald rotation request. `--rotate` plus vacuum options must be recognized as official `ACTION_ROTATE_AND_VACUUM`, then rejected because rotation is daemon-only. |
| `--header` | none | file-backed-required | Print header information for explicit file/directory input. |
| `--list-catalog` | none | recognized-unsupported | Host catalog database action, not journal file reading. |
| `--dump-catalog` | none | recognized-unsupported | Host catalog database action, not journal file reading. |
| `--update-catalog` | none | recognized-unsupported | Host catalog database mutation. |

## Parser Interaction Requirements

| Interaction | Classification | Required portable behavior |
| --- | --- | --- |
| Source exclusivity | parser-required | Reject more than one of `--directory=`, `--file=`, `--machine=`, `--root=`, and `--image=`. |
| Time bounds | parser-required | Reject `--since=` later than `--until=`. |
| Cursor source exclusivity | parser-required | Reject more than one of `--since=`, `--cursor=`, `--cursor-file=`, and `--after-cursor=`. |
| Follow/reverse conflict | parser-required | Reject `--follow` with `--reverse`. |
| Oldest-lines conflict | parser-required | Reject `--lines=+N` with `--reverse` or `--follow` for the show action. Preserve official action-specific behavior for non-show actions. |
| Action argument restriction | parser-required | For actions other than show, list catalog, and dump catalog, reject extraneous arguments. |
| Boot/merge conflict | parser-required | Reject specific `--boot` selectors or `--list-boots` with `--merge`; allow `--boot=all` with `--merge`. |
| Option interspersing | parser-required | Accept recognized options before or after show-action `FIELD=value`, `+`, or path-match positional arguments, matching stock argument permutation. |
| Explicit empty boot descriptor | parser-required | Treat bare `--boot` as the current boot, but reject explicit `--boot=` as a failed empty boot descriptor like stock. |
| Explicit empty optional values | parser-required | Reject explicit empty values where stock rejects them, including `--lines=` and `--case-sensitive=`. Bare `--lines` and bare `--case-sensitive` remain valid. |
| Short attached values and clusters | parser-required | Accept stock short-option attached values and clusters such as `-n2`, `-ball`, and `-rn2`; preserve stock failure behavior for forms such as `-n=2` and `-b=true`. |
| User plus unit rewrite | parser-required | With `--user --unit=` and no `--system`, treat the unit as `--user-unit=` before adding filters. |
| Grep reverse implication | parser-required | With `--grep` and tail-searching `--lines`, set reverse search unless follow is set. |
| Default host journal | parser-required | If no explicit `--file=` or `--directory=` is supplied, only portable utility commands may run. File/query actions must return unsupported default host journal behavior. |

## Implementation Notes

- The parser should be table-driven or generated from a shared manifest where
  practical so Rust and Go cannot silently drift.
- Parser tests must include every option in this matrix and must fail if Rust or
  Go returns an unknown-option error for an official v262 option.
- Behavior tests must use repository-local fixtures only. They must not read
  live host journals or write under host journal directories.
- Unsupported behavior tests must verify message class, exit code, and absence
  of host journal or systemd service interaction.

Ordered setters validate every occurrence; terminal help/version/introspection
returns immediately when encountered. Actions select the last requested action,
with the native rotate/vacuum combination. Superseded action parameters do not
force dispatch. Output JSON/export/cat makes quiet sticky; full/no-full follows
encounter order. Boot and invocation aliases share their underlying setter.
Cursor-relative forward limits begin at the cursor. Follow streams all history
selected by a resolved cursor even with a numeric line limit; an empty or missing
cursor file retains default tail behavior. Since-only follow and explicit all or
no-tail include the full existing matching backlog. Boot/invocation tables support
all four JSON framings with integer microsecond bounds.

Unix input filenames retain native bytes through globbing, directory discovery
and the SDK facade. File patterns use platform BRACE/NOCHECK/NOSORT expansion
in the environment locale; literal paths bypass expansion. Windows uses
Unicode filenames, brace alternatives and Unicode wildcard matching. Header
path output and vacuum filename ordering preserve native Unix bytes.

Unit names use native glob/suffix, byte-escape and pure path-conversion rules,
including hashed long paths. Unit wildcard matches retain raw FIELD/DATA values.
Unix uses fnmatch NOESCAPE, with a FreeBSD compatibility path for extended
brackets missing from libc; Windows uses the C-locale byte grammar. Follow keeps
the initial wildcard expansion and resolved invocation ID. Invocation discovery
uses base unit fields/groups, first-valid ID precedence, native directional
indexed traversal and boot-scoped global ID bounds. Explicit ID selection may
match secondary fields. Identifier/priority/facility/transport filters remain
active when invocation selection replaces unit/boot show matches.

Rendering follows SYSTEMD_COLORS, NO_COLOR, TERM, COLORTERM and both stdout and
stderr terminal state. Fixed modes and explicit true override NO_COLOR;
automatic modes respect it. Include priority/audit colors, first-match grep
offsets, verbose fields/cursors, JSON primitive styles, boot separators and
table underlining. TERM=linux disables underlining. Embedded DOCUMENTATION and
CONFIG_FILE links follow SYSTEMD_URLIFY. Windows drive and UNC file links use
slash-separated, percent-encoded file URIs; unsupported device paths remain
plain text. Unix link formatting is unchanged. Host catalog lookup, including implicit
documentation-link lookup, remains outside portable mode. Preserve plain
rendering paths and avoid a second regex evaluation for highlighting.

## Approved Vacuum Ordering Exception (2026-10-24)

Both CLIs order each sequenced archive history by sequence number and filename,
then merge available history heads by adjusted realtime, sequenced-before-
unsequenced, sequence ID and filename. Each archive without sequence metadata
is an independent history. This replaces the nontransitive stock comparison
when clock rollback causes conflicting sequence/time order; incidental libc
survivor sets are not the contract for that case. Count/size/age stop conditions,
empty cleanup and protected files otherwise retain their existing semantics.
The history sort and lazy heap merge cost O(n log n + k log h), with O(h)
auxiliary ranges and no added journal payload reads or metadata syscalls.
