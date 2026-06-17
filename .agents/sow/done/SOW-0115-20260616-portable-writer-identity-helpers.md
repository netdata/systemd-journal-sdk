# SOW-0115 - Portable writer identity & monotonic helpers (machine-id, boot-id, monotonic clock)

## Status

Status: completed

Sub-state: completed - strict Rust/Go writer contract, optional host helper packages, docs/spec updates, validation, reviewer rounds, and follow-up mapping are complete. Completed-implementation review round 1 returned 5/6 READY TO IMPLEMENT and 1/6 NOT READY; round 2 returned 4/5 completed READY TO IMPLEMENT, 1/5 completed NOT READY, and one qwen transport failure; round 3 returned 5/5 completed READY TO IMPLEMENT and one minimax timeout whose partial transcript had no blocker and stated it would vote READY. Real findings were fixed or explicitly dispositioned and revalidated.

## Requirements

### Purpose

Let the SDK's writing side make the three journal identity/time anchors explicit and portable: `_MACHINE_ID`, `_BOOT_ID`, and a boot-anchored `__MONOTONIC_TIMESTAMP`. The core writer remains OS-agnostic and must not guess these values; callers must provide them. A separate optional helper API/package will produce local-host values on Linux, FreeBSD, macOS, and Windows for callers that intentionally want collector-host identity, while callers such as SNMP traps or NetFlow may instead provide remote-device, synthetic, fixed, or auto-incremented values when that better matches the event source.

### User Request

"Create an SOW to provide portable equivalents for those 3 variables, as part of the writer API."

Context that triggered this: Netdata's `snmp_traps` collector hard-gates its direct-journal backend to Linux (`collector.go:162-163`, "SNMP trap journal backend requires Linux") because it sources the three anchors from Linux-only paths: `/etc/machine-id`, `/proc/sys/kernel/random/boot_id`, and `CLOCK_MONOTONIC` (with a non-boot-anchored fallback on other OSes). The Go SDK writes the journal *file format* on all four platforms, but its core writer deliberately does not discover these values - `go/journal/boot_id_linux.go` and `boot_id_other.go` are empty marker files whose only content states host-identity discovery is intentionally kept out of the core writer. Review found that Rust already has partial identity/time helpers; those are recorded below as existing surface to reconcile, not as a complete cross-language contract.

### Assistant Understanding

Facts:

- The journal entry format requires three caller-supplied anchors today. In the Go binding they arrive via `Options.MachineID`, `Options.BootID` (`go/journal/writer_init.go:13` checks `isZeroUUID(opts.MachineID)`) and per-entry `EntryOptions.MonotonicUsec` / `RealtimeUsec`.
- The SDK's Runtime Purity Architecture (`AGENTS.md:75-117`) defines four layers and explicitly reserves host-identity discovery for **Layer 3, the "Optional identity helper service"**, opt-in and *outside* the core file-format contract. The same contract is codified in the product spec (`.agents/sow/specs/product-scope.md:62-80`): core readers/writers must not probe host identity, read `/etc/machine-id`, registries, `sysctl`, `system_profiler`, `ps`, shell, or subprocess APIs - those are allowed only inside explicitly named optional helper code and its tests.
- The SDK already ships a per-platform, build-tagged, native-syscall precedent in its **Layer 4 writer-lock helper**: `currentBootID()` in `go/journal/lock_owner_{linux,bsd,windows,unix_other,other}.go`. This proves the file-layout/native-API pattern but its **values are not journal-grade**: Linux reads the real boot_id, but BSD returns `"bsd:"+hex(kern.boottime)`, Windows returns the literal `"windows"`, other-unix returns `"unix"`, fallback returns `"unknown"` - none are valid 128-bit UUIDs and Windows cannot even detect a reboot. They are lock-staleness tokens, not identities.
- Follow-up research on 2026-06-17 corrected the original non-Linux boot-id model. Linux has `/proc/sys/kernel/random/boot_id`; FreeBSD 13+ has native `kern.boot_id`; macOS has native read-only `kern.bootsessionuuid` in XNU, but it is not a documented public Apple API; Windows has no documented unprivileged runtime boot-session UUID. Therefore state-backed synthesis is required for Windows and only a fallback for FreeBSD versions/environments where `kern.boot_id` is unavailable.
- The SDK product scope is Rust and Go after SOW-0116. Historical Python/Node parity work no longer creates implementation requirements for this SOW.
- Review-discovered pre-implementation fact: Rust exposed partial host identity helpers in `rust/src/crates/journal-common/src/system.rs` (`load_machine_id`, `load_boot_id`) and a monotonic helper in `rust/src/crates/journal-common/src/time.rs` (`monotonic_now`). This corrected the original broad wording about SDK helper absence. The old identity helpers were partial and not aligned with this SOW's intended final contract: Windows identity returned unsupported, macOS/FreeBSD boot-id was stateless `kern.boottime` synthesis, and Unix monotonic used `CLOCK_MONOTONIC` on all Unix targets. Implementation removed the `journal-common::system` host-identity module and moved local-host discovery into the separate `journal_host` crate.
- Review-discovered in-tree fact: both product writer families already generate a default monotonic value when the caller omits one. Go uses writer-start-relative `time.Now().Sub(w.started)` in `go/journal/writer.go`; Rust uses `journal_common::monotonic_now()` plus clamp in `rust/src/crates/journal-log-writer/src/log/mod.rs`. The opt-in helper must reconcile with these existing defaults instead of pretending monotonic generation is greenfield.

Inferences:

- "As part of the writer API" means the SDK exposes an adjacent public helper API/package for callers that want local-host values, while the core writer itself becomes strict about the three anchors. The writer remains a pure file-format writer: it accepts caller-provided machine-id, boot-id, and monotonic timestamp values, and it must not silently generate local-host identity or fallback monotonic values.
- A boot-anchored monotonic clock source is available on all four platforms, but the correct source is OS-specific: Linux uses `CLOCK_MONOTONIC`; FreeBSD should prefer `CLOCK_UPTIME` for Linux-like suspend-excluding semantics; macOS should prefer `CLOCK_UPTIME_RAW`/equivalent; Windows should prefer unbiased interrupt time. Per-entry values must not be derived from wall-clock `now - boot_time`; wall-clock jumps would damage accuracy even if ordering is later clamped.

Remaining implementation proofs:

- The user-facing design decisions exposed by repeat review are resolved by Decisions 8A-prime, 9A, and 10A. Local implementation proofs are complete for Linux runtime, deterministic state-backed behavior, strict writer failures, Go non-Linux compile coverage, Rust Windows compile coverage, docs examples, runtime purity, whitespace, and SOW audit. Completed-implementation review round 1 found fixable issues and one false Windows API claim; fixes are applied and revalidated. Remaining gaps are explicit runtime-environment gaps: no native FreeBSD/macOS/Windows runtime smoke was run in this repository environment, and Rust FreeBSD/macOS target compile checks were not run because those targets are not installed.

### Acceptance Criteria

- The core writer APIs in Rust and Go require explicit caller-provided values for `_MACHINE_ID`, `_BOOT_ID`, and generated-entry `__MONOTONIC_TIMESTAMP`; they no longer silently synthesize or fallback for those three anchors. Verified by API tests that missing values fail fast with documented errors.
- A separate optional helper API/package returns a valid 128-bit `_MACHINE_ID`, `_BOOT_ID`, and boot-anchored monotonic timestamp source for local-host events on Linux, FreeBSD, macOS, and Windows, using native APIs only (no subprocess). Verified by per-platform unit tests and (where CI runners exist) real-host smoke tests.
- Within a single OS boot, repeated helper invocations and independent processes return a byte-identical `_BOOT_ID` on native and healthy state-backed paths; across a simulated reboot the value changes. Degraded state/discovery failure paths still return a valid boot ID but explicitly do not claim cross-process same-boot stability. Verified by deterministic tests that inject the boot-marker source and failure modes.
- Core writer/reader paths still compile and pass with the helper package absent; core code does not import host identity discovery. Verified by the purity tests/grep already used by the project and by `.agents/sow/audit.sh`.
- A journal file written on a non-Linux platform using explicit caller anchors, including anchors produced by the helper, is accepted and correctly ordered by the SDK reader and (where available) by stock `journalctl --directory`. Verified against the conformance/live harness.
- The chosen consumers (`snmp_traps` Go and NetFlow Rust) can choose the correct identity source for their domain: local-host helper values, remote-device values, synthetic values, fixed values, or auto-incremented values where the journal contract permits. Downstream Netdata gate removal remains tracked by a separate netdata-repo SOW.

## Analysis

Sources checked:

- `AGENTS.md:75-117` (Runtime Purity Architecture, four layers, forbidden core inputs).
- `.agents/sow/specs/product-scope.md:62-106` (core vs optional helper contract; FreeBSD/macOS monotonic-timestamp expectations).
- `go/journal/writer_init.go`, `go/journal/writer.go`, reader/`directory_reader.go` (where MachineID/BootID/Monotonic enter and how boot_id is used for ordering).
- `rust/src/crates/journal-common/src/system.rs` and `rust/src/crates/journal-common/src/time.rs` (existing Rust identity/time helpers).
- `rust/src/crates/journal-log-writer/src/log/mod.rs` and `go/journal/log.go` (current generated monotonic paths and clamping behavior).
- `go/journal/lock_owner_{linux,bsd,windows,unix_other,other}.go` @ `a2361ab` (existing per-platform `currentBootID()` precedent and why it is not journal-grade).
- `go/journal/boot_id_linux.go`, `boot_id_other.go` @ `a2361ab` (empty marker files documenting the deliberate exclusion).
- Downstream driver: `netdata snmp_traps` `collector.go:162-163`, `journal_writer.go:90,94`, `monotonic_linux.go`, `monotonic_fallback.go`.
- Official platform documentation: `systemd.journal-fields(7)`, Linux `clock_gettime(3)`/`machine-id(5)`/`sd_id128_get_boot(3)`, FreeBSD `clock_gettime(2)`/`sysctl(8)` host UUID behavior, macOS `clock_gettime(3)`/`gethostuuid(2)`, and Microsoft `QueryUnbiasedInterruptTime`, WMI boot-time, registry, and machine UUID documentation.
- Primary follow-up sources checked on 2026-06-17:
  - `systemd/systemd v260.1`: `man/systemd.journal-fields.xml` defines `__MONOTONIC_TIMESTAMP` as `CLOCK_MONOTONIC`; `src/basic/time-util.c` populates monotonic timestamps from `now(CLOCK_MONOTONIC)`.
  - `freebsd/freebsd-src main`, `releng/13.0`, `releng/14.0`: `sys/kern/kern_mib.c` defines `kern.boot_id` as a read-only 16-byte random boot ID generated once per boot; `stable/12` does not have this sysctl.
  - `freebsd/freebsd-src main`: `lib/libsys/clock_gettime.2` says `CLOCK_MONOTONIC` / `CLOCK_BOOTTIME` increment while suspended, while `CLOCK_UPTIME` increments only while the machine is running.
  - `apple-oss-distributions/xnu main`: `bsd/kern/kern_sysctl.c` defines read-only `kern.bootsessionuuid`; this is native but undocumented as a public Apple API.
  - Microsoft Learn: `QueryUnbiasedInterruptTime` excludes sleep/hibernation and starts at zero when the system starts; `GetTickCount64` is lower resolution and Microsoft points callers to `QueryUnbiasedInterruptTime` for working-state time.
- Reference implementations for native per-platform sources (read-only mirror):
  - `shirou/gopsutil @ df9e25f20` - `host/host_linux.go`, `host/host_freebsd.go`, `host/host_darwin.go`, `host/host_windows.go` (HostID + BootTime per OS).
  - `elastic/go-sysinfo @ efea16f` - `providers/{linux,darwin,windows}/machineid*.go` (machine-id per OS).
  - `osquery/osquery @ 225804016439` - `osquery/core/system.cpp`, `osquery/utils/system/uptime.cpp` (hardware UUID and per-OS uptime references).
  - `DataDog/datadog-agent @ 67ac6ce55e68` - `pkg/ebpf/time.go`, `pkg/network/time_converter_windows.go`, `pkg/util/uuid/uuid_windows.go` (Linux monotonic, Windows uptime conversion, Windows MachineGuid).
  - `open-telemetry/opentelemetry-go @ 6e2b9214f05e` - `sdk/resource/host_id*.go` (host-id sources; BSD/macOS subprocess fallback is useful negative evidence because this SDK must stay native-only).

Verified native per-platform sources (no subprocess):

| Platform | `_MACHINE_ID` (stable host UUID) | `_BOOT_ID` (per-boot UUID) | Boot-anchored monotonic default |
|---|---|---|---|
| Linux | `/etc/machine-id` -> fallback `/var/lib/dbus/machine-id` | **native** `/proc/sys/kernel/random/boot_id` | `clock_gettime(CLOCK_MONOTONIC)` |
| FreeBSD | `kern.hostuuid` sysctl (validate and reject all-zero jail sentinel) | **native** `kern.boot_id` on FreeBSD 13+; state-backed fallback only when unavailable | `clock_gettime(CLOCK_UPTIME)`; `_FAST` only as explicit approximate mode if accepted by evidence |
| macOS | documented `gethostuuid()` C API, or equivalent native platform UUID API | **native** `kern.bootsessionuuid` via sysctl; undocumented/public-contract risk accepted by Decision 8A-prime | `CLOCK_UPTIME_RAW` / native uptime raw API; `_APPROX` only as explicit approximate mode if accepted by evidence |
| Windows | registry `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` or documented SMBIOS UUID fallback | **state-backed synthesis**; no documented unprivileged runtime boot-session UUID found | `QueryUnbiasedInterruptTime`; `QueryUnbiasedInterruptTimePrecise` only when precision beats call cost |

Boot-id synthesis (the central correctness point): the helper must return a 128-bit boot ID that is stable across independent helper invocations during one OS boot and changes after reboot. Native boot IDs are preferred wherever the OS provides one because they avoid state and cross-process coordination. State-backed synthesis is required for Windows and remains a fallback for FreeBSD 12 or environments where `kern.boot_id` is unavailable. macOS uses native `kern.bootsessionuuid` with the accepted undocumented-API risk; if it is unavailable, the helper falls back according to Decision 10. Per-entry monotonic timestamps must not be derived from wall-clock `now - boot_time`; they must use a native monotonic/uptime source plus helper-owned clamping.

Official clock-semantics verification after external review:

- Linux `CLOCK_MONOTONIC` is not affected by wall-clock jumps, is frequency-adjusted, and does **not** count suspended time; Linux `CLOCK_BOOTTIME` is the suspend-inclusive variant.
- FreeBSD `CLOCK_MONOTONIC` / `CLOCK_BOOTTIME` increment even while suspended; FreeBSD `CLOCK_UPTIME` increments while the machine is running. `kern.boottime` is recomputed when system time is set, stepped by `ntpd`, or read from RTC on resume.
- macOS `CLOCK_MONOTONIC` continues while the system is asleep; `CLOCK_UPTIME_RAW` does not increment while asleep and matches `mach_absolute_time()` after timebase conversion.
- Windows `QueryUnbiasedInterruptTime` reflects only working-state time and does not include sleep or hibernation.

Reviewer note: some external reviewers returned conflicting claims about Linux/macOS suspend semantics. The bullets above are the accepted facts for this SOW because they match the official/manual-page sources checked after review.

### Concrete Contract After Decision 6A

Strict writer contract:

- Core writers remain file-format writers. They do not discover host identity, read host identity files, call OS identity APIs, or synthesize identity/time anchors.
- New journal creation requires explicit `machine_id` and default `boot_id` from the caller. SDK-generated random `machine_id` or `boot_id` defaults are removed from the core writer and high-level log writer defaults.
- Per-entry append requires an explicit `monotonic_usec`. A zero monotonic value is valid only when the language already has an explicit "set" marker, such as Go `MonotonicUsecSet=true`; otherwise missing and zero must be distinguishable through the idiomatic language API before implementation is accepted.
- The entry header boot-id is the explicit default boot-id supplied at writer/log construction unless the caller supplies an explicit per-entry boot-id override. Opening an existing non-empty file may continue from the file's on-disk tail boot-id because that value is file state, not host discovery. Opening an empty file or a file with no tail boot-id must require an explicit boot-id before the first append.
- `file_id`, `seqnum_id`, and generated sequence numbers may remain SDK-local defaults because they are journal-file identifiers, not the three event identity/time anchors discussed in this SOW.
- Realtime commit timestamp fallback is out of scope for this SOW. Existing realtime defaults may remain, but docs must distinguish commit realtime from source realtime and tell callers to pass explicit realtime/source-realtime when event-source time matters.
- Low-level raw/structured file writers continue to write caller-provided payloads and ENTRY header metadata. They do not auto-inject journald fields into caller payloads. The journald/log layer injects `_BOOT_ID` from the explicit default or per-entry boot-id.

Optional helper contract:

- The helper is a separate public API/package, not an automatic writer wrapper. Its purpose is to return local-host values for callers that intentionally want the collector host as the event identity source.
- The helper returns two stable identity anchors and one stateful clock/provider:
  - `machine_id`: 16 bytes / UUID.
  - `boot_id`: 16 bytes / UUID, stable for one OS boot and changed after reboot according to the accepted platform strategy.
  - `monotonic_usec()`: fast per-entry boot-anchored microseconds, clamped to strictly increase within one helper/provider instance.
- The helper must expose an idiomatic convenience that produces the language's append options, but the caller still passes those options explicitly to the writer.
- The helper must also expose the raw values so callers can combine local-host identity with their own timestamp policy or choose a non-local identity source.
- The helper must expose diagnostics/source metadata so callers can distinguish native values from degraded synthesized values without making boot-id state failures fatal.
- Helper tests may probe the real host only inside explicitly named optional-helper tests. Core writer tests continue to use synthetic identities.

Reference API shape to implement:

| Language | Strict writer change | Helper package/module | Required helper shape |
|---|---|---|---|
| Go | `journal.Create` requires non-zero `Options.MachineID` and `Options.BootID`; `Writer.Append`/`AppendRaw` require `EntryOptions.MonotonicUsecSet` or a non-zero `MonotonicUsec`; `LogIdentityAuto` and writer-start-relative monotonic fallback are removed from the default writer/log path. | New sibling package `github.com/netdata/systemd-journal-sdk/go/journalhost` so core `journal` does not import host probing. | `journalhost.Load(opts) (Provider, error)`; `Provider.MachineID() journal.UUID`; `Provider.BootID() journal.UUID`; `Provider.EntryOptions() journal.EntryOptions`; `Provider.MonotonicUsec() uint64`; `Provider.Diagnostics()`/equivalent source metadata; state options for Windows and FreeBSD fallback. |
| Rust | `journal-core` remains explicit: `JournalFileOptions::new(machine_id, boot_id, seqnum_id)` and append APIs already take monotonic values. `journal-log-writer` changes default identity mode to strict and stops generating SDK-local machine/boot IDs or monotonic timestamps silently. | New workspace crate `systemd-journal-sdk-host` with lib name `journal_host`; the old `journal-common::system::{load_machine_id, load_boot_id}` host-identity module is removed from the core-imported path. | `LocalJournalProvider::load(options) -> Result<Self>`; `machine_id() -> Uuid`; `boot_id() -> Uuid`; `entry_timestamps() -> EntryTimestamps`; `monotonic_usec() -> u64`; diagnostics/source metadata; state options for Windows and FreeBSD fallback. |

State-file contract for Windows and FreeBSD fallback:

- Scope: primary Windows boot-id synthesis and FreeBSD fallback only when native `kern.boot_id` is unavailable. Linux never uses helper state; FreeBSD 13+ does not use helper state on the normal path; macOS does not use helper state on the normal path unless native `kern.bootsessionuuid` is unavailable and implementation chooses the degraded fallback path.
- Default state data path:
  - FreeBSD fallback: `/var/run/systemd-journal-sdk/bootid.$UID.state`, where `$UID` is the numeric effective UID.
  - Windows: a per-user or service-user SDK path under the OS-local application-data area, with caller override. Packaged consumers that need machine-wide or cross-user sharing must provision the directory and pass an explicit path.
- State data file contains exactly two logical fields as ASCII lines:
  - `last_estimated_boottime=<decimal unix microseconds>`
  - `last_boot_id=<32 lowercase hex UUID bytes>`
- The helper must create new Unix state files with `0600` permissions unless the caller points to an already-provisioned path. Windows state files use the current user's default ACL unless the caller provisions a stricter directory.
- Locking must serialize helper initialization across processes before the file is read or written. Use a sidecar lock file when needed for safe atomic replacement. Go uses `flock` on FreeBSD and `LockFileEx` on Windows. Rust uses `rustix`/`libc` locking on FreeBSD and `windows-sys`/Win32 locking on Windows. If a platform primitive is unavailable, implementation must record the exact equivalent before review.
- Write policy:
  - Missing state: generate a new UUID, write current estimated boot time and boot-id.
  - Corrupt/unparseable state: preserve a best-effort `.corrupt` copy when safe, generate a new UUID, write a clean state when possible.
  - Same boot: do not rewrite, refresh, or "improve" the estimated boot time.
  - New boot: write the new estimated boot time and new boot-id.
  - Any state open, lock, read, parse, copy, write, fsync, rename, or permission failure: generate a fresh boot ID for this provider instance, continue without hard failure, and expose degraded diagnostics. This weakens cross-process same-boot stability only for the failing state path; it must not break the strict writer because the writer still receives explicit valid anchors.
- New boot test: `estimated_boottime > last_estimated_boottime + 30s`, where `estimated_boottime = realtime_now - boot_marker_elapsed_now`.
- The state-file boot marker is separate from the per-entry monotonic hot path. Use the best suspend-inclusive elapsed-since-start source available for reboot detection because a suspend-excluding marker would look like a new boot after sleep:
  - Windows: `GetTickCount64` for the state marker; `QueryUnbiasedInterruptTime` remains the per-entry monotonic clock.
  - FreeBSD fallback: prefer `CLOCK_MONOTONIC`/`CLOCK_BOOTTIME` when their platform version increments during suspend; fall back to `CLOCK_UPTIME` only with a recorded diagnostic and accepted false-new-boot risk after suspend.
- Reboot within 30 seconds is explicitly accepted as indistinguishable for state-backed synthesis in this design. The SOW records this as a tolerated edge case from the user decision.

Performance and benchmark contract:

- The per-entry helper hot path must not read identity files, state files, registries, sysctls, WMI, IOKit, or wall-clock boot markers.
- Identity and boot-id discovery happens at helper/provider initialization only.
- Per-entry `monotonic_usec()` uses the fastest correct source accepted for the platform:
  - Linux: `CLOCK_MONOTONIC`.
  - FreeBSD: `CLOCK_UPTIME`.
  - macOS: `CLOCK_UPTIME_RAW`/equivalent.
  - Windows: `QueryUnbiasedInterruptTime`; use the precise variant only if benchmarks justify it.
- Benchmarks must compare:
  - current writer fallback path vs helper/provider `monotonic_usec()` where both exist;
  - helper/provider `entry_options()` convenience vs direct `monotonic_usec()`;
  - initialization cost separately from per-entry hot path;
  - clamp overhead with and without repeated/equal timestamps.
- Results must be recorded per language and platform available locally/CI. Missing OS runtime evidence must be explicit; compile-only evidence is not enough for production claims on that OS.

Risks:

- **Correctness of synthesized boot-id** - an unstable or per-process boot-id breaks reader ordering and dedup within a boot. Mitigation: native boot IDs are used for Linux, FreeBSD 13+, and macOS; state-backed synthesis is used for Windows and FreeBSD fallback; state failures degrade explicitly through diagnostics rather than silently pretending cross-process stability was preserved.
- **Purity regression** - leaking host probing into core paths would violate `AGENTS.md:75-117`. Mitigated by keeping all probing in explicitly named optional files and reusing the project's purity grep/tests.
- **Monotonic vs boot-id epoch mismatch** - the monotonic reading must refer to the same OS boot lifecycle as the boot-id. Native random boot IDs and monotonic clocks do not share a numeric epoch, but they must both reset/change only across the same reboot boundary for journal ordering to make sense. Document the contract so callers do not mix a boot ID from one source with monotonic timestamps from another boot/source.
- **Wall-clock-derived monotonic drift** - deriving per-entry monotonic timestamps from realtime `now - cached_boot_time` would be vulnerable to NTP steps, admin clock changes, VM time corrections, and resume behavior. Mitigated by using native monotonic/uptime clocks for the hot path and clamping generated same-boot values to strictly increase.
- **Breaking writer contract change** - Go and Rust already generate monotonic timestamps when the caller omits them. Some existing paths are process-relative or wall-clock-derived. The accepted design removes these fallbacks for the three journal anchors, so callers/tests/docs must migrate to explicit values or the optional helper API/package.
- **Rust/Go drift** - if implemented per language without a shared contract/spec, bindings could diverge. Mitigated by writing the contract into `product-scope.md` first.
- **macOS/Windows CI** - real-host verification may be limited by available runners; fall back to injected-source deterministic tests + document the gap.

## Pre-Implementation Gate

Status: ready-for-code-implementation

Problem / root-cause model:

- The writer surfaces can write journal bytes anywhere, but current defaults blur ownership of the three mandatory anchors: Go generates fallback monotonic values, Rust has partial helper behavior, and neither product language exposes one portable local-host helper contract. For Netdata, that ambiguity is wrong: SNMP traps may describe a remote device, NetFlow may use synthetic or fixed values, and local collector identity is only one caller choice. Evidence: Go writer fallback lines listed above, Rust helper files listed above, `AGENTS.md:75-117`, downstream Linux gate at netdata `collector.go:162-163`.

Evidence reviewed:

- SDK: `AGENTS.md:75-117`, `.agents/sow/specs/product-scope.md:62-106`, `go/journal/writer_init.go`, `go/journal/lock_owner_*.go` @ `a2361ab`, `go/journal/boot_id_*.go` @ `a2361ab`.
- Downstream: netdata `snmp_traps` `collector.go`, `journal_writer.go`, `monotonic_linux.go`, `monotonic_fallback.go` (read-only, in a separate repo; not modified by this SOW).
- OSS references: `shirou/gopsutil @ df9e25f20` (per-OS HostID/BootTime); `elastic/go-sysinfo @ efea16f` (per-OS machine-id).
- Primary follow-up research: `systemd/systemd v260.1` journal fields and time utility, `freebsd/freebsd-src` `kern.boot_id` and clock docs, `apple-oss-distributions/xnu` `kern.bootsessionuuid`, and Microsoft `QueryUnbiasedInterruptTime`/`GetTickCount64` docs.

Affected contracts and surfaces:

- Core writer public contract changes: machine-id, boot-id, and generated-entry monotonic timestamp become mandatory explicit caller inputs instead of writer fallbacks. The writer remains OS-agnostic and does not discover host identity.
- New separate optional helper API/package on the SDK writer side (per Decision 6) for callers that intentionally want local-host values.
- Spec `product-scope.md` (identity-helper section) and writer docs (`docs/Writer-APIs.md`, `go/API.md`, README) gain the helper contract.
- No change to journal file format, cursor format, or existing public reader/writer signatures.

Existing patterns to reuse:

- Build-tag file layout and native-syscall style of `lock_owner_{linux,bsd,windows,unix_other,other}.go`.
- `ParseUUID`/`UUID` types already in the SDK for value parsing/formatting.
- The conformance/live harness and purity grep/tests already used for the lock helper.

Risk and blast radius:

- Breaking API/behavior change for callers/tests that relied on generated fallbacks. Main risks are migration friction, synthesized-boot-id correctness, purity regression, and Rust/Go contract drift (see Analysis).

Sensitive data handling plan:

- No secrets/customer data involved. machine-id and boot-id are host identifiers; tests MUST use synthetic identities per `AGENTS.md:116`, and only the explicitly-named optional-helper tests may probe the real host. No customer/vendor names; public-repo safe.

Implementation plan:

1. Write the strict writer and helper contract into `product-scope.md` (mandatory explicit anchors, helper package boundary, machine-id/boot-id/monotonic sources per platform, native-first boot-id strategy, state-backed fallback rule, native-only constraint, monotonic epoch contract).
2. Reconcile existing in-tree helpers and defaults: Rust `journal-common` identity/time helpers, Go writer-start fallback, and Rust log-writer monotonic generation. Remove or gate fallbacks so the writer fails fast when the three anchors are missing.
3. Implement Rust and Go helper APIs/packages against one shared contract: machine-id, native/state-backed/degraded boot-id source handling, and a fast local-host monotonic timestamp source that callers can use when local-host values are appropriate.
4. Tests: missing-anchor writer errors; deterministic (injected reboot marker and monotonic source) boot-id stability/reboot-change; cross-process boot-id agreement; generated monotonic clamping in helper/provider state; suspend/resume simulation with injected sources; per-platform machine-id; round-trip a synthetic-anchor journal through the reader + conformance/live harness.
5. Benchmarks: compare helper monotonic generation against current default generation in the targeted language(s), and record whether the helper source is "fastest reasonable" or whether a separate benchmark-backed approximate mode SOW is needed.
6. Docs (`Writer-APIs.md`, `go/API.md`, README) + a worked opt-in example.

Validation plan:

- Strict writer tests:
  - Create/new-log missing `machine_id` fails before file mutation in Go and Rust.
  - Create/new-log missing `boot_id` fails before file mutation in Go and Rust.
  - Append missing `monotonic_usec` fails before entry mutation in Go and Rust.
  - Explicit zero monotonic succeeds only through the language's explicit "set" representation and remains verifier-compatible when ordering permits it.
- Helper tests:
  - Deterministic injected-source tests prove same-boot `boot_id` stability and simulated-reboot change.
  - Cross-process Windows and FreeBSD-fallback state tests prove serialized initialization and byte-identical same-boot `boot_id` when state is healthy.
  - Corrupt-state tests prove recovery without crashing, without world-writable files, and with degraded diagnostics if clean rewrite is impossible.
  - Helper/provider monotonic clamping tests prove strictly increasing same-provider values.
  - Injected suspend/resume and wall-clock-step simulations prove helper monotonic does not use wall-clock `now - boot_time` on the per-entry hot path.
- Compatibility tests:
  - Round-trip explicit-anchor files through every supported repository reader.
  - Run applicable conformance/live writer tests because writer append behavior changes.
  - Run stock `journalctl --verify --file` and `journalctl --directory` where available, using repository-local fixtures only.
- Performance tests:
  - Per-entry helper/provider timestamp microbenchmarks by language.
  - Initialization benchmarks separate identity/state discovery from per-entry hot path.
  - Record missing OS runtime coverage explicitly.
- Purity tests:
  - Grep/static checks prove core writer/reader files do not import optional host helper packages/modules.
  - Grep/static checks prove no subprocess APIs in helper code.
  - Dependency audit proves no Go CGO.
- Process validation:
  - `.agents/sow/audit.sh`.
  - Repeat external reviewers per project cadence before implementation.

Artifact impact plan:

- AGENTS.md: likely unaffected (Layer 3 already described); update only if the helper introduces a new cross-cutting rule.
- Runtime project skills: check `project-docs-authoring`; update if examples/docs conventions touched.
- Specs: `product-scope.md` updated with the identity-helper contract (required).
- End-user/operator docs: `docs/Writer-APIs.md`, `go/API.md`, README writer sections updated.
- End-user/operator skills: none expected.
- SOW lifecycle: this SOW in `pending/` until decisions; downstream netdata gate-removal tracked as a separate netdata-repo SOW.
- SOW-status.md: add SOW-0115 to Pending (and `.agents/sow/SOW-status.md`).

Open-source reference evidence:

- `shirou/gopsutil @ df9e25f20`: `host/host_linux.go`, `host/host_freebsd.go`, `host/host_darwin.go`, `host/host_windows.go`.
- `elastic/go-sysinfo @ efea16fc3c4f`: `providers/linux/machineid.go`, `providers/darwin/machineid_darwin.go`, `providers/windows/machineid_windows.go`, `providers/linux/boottime_linux.go`, `providers/darwin/boottime_darwin.go`, `providers/windows/boottime_windows.go`.
- `osquery/osquery @ 225804016439`: `osquery/core/system.cpp`, `osquery/utils/system/uptime.cpp`.
- `DataDog/datadog-agent @ 67ac6ce55e68`: `pkg/ebpf/time.go`, `pkg/network/time_converter_windows.go`, `pkg/util/uuid/uuid_windows.go`.
- `open-telemetry/opentelemetry-go @ 6e2b9214f05e`: `sdk/resource/host_id.go`, `sdk/resource/host_id_bsd.go`, `sdk/resource/host_id_darwin.go`, `sdk/resource/host_id_linux.go`, `sdk/resource/host_id_windows.go`.

Resolved decisions from 2026-06-17:

- Decision 8A-prime accepted: native-first boot ID, state only where required.
- Decision 9A accepted: remove writer fallbacks for long-term clarity; fix tests and current Netdata consumers.
- Decision 10A accepted/recommended: state-backed helper failures generate a fresh boot ID and continue with diagnostics; Rust host probing moves out of core-imported `journal-common` paths.

Open implementation proofs:

- Prove the Go macOS `gethostuuid()` path can remain no-CGO, or use an equally native no-subprocess machine-id source and record why it is correct.
- Confirm exact Windows state default path behavior on service and user contexts during implementation.
- Confirm FreeBSD fallback behavior on FreeBSD 12 if a runner or VM is available; otherwise record compile/injected-test coverage and the runtime evidence gap.

## Implications And Decisions

User decisions are recorded below. Options carry pros/cons/risks; accepted options are marked explicitly.

### Decision 1 - API placement

- **User decision 2026-06-16: 1C accepted, later refined by Decision 6A.**
- **1A. Separate public helper only; caller passes everything per append.** Pros: clean purity boundary. Cons/RISK: every caller must own monotonic state and clamping, which invites repeated downstream bugs. Acceptable only as a low-level escape hatch.
- **1B. Auto-discover inside the core writer constructor.** Pros: zero caller effort. Cons/RISK: directly violates Layer-1 purity (core must not probe host identity), breaks the "synthetic identities in tests" rule, and contradicts the spec; high architectural debt. Rejected.
- **1C. Public opt-in helper outside the core writer (ACCEPTED, refined).** Caller explicitly invokes the host helper when local-host values are desired, then passes explicit values to the writer. Decision 6A supersedes the earlier wrapper-owned-state framing: the helper may own local-host discovery and monotonic provider state, but the writer boundary stays explicit and strict. Pros: honors Runtime Purity Architecture (`AGENTS.md:75-117`) and `product-scope.md:62-80`; core stays testable with synthetic identities; callers can choose local-host, remote-device, fixed, synthetic, or auto-increment values. Cons: callers must wire explicit values. Risk: low if helper names and errors make the ownership obvious.
- Reasoning: 1C is the only option that satisfies both constraints: host discovery remains explicit, and the writer does not silently choose an event identity source.

### Decision 2 - Historical FreeBSD boot-id strategy, superseded by Decision 8A-prime

- **User decision 2026-06-16: 2A accepted for FreeBSD, with a minimal locked state file.**
- **Superseded on 2026-06-17 by Decision 8A-prime after follow-up primary-source research verified native FreeBSD `kern.boot_id` on FreeBSD 13+.** The original 2A state-backed FreeBSD design remains valid only as the fallback for FreeBSD 12 or environments where `kern.boot_id` is unavailable.
- **2A. State-backed synthesis under caller-approved runtime storage (SUPERSEDED as FreeBSD primary path; retained as fallback).** Derive or generate one UUID for the current OS boot, persist it with the best available native reboot marker, and reuse it while the marker still identifies the same boot. Pros: robust same-boot cross-process identity when a native boot ID is unavailable. Cons: requires state path and locking protocol. Risk: degraded state failure can only provide provider-local boot-id stability.
- **2B. Stateless deterministic synthesis from machine-id + native reboot marker.** Pros: no state file, no helper-side writes, simpler deployment. Cons/RISK: only safe where the marker is proven stable for one boot; FreeBSD `kern.boottime` is documented as recomputed after wall-clock changes/resume, and macOS/Windows marker semantics need explicit proof. Acceptable only as a documented best-effort mode or for OSes where evidence proves stability.
- **2C. Random UUID generated at helper init.** Pros: trivial. Cons/RISK: differs per process and per restart within the same boot - breaks multi-writer and restart ordering under one boot. Rejected.
- Reasoning update: journal readers correlate monotonic timestamps within a boot-id; only a stable per-boot value is correct. The original reasoning correctly rejected `kern.boottime`, but incorrectly assumed FreeBSD lacked a native boot UUID. FreeBSD 13+ `kern.boot_id` fixes that primary path.

### Decision 3 - Language scope

- **User decision 2026-06-16: Rust and Go are mandatory in this SOW.** Rationale: Netdata has immediate consumers in both languages: `snmp_traps` in Go and NetFlow in Rust.
- **User decision 2026-06-16: SOW-0116 supersedes the earlier all-four-binding scope.** Python and Node.js are retired product targets and moved to `experiments/`; this SOW now targets Rust and Go only.
- **3A. Rust+Go in this SOW (ACCEPTED after SOW-0116, long-term-best for current product scope).** Pros: matches Netdata's actual consumers, removes native-access uncertainty from retired targets, and keeps implementation/review focused on product code. Cons: historical parity goals no longer apply. Impact: no Python/Node helper parity SOW is created because those targets are retired. Risk: any future revival of Python/Node must start from experiments with a new user-approved SOW.

### Decision 4 - Monotonic clock semantics and packaging

- **User decision 2026-06-16: 4A accepted.**
- **4A. Correct-by-default boot-monotonic writer clock state (ACCEPTED).** Use the closest Linux/journald-compatible suspend-excluding source per OS: Linux `CLOCK_MONOTONIC`; FreeBSD `CLOCK_UPTIME`; macOS `CLOCK_UPTIME_RAW`/equivalent; Windows unbiased interrupt time. Capture writer base state once, use a fast native monotonic elapsed/uptime source per entry, and clamp generated same-boot values to `last + 1us` when needed. Do not derive hot-path monotonic timestamps from wall-clock `now - cached_boot_time`.
- **4B. Add explicit approximate/fast mode after benchmark evidence.** Pros: gives high-throughput callers a controlled escape hatch using coarse/approx clocks such as Linux `CLOCK_MONOTONIC_COARSE`, FreeBSD `_FAST`, macOS `_APPROX`, or non-precise Windows variants. Cons/RISK: lower timestamp precision and more API/docs surface. Not part of the default contract unless a later benchmark-backed decision accepts it.
- **4C. Maximum-speed approximate mode as the default.** Pros: fastest possible call path. Cons/RISK: silently degrades timestamp precision or suspend semantics. Rejected as the default.
- Reasoning: journal entries need a reliable boot-monotonic ordering anchor. Correct default semantics plus writer-owned clamping protect callers; approximate clocks require explicit opt-in and measurement.

### Decision 5 - Native-only constraint (confirm, not a fork)

- **User decision 2026-06-16: 5A accepted.**
- **5A. Confirm hard native-only/no-subprocess policy (ACCEPTED, long-term-best).** The helper MUST use native APIs/syscalls only (sysctl(3) syscall, IOKit/`gethostuuid`, registry API), never shelling out to `ioreg`/`system_profiler`/`sysctl`-binary/`ps`/`reg.exe`/PowerShell. This is already mandated by `product-scope.md:77-80`; the user confirmed it as a hard requirement. Go remains no-CGO.
- **5B. Allow subprocess fallback only in optional helper (REJECTED).** Pros: easier access to macOS/FreeBSD/Windows host metadata. Cons/RISK: expands attack surface, adds command availability drift, and weakens the runtime-purity contract.
- **5C. Allow native add-on or FFI helper dependencies (REJECTED).** Pros: direct OS access where the language standard library is missing a needed API. Cons/RISK: introduces native build/ABI/package trust risk and requires a separate package policy decision.

### Decision 6 - Writer fallback policy and helper packaging

- **User decision 2026-06-16: 6A accepted.** This records the user's answer "A" to the follow-up strict-writer question and supersedes the earlier fallback-preserving framing.
- **6A. Strict writer plus separate helper API/package (ACCEPTED, long-term-best).** The core writer does not fallback or guess the three anchors: `_MACHINE_ID`, `_BOOT_ID`, and generated-entry `__MONOTONIC_TIMESTAMP`. Callers must provide them explicitly. The SDK also ships a separate optional helper API/package that returns local-host values for callers that intentionally want collector-host identity/time. Pros: correct separation of concerns; avoids hidden wrong metadata; lets SNMP traps use remote-device identity and NetFlow use synthetic/fixed values if that is the right product choice. Cons: breaking change for callers/tests relying on fallback. Impact: writer docs, examples, tests, and Rust/Go APIs must migrate to explicit anchors or helper calls. Risk: migration work and stricter errors must be handled consistently across Rust and Go.
- **6B. Keep existing writer fallbacks and add helper (REJECTED).** Pros: lower migration cost. Cons/RISK: preserves ambiguity and may silently write collector metadata or process-relative monotonic values for events that describe another source.
- **6C. Hide helper use inside a writer/log convenience wrapper (REJECTED).** Pros: easiest for local-host events. Cons/RISK: encourages callers to stop thinking about event-source identity; this is wrong for SNMP traps and possibly NetFlow.
- Reasoning: the writer is a file-format writer, not the owner of event identity. The accepted long-term-best design is explicit values at the writer boundary plus a separate local-host helper for callers that choose it.

### Decision 8 - Corrected non-Linux boot-id strategy after follow-up research

- **User decision 2026-06-17: 8A-prime accepted.** The user answered "8: yes, accepted" after the corrected native-first recommendation was presented.
- **8A-prime. Native-first boot ID; state only where required (ACCEPTED, long-term-best).**
  - Linux: native `/proc/sys/kernel/random/boot_id`.
  - FreeBSD 13+: native `kern.boot_id`.
  - FreeBSD 12 or unavailable `kern.boot_id`: state-backed fallback.
  - macOS: native `kern.bootsessionuuid`; undocumented public-API risk recorded and accepted.
  - Windows: state-backed synthesis because no documented unprivileged runtime boot-session UUID was found.
  - Pros: simplest correct design; uses kernel/native boot identity where the OS has one; minimizes persistent state; keeps helper state out of Linux/FreeBSD 13+/normal macOS. Cons: macOS depends on an undocumented sysctl; Windows still needs state for cross-process same-boot stability. Risk: if macOS removes `kern.bootsessionuuid`, the helper degrades through Decision 10 diagnostics/fallback instead of blocking writer use.
- **8B. State-backed for all non-Linux (REJECTED).** Pros: one uniform cross-platform model. Cons/RISK: unnecessary state on FreeBSD 13+ and macOS, more permission/path/security surface, worse long-term maintainability.
- **8C. Stateless/native marker for all non-Linux (REJECTED).** Pros: no helper state. Cons/RISK: unsound on Windows because no documented runtime boot-session UUID exists.
- Reasoning: native-first is the long-term-best design because persistent state is a workaround, not the product model, when the OS already exposes a per-boot identity.

### Decision 9 - Strict writer migration boundary

- **User decision 2026-06-17: 9A accepted.** The user stated the long-term-best direction is clarity and that tests plus current Netdata consumers can be fixed; fallbacks should be removed.
- **9A. Remove writer fallbacks for the three anchors (ACCEPTED, long-term-best).** Core writer/log APIs must fail fast when `machine_id`, `boot_id`, or append `monotonic_usec` is missing. Existing tests, examples, and Netdata consumers migrate to explicit synthetic values or the optional helper. Pros: clear ownership; no silent collector-host metadata; no process-relative monotonic values pretending to be boot-anchored. Cons: breaking change. Impact: update Rust/Go tests, writer docs, examples, and downstream Netdata integration. Risk: missing migration sites; mitigate with same-failure searches and compiler/test failures.
- **9B. Keep compatibility fallbacks with warnings (REJECTED).** Pros: lower immediate migration cost. Cons/RISK: preserves ambiguity and hidden wrong metadata.
- **9C. Keep only test/fixture fallback escape hatches (REJECTED as default).** Pros: easier byte-fixture updates. Cons/RISK: invites production code to depend on hidden fallback paths; tests should use explicit synthetic anchors instead.
- Reasoning: this project is early enough that the right long-term public contract is worth the breaking change.

### Decision 10 - State-file locking/security and Rust helper relocation

- **User decision 2026-06-17: 10A accepted/recommended.** The user asked for the obvious recommendation and stated: on failure, generate a new boot ID and move on; no hard failures.
- **10A. Fully separate host helper, hardened state best effort, no boot-id hard failure (ACCEPTED, long-term-best).**
  - Rust host-probing helpers move out of core-imported `journal-common` paths into `systemd-journal-sdk-host` / `journal_host`. Any compatibility wrapper must be deprecated and must not be imported by core writer/log writer code.
  - State-backed boot-id code is used only for Windows and FreeBSD fallback.
  - State initialization uses an exclusive lock, best-effort corrupt copy, atomic write/rename when practical, and no world-writable state file.
  - Any boot-id state/discovery failure generates a fresh valid boot ID for this provider instance, continues, and records degraded diagnostics/source metadata. The helper does not pretend cross-process stability is guaranteed when state failed.
  - Pros: simple public model; no writer fallback; no deployment hard-stop from a broken state directory; diagnostics preserve clarity. Cons: degraded failure mode can produce different boot IDs across processes during one OS boot. Impact: tests must verify both stable-state and degraded-state behavior. Risk: callers may ignore diagnostics; docs must state that degraded boot ID is valid but not cross-process-stable.
- **10B. Fail helper initialization on state errors (REJECTED).** Pros: strongest correctness signal. Cons/RISK: too fragile operationally; a permissions or corrupt-state issue would disable local-host helper use entirely.
- **10C. Keep Rust host helpers in `journal-common` behind features (REJECTED as default).** Pros: lower Rust migration cost. Cons/RISK: keeps host probing too close to core paths and weakens the architecture boundary.
- Reasoning: the writer remains strict; the helper is an opt-in convenience service. For long-term portability and operations, the helper should always provide valid anchors when it reasonably can, while exposing degraded quality instead of hiding it.

### Decision 11 - Implementation routing after delegated implementer timeouts

- **User decision 2026-06-17: 11A accepted.** The user answered "11a" after both configured implementer routes timed out.
- **11A. Project manager implements directly from the partial worktree (ACCEPTED, long-term-best for this SOW).** Pros: fastest recovery from two timed-out implementer runs; allows the known partial-code risks to be repaired directly before the normal reviewer batch. Cons: explicit exception to the standing routing rule that code implementation is delegated. Impact: this exception is scoped to SOW-0115 and must be recorded before direct code edits continue. Risk: reviewers must pay close attention because the project manager is now the implementer for this SOW.
- **11B. Keep delegated implementation and approve another implementer model (REJECTED).** Pros: preserves normal routing. Cons/RISK: two 30-minute runs already timed out and left partial/incomplete work; more delegation risks more churn.
- **11C. Pause SOW-0115 and clean back to spec/docs-only state (REJECTED).** Pros: reduces immediate worktree mess. Cons/RISK: leaves Netdata portability work blocked and loses useful strict-Go changes that already compile on Linux.
- Reasoning: the remaining work is now concrete implementation/repair, not an unresolved design fork. Direct implementation is the long-term-best route to get a coherent patch ready for external review.

## Plan

1. **Spec first.** Update `product-scope.md` with the strict writer contract, helper package boundary, corrected platform source table, native-first boot-id strategy, and state-file fallback contract.
2. **Strict writer contract.** Remove silent `machine_id`, `boot_id`, and `monotonic_usec` fallbacks in Rust and Go writer/log layers; keep file-id/seqnum-id defaults and realtime fallback scoped as documented.
3. **Helper package/module skeleton.** Add the separate helper surfaces:
   - Go: `go/journalhost`.
   - Rust: `systemd-journal-sdk-host` / `journal_host`.
4. **State-backed boot-id fallback.** Implement the locked two-field state file for Windows and FreeBSD fallback with injected-source tests before wiring it into platform-specific helpers.
5. **Platform helpers.** Implement per-OS identity and monotonic sources in Rust and Go, keeping host probing out of core writer files.
6. **Tests and benchmarks.** Add missing-anchor failures, helper determinism/race/corrupt-state tests, monotonic clamp tests, interoperability verification, and per-entry microbenchmarks.
7. **Docs.** Update writer docs and examples to show explicit synthetic anchors and optional local-host helper anchors. Make SNMP traps and NetFlow examples describe caller-owned identity choice, not automatic collector identity.
8. **Review and iterate.** Run the full reviewer batch against the whole SOW and changed surface. Do not start implementation until reviewer blockers are resolved or the user accepts a specific risk.

## Delegation Plan

Implementer:

- Per `AGENTS.md` routing (2026-06-11): external implementer `llm-netdata-cloud/minimax-m3-coder` (fallback `llm-netdata-cloud/glm-5.1`), with the project manager writing prose, orchestrating, and validating. Confirm at implementation start.

Reviewers:

- The other `llm-netdata-cloud` pool models as read-only reviewers, run as one batch against the complete SOW per project cadence.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record implementer/reviewer/audit/model-availability failures in the Execution Log; do not close on audit failure.

## Execution Log

### 2026-06-16

- SOW created in `pending/`. Investigation complete: SDK purity architecture, writer API plug-in points, existing lock-helper `currentBootID()` precedent, and authoritative per-platform native sources verified against gopsutil/go-sysinfo. Initial design decisions presented to the user; implementation blocked pending answers.
- User decision recorded: host identity/time discovery is a public opt-in writer-side helper API, not silent core-writer behavior. This was later refined by Decision 6A so the helper/package may provide local-host values and monotonic provider state, but callers still pass the three anchors explicitly to the strict writer.
- User decision recorded: FreeBSD `_BOOT_ID` synthesis uses a locked state file under `/var/run` by default, with API consumer path override. The state file stores only `last_estimated_boottime` and `last_boot_id`, is locked during helper initialization, uses a 30-second minimum reboot-time threshold, and is not refreshed or rewritten on same-boot drift.
- This FreeBSD state-file-primary decision was superseded on 2026-06-17 after follow-up research verified native FreeBSD 13+ `kern.boot_id`; state is now only a fallback for FreeBSD versions/environments without native `kern.boot_id`.
- User scope constraint recorded: Rust and Go helpers are mandatory in this SOW because Netdata has immediate Rust NetFlow and Go `snmp_traps` consumers.
- Earlier all-binding scope was superseded by SOW-0116; Rust and Go remain mandatory product targets.
- User decision recorded: hard native-only/no-subprocess policy is confirmed. No subprocess fallback and no Go CGO may be introduced without returning to the user with evidence and a new decision.
- User decision recorded: the writer must be strict and OS-agnostic. The three anchors (`_MACHINE_ID`, `_BOOT_ID`, and generated-entry `__MONOTONIC_TIMESTAMP`) are mandatory caller-provided values; the SDK will provide a separate optional helper API/package for callers that intentionally want local-host values.
- SOW hardening after Decision 6A: added the concrete strict-writer contract, reference API/package shape, FreeBSD state-file format and locking contract, performance benchmark contract, and validation matrix. Retired-language portions were later superseded by SOW-0116.
- User decision recorded by SOW-0116: Python and Node.js are retired product targets. This SOW's implementation scope is Rust and Go only.
- External review batch requested by user and run read-only with `claude`, `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen`. Result: 7/7 reviewers voted NOT READY TO IMPLEMENT. Review-discovered blockers and accepted/disputed dispositions are recorded below.
- Repeat external review batch requested by user and run read-only against the hardened SOW with `claude`, `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen`. Result: 6/7 reviewers voted NOT READY TO IMPLEMENT; `deepseek` voted READY TO IMPLEMENT with non-blocking issues. At that point, the SOW remained open in `pending/` and blocked on user decisions later resolved on 2026-06-17.

### 2026-06-17

- Follow-up research from `minimax-m2.7` (online-only) and `claude-opus-4.8` (repo/mirror access) was reviewed and independently verified against primary sources.
- Verification corrected several report conflicts: systemd/journald uses `CLOCK_MONOTONIC` for `__MONOTONIC_TIMESTAMP`; FreeBSD 13+ exposes native `kern.boot_id`; FreeBSD uses `CLOCK_UPTIME` for Linux-like suspend-excluding monotonic semantics; macOS exposes native `kern.bootsessionuuid` but it is undocumented as a public Apple API; Windows has no documented unprivileged runtime boot-session UUID and uses `QueryUnbiasedInterruptTime` for the per-entry monotonic clock.
- User accepted Decision 8A-prime: native-first boot-id discovery with state only where required. State-backed synthesis is now Windows primary and FreeBSD fallback only.
- User accepted Decision 9A: remove writer fallbacks for the three anchors for long-term clarity; fix tests and current Netdata consumers.
- User accepted/reinforced Decision 10A: fully separate host helper, no boot-id hard failure on state/discovery failure, generate a fresh boot ID and continue with diagnostics.
- Spec/doc hardening completed before code implementation: `product-scope.md`, `README.md`, `docs/Writer-APIs.md`, `docs/Getting-Started.md`, `docs/API-Overview.md`, `docs/Production-Profiles.md`, `docs/Go-API.md`, `docs/Rust-API.md`, `go/API.md`, `go/README.md`, and `rust/README.md` now describe strict explicit identity/time anchors and the opt-in helper boundary. Verified by docs validators and focused stale-reference searches.
- Implementation delegated to `llm-netdata-cloud/minimax-m3-coder` per project routing. The run timed out after 30 minutes with exit code 124 and left partial Go-only changes: strict Go writer checks, a partial `go/journalhost` package, and additional `product-scope.md` wording. Immediate local check after timeout: `cd go && go build ./...` passed on Linux. Known unfinished work: Go tests still need migration, cross-platform Go helper files need compile/source verification, Rust strict writer/helper work is not started, docs examples are not migrated, and final validation/review are pending. Fallback implementer routing starts next.
- Fallback implementer `llm-netdata-cloud/glm-5.2-max` was run with explicit repair instructions for the partial minimax changes. It timed out after 30 minutes with exit code 124 before making code repairs. Local check after timeout: `cd go && GOCACHE=$PWD/../.local/go-cache GOMODCACHE=$PWD/../.local/go-mod go build ./...` still passes on Linux. The work remains incomplete and blocked on implementation routing under the project rule that code implementation is delegated unless the user explicitly changes that routing.
- User decision 11A recorded: direct implementation by the project manager is authorized for this SOW after the two delegated implementer timeouts.
- Direct implementation completed:
  - Go strict writer/log contract: `journal.Create` and `NewLog` require explicit machine ID and boot ID, append paths require explicit monotonic timestamps, same-boot monotonic values clamp forward, and tests/examples migrated to explicit anchors.
  - Go optional helper package added as `go/journalhost`, with native Linux/FreeBSD/macOS sources, Windows state-backed boot-id synthesis, deterministic state tests, degraded boot-id diagnostics, and non-Linux compile coverage.
  - Rust host helper crate added as `systemd-journal-sdk-host` / `journal_host`; host identity discovery moved out of `journal-common`; Rust log writer requires explicit identity and append monotonic timestamps; low-level writer clamps same-boot monotonic values.
  - Docs, verified examples, and product scope updated for the strict writer and opt-in helper boundary.
- During local review, a Decision 10A mismatch was found and fixed in Go: Linux/macOS native boot-id failures and FreeBSD state fallback failures now generate a fresh degraded boot ID with diagnostics instead of hard-failing when UUID generation still succeeds. The Go state-backed path also honors public `LoadOptions.Now` and `LoadOptions.BootMarkerNow` overrides for deterministic tests.
- Completed-implementation reviewer batch round 1 ran read-only against the whole SOW and changed surface with `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen`. Votes: 5/6 READY TO IMPLEMENT; `glm` voted NOT READY TO IMPLEMENT.
- Round-1 real findings fixed:
  - Go `Log.Append` / `AppendRaw` now validate explicit monotonic options before opening/creating the active file, so missing monotonic cannot create a file before returning `ErrMissingMonotonicUsec`.
  - Rust compatibility methods `write_entry`, `write_fields`, and `write_structured` are deprecated and documented as strict-contract compatibility methods that always error without explicit timestamps.
  - `journal_common::time::monotonic_now` documentation now describes the actual per-OS `CLOCK_MONOTONIC` semantics and warns callers not to use it as the SOW helper source.
  - Go FreeBSD state fallback default path now uses effective UID.
  - Runtime-purity tests now include the new helper package/crate names in the forbidden-core-import pattern set.
  - Rust Unix state directory creation now applies best-effort `0700` parent permissions after `create_dir_all`.
  - Dead Go state-backed lock variables and the dead `Writer.started` / `startTimeForTailMonotonic` path were removed.
  - Go `AppendMap` compatibility wrappers now document that strict-contract callers should use `AppendMapWithOptions`.
- Round-1 false finding rejected: `glm` claimed Go's Windows `QueryUnbiasedInterruptTime` wrapper checks the return value of a VOID function. Official Microsoft Learn documentation for `QueryUnbiasedInterruptTime` says the signature is `BOOL QueryUnbiasedInterruptTime(PULONGLONG UnbiasedTime)` and the return value is nonzero on success; the VOID API is `QueryUnbiasedInterruptTimePrecise`. Source: <https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttime>.
- Round-1 non-blocking findings explicitly left unchanged:
  - A dedicated Rust `BootId` error variant is API polish, not required for the strict writer contract; existing errors are already deterministic and tests verify behavior.
  - Go Unix monotonic `ClockGettime` closures remain infallible-style because these fixed OS clock IDs are expected to exist on supported targets; if the clock unexpectedly fails, the provider's per-instance clamp still preserves strict increase. A fallible per-entry helper API can be considered only with benchmark/API evidence.
- Completed-implementation reviewer batch round 2 ran read-only with the same scope plus round-1 fix notes. Completed votes: `glm`, `kimi`, `mimo`, and `deepseek` READY TO IMPLEMENT; `minimax` NOT READY TO IMPLEMENT; `qwen` failed midstream with a LiteLLM socket timeout before final verdict.
- Round-2 real findings fixed:
  - Go `Writer.Open` / `OpenWithOptions` on an existing empty file now rejects append attempts that have explicit monotonic metadata but no default or per-entry boot ID, returning `ErrMissingBootID` before entry mutation. Focused tests cover both the rejection and the explicit per-entry boot-id success path.
  - Rust high-level log identity validation now rejects `Uuid::nil()` for machine ID and boot ID, not only absent IDs. Focused tests cover nil machine ID and nil boot ID.
  - Public docs now name the optional helper package/crate in `docs/Writer-APIs.md`, `docs/Go-API.md`, `docs/Rust-API.md`, and `rust/README.md`.
  - `product-scope.md` now states that Rust's low-level `JournalFileOptions::new` is an explicit exact-regeneration/file-identity constructor, while high-level strict log enforcement is in `journal_log_writer::Log`.
  - `product-scope.md` now clarifies that eager open may create an empty active file after explicit identity validation; it still cannot append an entry without explicit monotonic metadata.
- Round-2 non-blocking findings dispositioned:
  - Eager open file creation before entry monotonic exists is accepted as lifecycle behavior, not entry mutation; the spec now names this explicitly.
  - Dedicated Rust `BootId` error variant remains rejected as API polish for this SOW.
- Completed-implementation reviewer batch round 3 ran read-only with the same scope plus round-1 and round-2 fix notes. Completed votes: `glm`, `kimi`, `mimo`, `deepseek`, and `qwen` READY TO IMPLEMENT. `minimax` timed out after 30 minutes; its partial transcript found no blocker and stated it would vote READY TO IMPLEMENT.
- Round-3 low hygiene findings fixed:
  - `product-scope.md` no longer says Windows Rust generates monotonic timestamps when callers omit them; it now states the strict writer contract and identifies `journal_host` as the helper source for local-host Windows monotonic timestamps.
  - Rust state-backed helper temp and lock file opens now use `O_CLOEXEC` on Unix.
  - The top-level README advanced Rust package list now includes `systemd-journal-sdk-host` / `journal_host`.
  - Generated untracked Go helper test binaries left by earlier cross-compile runs were removed from the worktree.
- Round-3 non-blocking findings dispositioned:
  - Dedicated Rust `BootId` error variant remains rejected as API polish for this SOW.
  - Go helper monotonic clock closures continue to degrade to zero-plus-provider-clamp on unexpected clock failure; this preserves strict increasing values and avoids changing the per-entry hot-path API in this SOW.
  - macOS `CLOCK_UPTIME_RAW` Rust literal remains accepted because the constant is stable on macOS and the code is isolated to the macOS platform module.

## External Review - 2026-06-16

Reviewer votes:

- `claude`: NOT READY TO IMPLEMENT.
- `glm`: NOT READY TO IMPLEMENT.
- `minimax`: NOT READY TO IMPLEMENT.
- `kimi`: NOT READY TO IMPLEMENT.
- `mimo`: NOT READY TO IMPLEMENT.
- `deepseek`: NOT READY TO IMPLEMENT.
- `qwen`: NOT READY TO IMPLEMENT.

Accepted blocking findings and current dispositions:

1. **Open decision blocker resolved.** Resolved on 2026-06-16 by Decision 6A: strict writer plus separate helper API/package. Remaining work is SOW detailing, not a user decision.
2. **Existing Rust helpers were omitted from the original SOW analysis.** Addressed in the concrete contract: existing `journal-common` helpers must be reconciled by moving/deprecating host probing into the new `journal_host` package boundary or feature-gating it so core/log writer code does not import host discovery.
3. **Existing per-language default monotonic behavior must be reconciled.** Addressed by Decision 6A and the strict writer contract: remove/gate fallback monotonic generation and require explicit caller/helper-provided values.
4. **Boot-id synthesis needs implementation-level detailing.** Superseded by 2026-06-17 follow-up research and Decisions 8A-prime/10A. Current disposition: native Linux, native FreeBSD 13+ `kern.boot_id`, native macOS `kern.bootsessionuuid` with documented risk, Windows state-backed synthesis, FreeBSD state fallback only when native `kern.boot_id` is unavailable, and degraded no-hard-failure boot-id generation with diagnostics on state/discovery failure.
5. **API surface is not concrete enough.** Addressed by the reference API shape table for Rust and Go.
6. **Performance requirement lacks a benchmark plan.** Addressed by the performance and benchmark contract.
7. **Language/native feasibility must be explicit.** Addressed by per-language package boundaries. Go remains no-CGO.
8. **Validation plan needs cross-process and suspend/resume simulation coverage.** Addressed by the validation matrix.
9. **Runtime-purity placement must be explicit.** Addressed by helper package/module boundaries, purity checks, and dependency audit requirements.

Disputed or corrected reviewer claims:

- Some reviewers claimed Linux `CLOCK_MONOTONIC` includes suspend time. This is rejected: Linux manual pages state `CLOCK_MONOTONIC` does not count suspended time; `CLOCK_BOOTTIME` includes suspend.
- Some reviewers claimed macOS `CLOCK_UPTIME_RAW` includes suspend time or is the wrong suspend-excluding source. This is rejected: the macOS man page states `CLOCK_UPTIME_RAW` does not increment while asleep.
- Some reviewers asserted the SOW's FreeBSD/macOS clock choices are wrong solely because Rust currently uses `CLOCK_MONOTONIC` on all Unix. The accepted disposition is narrower: the SOW's default clock choices remain plausible based on official docs, but the SOW must reconcile or explicitly track the existing Rust divergence before implementation.

Initial implementation-readiness verdict:

- The initial SOW hardening pass was not enough for implementation. The SOW stayed `open` in `pending/` for repeat external review.

## Repeat External Review - 2026-06-16

Reviewer votes:

- `claude`: NOT READY TO IMPLEMENT.
- `glm`: NOT READY TO IMPLEMENT.
- `minimax`: NOT READY TO IMPLEMENT.
- `kimi`: NOT READY TO IMPLEMENT.
- `mimo`: NOT READY TO IMPLEMENT.
- `deepseek`: READY TO IMPLEMENT, with non-blocking concerns about macOS/Windows boot markers, Rust helper relocation, helper vs writer monotonic clamping, and ENTRY-header wording.
- `qwen`: NOT READY TO IMPLEMENT.

Repeat-review blocking findings after SOW-0116 language-scope reduction:

1. **macOS/Windows boot-id synthesis lacks named stable reboot markers.** Reviewers agreed the SOW cannot leave the central boot-id correctness strategy to implementation-time evidence without a fallback decision.
2. **FreeBSD state-file detection, locking, and security need stronger rules.** Reviewers requested exact lock primitive(s), atomic-write rules, symlink/TOCTOU hardening, corrupt-copy rules, and tests for wall-clock jumps or a wall-clock-free reboot detector.
3. **Strict writer migration is under-specified.** Reviewers requested an explicit decision for `LogIdentityAuto`, existing fallback behavior, public examples/tests that use zero-value options, and whether low-level fixture/raw writer APIs retain an explicit escape hatch.
4. **Rust helper relocation and clock semantics must be reconciled.** Reviewers requested a definitive plan for moving host-probing out of `journal-common` and for aligning Rust's current Unix `CLOCK_MONOTONIC` helper with the accepted per-OS clock contract.
5. **Benchmark and validation targets need tightening.** Reviewers requested numeric performance expectations, cross-language interoperability chains, transitive purity checks, and explicit same-state-path/cross-process assumptions.

Repeat implementation-readiness verdict:

- NOT READY TO IMPLEMENT at review time. This verdict was before the 2026-06-17 follow-up research and user decisions. The SOW stays `open` in `pending/` while the corrected contract is hardened into specs/docs before implementation.

## Follow-up Research And Decision Closure - 2026-06-17

Primary-source corrections:

- `systemd/systemd v260.1` confirms `__MONOTONIC_TIMESTAMP` is `CLOCK_MONOTONIC`, not `CLOCK_BOOTTIME`.
- `freebsd/freebsd-src` confirms native `kern.boot_id` exists in FreeBSD 13+ and is absent from stable/12.
- `freebsd/freebsd-src` clock docs confirm `CLOCK_UPTIME` is the FreeBSD suspend-excluding monotonic source.
- `apple-oss-distributions/xnu` confirms read-only `kern.bootsessionuuid`, with the risk that it is an undocumented sysctl.
- Microsoft docs confirm `QueryUnbiasedInterruptTime` is the Windows working-state, sleep-excluding monotonic source and `GetTickCount64` is appropriate only as a state boot marker.

Decision closure:

- Decision 8A-prime accepted: native-first boot ID, state only where required.
- Decision 9A accepted: strict writer migration removes fallbacks and updates tests/Netdata consumers.
- Decision 10A accepted/recommended: helper boot-id state/discovery failures are degraded, not fatal; generate a fresh boot ID and expose diagnostics.

Readiness after closure:

- User-design blockers are resolved. Implementation must still first update `product-scope.md`, writer docs, API docs, and the SOW pre-implementation gate to match the corrected contract.

## Completed Implementation External Review - 2026-06-17 Round 1

Reviewer votes:

- `glm`: NOT READY TO IMPLEMENT.
- `minimax`: READY TO IMPLEMENT, with one high-priority Go log append ordering fix and non-blocking follow-ups.
- `kimi`: READY TO IMPLEMENT, with minor cleanup recommendations.
- `mimo`: READY TO IMPLEMENT, with minor code-quality recommendations.
- `deepseek`: READY TO IMPLEMENT, with dead-code cleanup recommendation.
- `qwen`: READY TO IMPLEMENT, with minor cleanup recommendations.

Accepted fixes applied:

1. **Missing Go log monotonic could create/open the active file before failing.** Fixed by validating `EntryOptions.MonotonicUsecSet` before `ensureWriter`, retention, rotation, or append.
2. **Rust compatibility methods were behaviorally surprising under the strict contract.** Fixed by deprecating `write_entry`, `write_fields`, and `write_structured` and documenting that they always error without explicit entry timestamps.
3. **`journal_common::time::monotonic_now` documentation implied stronger portable semantics than the function can provide.** Fixed by documenting the actual per-OS `CLOCK_MONOTONIC` behavior and keeping the new `journal_host` helper as the SOW clock source.
4. **Go FreeBSD fallback state path used real UID, not effective UID.** Fixed so the default path matches the state-file contract.
5. **Runtime purity tests did not name the new helper packages/crates.** Fixed by adding `journalhost`, `journal_host`, `journal-host`, and `systemd-journal-sdk-host` to forbidden core-import patterns.
6. **State-directory permissions needed hardening.** Fixed in Rust with best-effort `0700` permissions for newly created Unix parent directories; Go already uses restrictive file mode for the state file and caller-provisioned directory policy remains documented.
7. **Dead Go implementation state remained after strict monotonic migration.** Fixed by removing unused state-backed globals and the dead writer-start monotonic reconstruction path.

Rejected finding:

- `glm` claimed the Windows `QueryUnbiasedInterruptTime` binding is wrong because the API is VOID. This is rejected with official evidence: Microsoft documents `QueryUnbiasedInterruptTime` as returning `BOOL`; the return value is zero only on failure/null parameter. The sleep-excluding behavior also matches the SOW clock contract. Source: <https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttime>.

Non-blocking findings left unchanged:

- Dedicated Rust boot-id-specific error variant: rejected for this SOW as API polish. The strict contract and tests already expose deterministic failure behavior.
- Fallible Go Unix monotonic per-entry API: rejected for this SOW. The helper uses fixed supported OS clock IDs and preserves strict increase with per-provider clamping. A fallible/diagnostic per-entry API needs a separate benchmark/API decision because it changes the hot path.

Round-1 result:

- Review did not expose a remaining design blocker. Real findings were fixed and revalidated. Per the repeat-review rule, a second completed-implementation reviewer batch must run with the same scope plus these fix notes before close.

## Completed Implementation External Review - 2026-06-17 Round 2

Reviewer votes:

- `glm`: READY TO IMPLEMENT.
- `kimi`: READY TO IMPLEMENT, with a non-blocking spec/docs drift finding.
- `mimo`: READY TO IMPLEMENT, with non-blocking polish findings.
- `deepseek`: READY TO IMPLEMENT.
- `minimax`: NOT READY TO IMPLEMENT, due to the Go reopened-empty-file boot-id gap.
- `qwen`: no verdict; the reviewer failed midstream with a LiteLLM socket timeout.

Accepted fixes applied:

1. **Go reopened empty journal could append with zero boot ID.** Fixed by validating resolved entry boot ID in `Writer.prepareEntryOptions`; an empty reopened file now needs either a writer/default boot ID or a per-entry boot ID before append. Focused tests verify rejection without entry mutation and success with explicit per-entry boot ID.
2. **Rust high-level log writer accepted nil UUIDs as explicit IDs.** Fixed by rejecting `Uuid::nil()` in `resolve_machine_id` and `resolve_boot_id`. Focused tests cover nil machine and boot IDs.
3. **Helper package/crate names were under-linked in user docs.** Fixed by naming `github.com/netdata/systemd-journal-sdk/go/journalhost` and `systemd-journal-sdk-host` / `journal_host` in writer/API docs and Rust README.
4. **Spec overstated Rust low-level strict enforcement.** Fixed by stating that Rust `JournalFileOptions::new` is an explicit low-level file-identity/exact-regeneration constructor and that high-level strict log enforcement is in `journal_log_writer::Log`.
5. **Eager-open wording was ambiguous.** Fixed by documenting that eager open may create an empty active file after explicit identity validation, but cannot append without explicit monotonic metadata.

Round-2 result:

- One completed reviewer found a real close blocker. It was fixed and locally revalidated. Because a blocker was fixed and `qwen` did not complete, a third completed-implementation reviewer batch must run with the same scope plus round-1 and round-2 fix notes before close.

## Completed Implementation External Review - 2026-06-17 Round 3

Reviewer votes:

- `glm`: READY TO IMPLEMENT, with low non-blocking cleanup/doc findings.
- `kimi`: READY TO IMPLEMENT, with non-blocking spec and Rust hygiene findings.
- `mimo`: READY TO IMPLEMENT, with non-blocking clock-error handling observations.
- `deepseek`: READY TO IMPLEMENT, with non-blocking observations.
- `qwen`: READY TO IMPLEMENT.
- `minimax`: no final verdict due to 30-minute timeout; partial transcript found no blocker and stated it would vote READY TO IMPLEMENT.

Accepted low-hygiene fixes applied after round 3:

1. **Stale Windows Rust monotonic wording.** Fixed in `product-scope.md`; Windows strict writers do not generate monotonic timestamps, and `journal_host` is the optional helper source.
2. **Rust state temp/lock file descriptors lacked close-on-exec.** Fixed with Unix `O_CLOEXEC` custom flags.
3. **Top-level README omitted the new Rust helper crate from the advanced package list.** Fixed by adding `systemd-journal-sdk-host` / `journal_host`.
4. **Generated Go helper test binaries existed in the worktree.** Removed the untracked generated binaries; source package remains untracked because it is new SOW work.

Round-3 result:

- No completed reviewer found a correctness, security, runtime-purity, portability, or contract blocker. The only timeout was from `minimax`, whose partial review had already verified the round-1/round-2 fixes and did not identify a blocker before timeout.

## Validation

Sensitive data gate:

- `.agents/sow/audit.sh` passed on 2026-06-17 and reported no sensitive-data patterns in durable artifacts. This SOW mentions host identity concepts such as machine ID and boot ID, but tests and durable artifacts use synthetic values except optional-helper runtime code/tests that intentionally probe the local host.

Post-round-1-fix local implementation validation on 2026-06-17:

- Go full suite: `cd go && GOCACHE=$PWD/../.local/go-cache GOMODCACHE=$PWD/../.local/go-mod GOPATH=$PWD/../.local/go-path go test ./...` passed.
- Go helper cross-compile checks: `GOOS=freebsd GOARCH=amd64 go test -c -o ../.local/journalhost-freebsd.test ./journalhost`, `GOOS=darwin GOARCH=amd64 go test -c -o ../.local/journalhost-darwin.test ./journalhost`, and `GOOS=windows GOARCH=amd64 go test -c -o ../.local/journalhost-windows.test ./journalhost` passed.
- Rust host helper: `cd rust && cargo test -p systemd-journal-sdk-host` passed.
- Rust log writer: `cargo test -p systemd-journal-sdk-log-writer` passed.
- Rust log writer with serde API: `cargo test -p systemd-journal-sdk-log-writer --features serde-api` passed.
- Rust core: `cargo test -p systemd-journal-sdk-core` passed.
- Rust formatting and Windows host-helper compile: `cargo fmt --all -- --check && cargo check -p systemd-journal-sdk-host --target x86_64-pc-windows-gnu` passed.
- Docs validation: `python3 tests/docs/check_wiki_docs.py` passed and `python3 tests/docs/verify_examples.py` passed 31/31 verified examples.
- Runtime purity: `python3 -m unittest tests.runtime_purity.test_core_runtime_purity` passed.
- Whitespace: `git diff --check` passed.
- SOW audit: `.agents/sow/audit.sh` passed.

Post-round-3-low-fix local validation on 2026-06-17:

- Rust formatting: `cd rust && cargo fmt --all` passed.
- Rust host helper: `cargo test -p systemd-journal-sdk-host` passed.
- Rust Windows host-helper compile: `cargo check -p systemd-journal-sdk-host --target x86_64-pc-windows-gnu` passed.
- Docs validation: `python3 tests/docs/check_wiki_docs.py` passed and `python3 tests/docs/verify_examples.py` passed 31/31 verified examples.
- Generated-binary cleanup: `git status --short -- go/journalhost.test go/journalhost.test.exe go/journalhost` showed only the new `go/journalhost/` source package remains untracked.
- SOW audit: `.agents/sow/audit.sh` passed.

Post-round-2-fix local implementation validation on 2026-06-17:

- Focused Go writer/log regression checks: `cd go && GOCACHE=$PWD/../.local/go-cache GOMODCACHE=$PWD/../.local/go-mod GOPATH=$PWD/../.local/go-path go test ./journal` passed.
- Docs validation after helper cross-link edits: `python3 tests/docs/check_wiki_docs.py` passed and `python3 tests/docs/verify_examples.py` passed 31/31 verified examples.
- Focused Rust strict-log regression checks: `cd rust && cargo test -p systemd-journal-sdk-log-writer` passed.
- Go full suite: `cd go && GOCACHE=$PWD/../.local/go-cache GOMODCACHE=$PWD/../.local/go-mod GOPATH=$PWD/../.local/go-path go test ./...` passed.
- Go helper cross-compile checks: `GOOS=freebsd GOARCH=amd64 go test -c -o ../.local/journalhost-freebsd.test ./journalhost`, `GOOS=darwin GOARCH=amd64 go test -c -o ../.local/journalhost-darwin.test ./journalhost`, and `GOOS=windows GOARCH=amd64 go test -c -o ../.local/journalhost-windows.test ./journalhost` passed.
- Rust host helper: `cd rust && cargo test -p systemd-journal-sdk-host` passed.
- Rust log writer with serde API: `cargo test -p systemd-journal-sdk-log-writer --features serde-api` passed.
- Rust core: `cargo test -p systemd-journal-sdk-core` passed.
- Rust formatting and Windows host-helper compile: `cargo fmt --all -- --check` passed and `cargo check -p systemd-journal-sdk-host --target x86_64-pc-windows-gnu` passed.
- Runtime purity: `python3 -m unittest tests.runtime_purity.test_core_runtime_purity` passed.
- Whitespace: `git diff --check` passed.

Post-close native runtime validation on 2026-06-17:

- Go local-host helper plus writer/readback smoke passed on Windows 11 over SSH with `MSYSTEM=MSYS`. The temporary cross-compiled Go smoke called `journalhost.Load`, used `MachineGuid`, state-backed boot ID, and `QueryUnbiasedInterruptTime`, verified strictly increasing monotonic values, created a journal with the provider identity, appended one entry with `EntryOptions()`, and read the entry back. Durable artifacts do not record the raw machine ID or boot ID.
- Go local-host helper plus writer/readback smoke passed on macOS over SSH. The temporary cross-compiled Go smoke called `journalhost.Load`, used `gethostuuid`, native `kern.bootsessionuuid`, and `CLOCK_UPTIME_RAW`, verified strictly increasing monotonic values, created a journal with the provider identity, appended one entry with `EntryOptions()`, and read the entry back. Durable artifacts do not record the raw machine ID or boot ID.
- Go local-host helper plus writer/readback smoke passed on FreeBSD 14.1 over SSH. The temporary cross-compiled Go smoke called `journalhost.Load`, used `kern.hostuuid`, native 16-byte `kern.boot_id`, and `CLOCK_UPTIME`, verified strictly increasing monotonic values, created a journal with the provider identity, appended one entry with `EntryOptions()`, and read the entry back. Durable artifacts do not record the raw machine ID or boot ID.
- Cross-compiled Go `journalhost` package test binaries passed on Windows 11, macOS, and FreeBSD 14.1 over SSH.
- Native C probes compiled and ran on Windows 11, macOS, and FreeBSD 14.1 to validate the platform APIs used by the Rust helper code: Windows registry `MachineGuid`, `QueryUnbiasedInterruptTime`, and `GetTickCount64`; macOS `gethostuuid`, `kern.bootsessionuuid`, and `CLOCK_UPTIME_RAW`; FreeBSD `kern.hostuuid`, 16-byte non-zero `kern.boot_id`, and `CLOCK_UPTIME`.
- Rust local-host helper smoke passed on Linux by compiling a temporary repository-local binary under `.local/` and calling `journal_host::load`.
- Temporary Rust toolchains were bootstrapped under `/tmp` on Windows 11, macOS, and FreeBSD 14.1 with `RUSTUP_HOME`, `CARGO_HOME`, and `CARGO_TARGET_DIR` also under `/tmp`; no host-persistent Rust installation was made.
- Native Rust `cargo test -p systemd-journal-sdk-host -- --nocapture` passed on macOS, FreeBSD 14.1, and Windows 11. Each run passed 4 unit tests and 0 doctests using Rust 1.96.0.
- Native Rust helper plus writer/readback smoke passed on macOS. The temporary smoke called `journal_host::load`, used `gethostuuid`, native `kern.bootsessionuuid`, and `CLOCK_UPTIME_RAW`, created a Rust journal log with the provider identity, wrote one entry with `entry_timestamps()`, reopened it through the Rust reader facade, and verified the entry.
- Native Rust helper plus writer/readback smoke passed on FreeBSD 14.1. The temporary smoke called `journal_host::load`, used `kern.hostuuid`, native `kern.boot_id`, and `CLOCK_UPTIME`, created a Rust journal log with the provider identity, wrote one entry with `entry_timestamps()`, reopened it through the Rust reader facade, and verified the entry.
- Native Rust helper plus writer/readback smoke passed on Windows 11. The first broader smoke attempt used MSYS/Cygwin GCC and failed in `zstd-sys` header resolution; rerunning with `cc-rs` and Rust linking forced to `/mingw64/bin/gcc.exe` solved the toolchain issue. The passing smoke called `journal_host::load`, used `MachineGuid`, state-backed boot ID, and `QueryUnbiasedInterruptTime`, created a Rust journal log with the provider identity, wrote one entry with `entry_timestamps()`, reopened it through the Rust reader facade, and verified the entry.
- Native Rust `cargo test -p systemd-journal-sdk-log-writer` passed on macOS: 2 unit tests, 51 integration tests, and 1 doctest.
- Native Rust `cargo test -p systemd-journal-sdk-log-writer` passed on FreeBSD 14.1: 2 unit tests, 51 integration tests, and 1 doctest.
- Native Rust `cargo test -p systemd-journal-sdk-log-writer` passed on Windows 11 with MinGW forced for C compilation/linking: 1 unit test, 51 integration tests, and 1 doctest.
- Non-blocking native Rust warnings observed: macOS and FreeBSD emitted the existing `sigbus.rs` function-pointer-cast warning; Windows emitted the existing unused `mode` warning in `file_mut.rs`. These warnings did not block the tested host-helper, writer, or readback behavior.
- Remaining gap: none for the SOW-0115 Rust host-helper and strict log-writer surface on Linux, Windows 11, macOS, and FreeBSD 14.1. This validation did not run the entire Rust workspace test suite on every non-Linux host; it ran the packages and smoke paths affected by SOW-0115.

External review status:

- Completed-implementation external review round 1 completed on 2026-06-17: 5/6 READY TO IMPLEMENT, 1/6 NOT READY TO IMPLEMENT. All real round-1 findings were fixed and revalidated; one Windows API finding was rejected with official Microsoft evidence.
- Completed-implementation external review round 2 completed on 2026-06-17 with 5 completed verdicts and one qwen transport failure: 4/5 completed READY TO IMPLEMENT and 1/5 completed NOT READY TO IMPLEMENT. The NOT READY finding was real and fixed before round 3.
- Completed-implementation external review round 3 completed on 2026-06-17 with 5 completed READY TO IMPLEMENT verdicts and one minimax timeout. The minimax partial transcript found no blocker and stated it would vote READY TO IMPLEMENT. Round-3 low hygiene findings were fixed or dispositioned and revalidated.

Closeout validation gate:

- Acceptance criteria evidence: Rust and Go writers now require explicit machine ID, boot ID, and generated-entry monotonic timestamp inputs; optional helper packages are separate (`go/journalhost`, `systemd-journal-sdk-host` / `journal_host`); state-backed Windows and FreeBSD fallback behavior is deterministic and degraded on failure; docs/specs describe caller-owned identity selection; runtime-purity tests verify core paths do not import helpers.
- Tests or equivalent validation: Go full suite, Go helper FreeBSD/macOS/Windows compile checks and native smoke, Rust host/log/core tests, Rust host Windows compile check, native Rust host-helper tests on Windows 11/macOS/FreeBSD 14.1, native Rust log-writer tests on Windows 11/macOS/FreeBSD 14.1, docs validation, verified examples, runtime purity, `git diff --check`, and SOW audit passed as recorded above.
- Real-use evidence: repository-local verified examples passed 31/31; Go native host-helper plus writer/readback smoke passed on Windows 11, macOS, and FreeBSD 14.1; native C probes confirmed the Rust helper's OS API sources on Windows 11, macOS, and FreeBSD 14.1; native Rust helper plus writer/readback smoke passed on Windows 11, macOS, and FreeBSD 14.1.
- Reviewer findings: three completed-implementation review rounds were run; all real blockers were fixed. Final completed verdicts were READY TO IMPLEMENT, except one minimax timeout with partial READY/no-blocker transcript.
- Same-failure scan: searches for removed fallback names and helper imports found only expected historical SOW discussion, lock-helper code, or helper internals; no core reader/writer path imports `journalhost`, `journal_host`, or `systemd-journal-sdk-host`.
- Sensitive data gate: `.agents/sow/audit.sh` passed and reported no sensitive-data patterns in durable artifacts. Durable tests/docs use synthetic identifiers.
- Artifact maintenance gate:
  - AGENTS.md: no update needed; the existing Runtime Purity Architecture already covered the helper boundary.
  - Runtime project skills: no update needed; existing orchestration/docs/journal compatibility skills remain accurate for this work.
  - Specs: `.agents/sow/specs/product-scope.md` updated for the strict writer and helper contract.
  - End-user/operator docs: README, docs wiki pages, Go API docs, and Rust README/API docs updated.
  - End-user/operator skills: none exist for this repository surface, so no update was needed.
  - SOW lifecycle: SOW-0115 is marked `completed` and moved to `.agents/sow/done/`; follow-up SOW-0118 tracks release and Netdata integration planning.
  - SOW-status.md: updated for SOW-0115 completion and SOW-0118 pending follow-up.
- Specs update: product-scope now records current behavior.
- Project skills update: no project skill behavior changed.
- End-user/operator docs update: writer/API docs and README files updated with strict anchors and helper package names.
- End-user/operator skills update: no output/reference skills were affected.
- Lessons: recorded below.
- Follow-up mapping: SOW-0118 created and listed in project status.

## Outcome

Completed. Rust and Go now expose strict writer behavior for the three journal anchors and separate optional local-host helper APIs. Core writer/reader runtime purity is preserved, tests/docs/specs are updated, and release plus Netdata integration planning is tracked by SOW-0118.

## Lessons Extracted

- OS boot identity research needs primary-source verification; reviewers disagreed on FreeBSD and Windows details until native sources and official docs were checked directly.
- Strict writer contracts need empty-file/reopen tests, not only create and first-append tests.
- Optional helper names must appear in user-facing docs, not only specs, otherwise downstream consumers cannot discover the intended boundary.

## Followup

- Tracked by `.agents/sow/pending/SOW-0118-20260617-host-helper-release-and-netdata-integration.md`: release the SDK version containing SOW-0115 and plan Netdata-side Rust NetFlow / Go SNMP traps adoption with read-only Netdata inspection first.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
