# journalctl v260.1 Portable Parity Matrix

## Scope

This matrix defines the Rust and Go portable `journalctl` target for
SOW-0121.

Baseline authority:

- `systemd/systemd @ c0a5a2516d28` (`v260.1`)
- `src/journal/journalctl.c:241` help surface
- `src/journal/journalctl.c:420` option table
- `src/journal/journalctl.c:520` parser switch
- `src/journal/journalctl.c:1085` parser interaction checks
- `src/journal/journalctl.h:7` action enum
- `src/shared/output-mode.c:26` output mode names
- `src/journal/journalctl-filter.c:17` file-backed filter construction
- `src/journal/journalctl-show.c:51` seek, cursor, follow, and output loop

The portable Rust and Go commands must recognize 100% of the official v260.1
command-line option and action surface. Unknown-option errors for official
v260.1 options are compatibility failures.

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
- `parser-required`: parser behavior that must match official v260.1 before any
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
| `-m`, `--merge` | none | recognized-no-op | Recognize. Interleaving multiple explicit file/directory inputs is already the portable directory/file behavior. Preserve official conflicts with `--boot` and `--list-boots`. |
| `-D`, `--directory=` | path | file-backed-required | Open journal files from the supplied directory, including supported repository `.journal.zst` directory files. |
| `-i`, `--file=` | glob/path | file-backed-required | Open one or more explicit journal files. Multiple `--file=` occurrences and glob expansion must be supported. `--file=-` must be recognized; support only if a seekable stdin-backed implementation exists, otherwise return a specific unsupported message. |
| `--root=` | path | recognized-unsupported | Requires alternate root filesystem discovery and catalog hierarchy access. Do not inspect host rootfs. |
| `--image=` | path | recognized-unsupported | Requires disk image dissection/mounting. Do not mount or inspect images. |
| `--image-policy=` | policy | recognized-unsupported | Only meaningful with `--image=`; parse and reject with the same portable image unsupported reason. |
| `--namespace=` | string | recognized-unsupported | Requires systemd journal namespaces. Do not discover or open host namespaces. |

## Filtering Options

| Option | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `-S`, `--since=` | timestamp | file-backed-required | Inclusive realtime lower bound. Match official timestamp grammar where practical. |
| `-U`, `--until=` | timestamp | file-backed-required | Inclusive realtime upper bound. Reject `--since` later than `--until`. |
| `-c`, `--cursor=` | cursor | file-backed-required | Seek to the specified cursor and include that entry when it matches filters. |
| `--after-cursor=` | cursor | file-backed-required | Seek to the specified cursor and start after that entry when it is present. |
| `--cursor-file=` | path | file-backed-required | Read cursor from the caller-provided file if present, start after it, and atomically update the file with the final cursor. |
| `-b`, `--boot[=ID]` | optional descriptor | file-backed-required | Support no argument, `all`, numeric offsets, boot IDs, and boot ID plus offset from explicit file/directory entries. Do not use the host boot ID. |
| `--this-boot` | none | file-backed-required | Deprecated alias for the current `--boot` selection. Resolve from explicit file/directory input, not host state. |
| `-u`, `--unit=` | unit/glob | file-backed-required | Add system unit match expansion using journal fields. Support exact names and globs through indexed unique field discovery. |
| `--user-unit=` | unit/glob | file-backed-required | Add user unit match expansion using journal fields. Support exact names and globs through indexed unique field discovery. |
| `--invocation=` | ID/offset descriptor | file-backed-required | Match explicit invocation IDs and resolve offsets from explicit file/directory input when unit context is required. |
| `-I` | none | file-backed-required | Equivalent to `--invocation=0`; require the same unit-context validation as official v260.1. |
| `-t`, `--identifier=` | string | file-backed-required | Add `SYSLOG_IDENTIFIER=` alternatives, with repeated values ORed. |
| `-T`, `--exclude-identifier=` | string | file-backed-required | Exclude matching `SYSLOG_IDENTIFIER=` values. |
| `-p`, `--priority=` | level/range | file-backed-required | Support numeric and named priorities plus `from..to` ranges, matching official inclusive expansion. |
| `--facility=` | list | file-backed-required | Support comma-separated numeric/named syslog facilities and `help`. |
| `-g`, `--grep=` | pattern | file-backed-required | Filter `MESSAGE=` with compatible regular expression behavior. If `--lines` searches from tail and `--follow` is not set, preserve official reverse-search behavior. |
| `--case-sensitive[=BOOL]` | optional bool | file-backed-required | Affect `--grep=` only. No argument means case-sensitive. |
| `-k`, `--dmesg` | none | file-backed-required | Add `_TRANSPORT=kernel`. Do not read the kernel ring buffer or host journal. |
| path match argument | path | recognized-unsupported | Official path arguments inspect current filesystem metadata, executables, scripts, and devices. In portable mode, only `FIELD=VALUE` matches and `+` are supported; path matches must return a specific unsupported message. |
| `FIELD=VALUE` match | string | file-backed-required | Different fields are ANDed, repeated same fields are OR alternatives. |
| `+` disjunction | token | file-backed-required | Preserve official disjunction-group semantics. |

## Output Control Options

| Option | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `-o`, `--output=` | mode | file-backed-required | Support every official v260.1 output mode listed below. `--output=help` prints the official mode list. |
| `--output-fields=` | comma list | file-backed-required | Restrict verbose/export/json fields. |
| `-n`, `--lines[=[+]N]` | optional int | file-backed-required | Support default, tail count, and `+N` oldest semantics. Preserve conflicts with reverse/follow. |
| `-r`, `--reverse` | none | file-backed-required | Show newest matching entries first. Conflict with follow. |
| `--show-cursor` | none | file-backed-required | Print final cursor after entries. |
| `--utc` | none | file-backed-required | Render applicable timestamps in UTC. |
| `-x`, `--catalog` | none | recognized-no-op | Recognize. Portable commands do not read host catalog databases. If no portable catalog database is configured, output entries without explanations. |
| `-W`, `--no-hostname` | none | file-backed-required | Suppress hostname in short-style output modes. |
| `--no-full` | none | file-backed-required | Disable full-width output. |
| `-l`, `--full` | none | file-backed-required | Hidden/legacy alias that enables full-width output. |
| `-a`, `--all` | none | file-backed-required | Show all fields, including long or non-printable fields where the selected output mode supports them. |
| `-f`, `--follow` | none | file-backed-required | Follow explicit file/directory inputs only. Never open default host journals. |
| `--no-tail` | none | file-backed-required | With follow, show all existing matching entries before following. |
| `--truncate-newline` | none | file-backed-required | Truncate displayed message text at the first newline where official output does. |
| `-q`, `--quiet` | none | file-backed-required | Suppress informational messages and boot separator output where applicable. |
| `--synchronize-on-exit=` | bool | recognized-unsupported | Requires journald Varlink synchronization on signal exit. Parse and reject when true; false may be accepted as no-op. |
| `--no-pager` | none | recognized-no-op | Portable commands do not spawn a pager. |
| `-e`, `--pager-end` | none | file-backed-required | Do not spawn a pager, but preserve official default-line side effects for file-backed output. |

## Output Modes

Every official v260.1 output mode is `file-backed-required`:

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
| `--interval=` | duration | portable-utility-required | Parse for `--setup-keys`. Reject outside `--setup-keys` with the same action/option rules as official v260.1. |
| `--force` | none | portable-utility-required | Parse for `--setup-keys`; no effect elsewhere except official option interactions. |
| `--setup-keys` | none | portable-utility-required | Generate an FSS key pair without touching host journal state. If implementation cannot produce systemd-compatible keys in both languages, recognize and fail with a specific unsupported FSS setup message until completed. |

## Commands And Actions

| Command/action | Args | Classification | Required portable behavior |
| --- | --- | --- | --- |
| `-h`, `--help` | none | portable-utility-required | Print help including the full official v260.1 option surface. |
| `--version` | none | portable-utility-required | Print portable command version and compatibility baseline. |
| `--new-id128` | none | portable-utility-required | Deprecated utility action; print a new ID128 without touching host state. |
| `-N`, `--fields` | none | file-backed-required | List field names from explicit file/directory input using FIELD indexes where safe. |
| `-F`, `--field=` | field | file-backed-required | List unique values for a field from explicit file/directory input using FIELD DATA chains where safe. |
| `--list-boots` | none | file-backed-required | List boots from explicit file/directory entries. Preserve official conflict with `--merge`. |
| `--list-invocations` | none | file-backed-required | List invocation IDs from explicit file/directory entries for the selected unit context. |
| `--list-namespaces` | none | recognized-unsupported | Requires host journal namespace discovery. |
| `--disk-usage` | none | file-backed-required | With explicit file/directory input, report disk usage for selected journal files. Without explicit input, return unsupported host journal discovery. |
| `--vacuum-size=` | bytes | file-backed-maintenance-required | With explicit `--directory=`, remove archived journal files until under the requested size while protecting active/current files. Without explicit directory, return unsupported. |
| `--vacuum-files=` | int | file-backed-maintenance-required | With explicit `--directory=`, retain the requested number of archived journal files while protecting active/current files. Without explicit directory, return unsupported. |
| `--vacuum-time=` | duration | file-backed-maintenance-required | With explicit `--directory=`, remove archived journal files older than the requested age while protecting active/current files. Without explicit directory, return unsupported. |
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
| Oldest-lines conflict | parser-required | Reject `--lines=+N` with `--reverse` or `--follow`. |
| Action argument restriction | parser-required | For actions other than show, list catalog, and dump catalog, reject extraneous arguments. |
| Boot/merge conflict | parser-required | Reject `--boot` or `--list-boots` with `--merge`. |
| User plus unit rewrite | parser-required | With `--user --unit=`, treat the unit as `--user-unit=` before adding filters. |
| Grep reverse implication | parser-required | With `--grep` and tail-searching `--lines`, set reverse search unless follow is set. |
| Default host journal | parser-required | If no explicit `--file=` or `--directory=` is supplied, only portable utility commands may run. File/query actions must return unsupported default host journal behavior. |

## Implementation Notes

- The parser should be table-driven or generated from a shared manifest where
  practical so Rust and Go cannot silently drift.
- Parser tests must include every option in this matrix and must fail if Rust or
  Go returns an unknown-option error for an official v260.1 option.
- Behavior tests must use repository-local fixtures only. They must not read
  live host journals or write under host journal directories.
- Unsupported behavior tests must verify message class, exit code, and absence
  of host journal or systemd service interaction.
