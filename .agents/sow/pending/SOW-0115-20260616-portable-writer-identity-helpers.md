# SOW-0115 - Portable writer identity & monotonic helpers (machine-id, boot-id, monotonic clock)

## Status

Status: open

Sub-state: blocked-on-user-decisions - user accepted a strict writer contract with no fallback for the three journal anchors, a separate optional helper API/package for callers that want local-host values, the FreeBSD boot-id state strategy, Rust/Go product scope after SOW-0116, and the hard native-only/no-subprocess policy. The hardened SOW repeat external review on 2026-06-16 returned 6/7 NOT READY TO IMPLEMENT votes and 1/7 READY vote before the language-scope reduction. The SOW must resolve non-Linux boot-id strategy, state-file locking/security, Rust helper relocation/clock semantics, and breaking-change migration before implementation.

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
- Only Linux provides a kernel-managed per-boot UUID (`/proc/sys/kernel/random/boot_id`). FreeBSD, macOS, and Windows have no native per-boot UUID; a portable boot-id must be **synthesized deterministically** so all processes within one boot agree.
- The SDK product scope is Rust and Go after SOW-0116. Historical Python/Node parity work no longer creates implementation requirements for this SOW.
- Review-discovered in-tree fact: Rust already exposes partial host identity helpers in `rust/src/crates/journal-common/src/system.rs` (`load_machine_id`, `load_boot_id`) and a monotonic helper in `rust/src/crates/journal-common/src/time.rs` (`monotonic_now`). This corrects the original broad wording about SDK helper absence. The Rust helpers are partial and not aligned with this SOW's intended final contract: Windows identity returns unsupported, macOS/FreeBSD boot-id is stateless `kern.boottime` synthesis, and Unix monotonic uses `CLOCK_MONOTONIC` on all Unix targets.
- Review-discovered in-tree fact: both product writer families already generate a default monotonic value when the caller omits one. Go uses writer-start-relative `time.Now().Sub(w.started)` in `go/journal/writer.go`; Rust uses `journal_common::monotonic_now()` plus clamp in `rust/src/crates/journal-log-writer/src/log/mod.rs`. The opt-in helper must reconcile with these existing defaults instead of pretending monotonic generation is greenfield.

Inferences:

- "As part of the writer API" means the SDK exposes an adjacent public helper API/package for callers that want local-host values, while the core writer itself becomes strict about the three anchors. The writer remains a pure file-format writer: it accepts caller-provided machine-id, boot-id, and monotonic timestamp values, and it must not silently generate local-host identity or fallback monotonic values.
- A boot-anchored monotonic clock source is available on all four platforms, but the correct source is OS-specific: Linux uses `CLOCK_MONOTONIC`; FreeBSD should prefer `CLOCK_UPTIME` for Linux-like suspend-excluding semantics; macOS should prefer `CLOCK_UPTIME_RAW`/equivalent; Windows should prefer unbiased interrupt time. Per-entry values must not be derived from wall-clock `now - boot_time`; wall-clock jumps would damage accuracy even if ordering is later clamped.

Unknowns:

- Repeat external review exposed new user-facing decisions after Decision 6. The SOW is blocked on those decisions before the Pre-Implementation Gate can become ready.

### Acceptance Criteria

- The core writer APIs in Rust and Go require explicit caller-provided values for `_MACHINE_ID`, `_BOOT_ID`, and generated-entry `__MONOTONIC_TIMESTAMP`; they no longer silently synthesize or fallback for those three anchors. Verified by API tests that missing values fail fast with documented errors.
- A separate optional helper API/package returns a valid 128-bit `_MACHINE_ID`, `_BOOT_ID`, and boot-anchored monotonic timestamp source for local-host events on Linux, FreeBSD, macOS, and Windows, using native APIs only (no subprocess). Verified by per-platform unit tests and (where CI runners exist) real-host smoke tests.
- Within a single OS boot, repeated helper invocations and independent processes return a byte-identical `_BOOT_ID`; across a simulated reboot the value changes. Verified by deterministic tests that inject the boot-time source.
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
| FreeBSD | `kern.hostuuid` sysctl (syscall) | **state-backed synthesis** using a locked helper state file under `/var/run` by default, caller-overridable; see Decision 2 | `clock_gettime(CLOCK_UPTIME)`; `_FAST` only as explicit approximate mode if accepted by evidence |
| macOS | `gethostuuid()` C API, or native platform UUID API | **stateless synthesis** from machine identity plus native reboot marker unless implementation evidence proves state is also needed | `CLOCK_UPTIME_RAW` / native uptime raw API; `_APPROX` only as explicit approximate mode if accepted by evidence |
| Windows | registry `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` or documented native machine UUID fallback | **stateless synthesis** from machine identity plus native reboot marker unless implementation evidence proves state is also needed | `QueryUnbiasedInterruptTime`; `QueryUnbiasedInterruptTimePrecise` only when precision beats call cost |

Boot-id synthesis (the central correctness point): outside Linux there is no kernel per-boot UUID. The helper must synthesize a deterministic 128-bit UUID so every process in the same boot computes the same value and it changes only on reboot. Stateless synthesis from a machine-id plus an OS reboot marker is attractive, but FreeBSD needs state-backed synthesis because `kern.boottime` is an estimate that may move during one boot. The accepted FreeBSD strategy stores one locked two-field state file under `/var/run` by default, with caller override, and never rewrites it for same-boot drift. Per-entry monotonic timestamps must not be derived from wall-clock `now - boot_time`; they must use a native monotonic/uptime source plus writer-owned clamping.

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
- Helper tests may probe the real host only inside explicitly named optional-helper tests. Core writer tests continue to use synthetic identities.

Reference API shape to implement:

| Language | Strict writer change | Helper package/module | Required helper shape |
|---|---|---|---|
| Go | `journal.Create` requires non-zero `Options.MachineID` and `Options.BootID`; `Writer.Append`/`AppendRaw` require `EntryOptions.MonotonicUsecSet` or a non-zero `MonotonicUsec`; `LogIdentityAuto` stops being the default silent identity generator. | New sibling package `github.com/netdata/systemd-journal-sdk/go/journalhost` so core `journal` does not import host probing. | `journalhost.Load(opts) (Provider, error)`; `Provider.MachineID() journal.UUID`; `Provider.BootID() journal.UUID`; `Provider.EntryOptions() journal.EntryOptions`; `Provider.MonotonicUsec() uint64`; FreeBSD `opts.StatePath`, `opts.MinRebootTime`. |
| Rust | `journal-core` remains explicit: `JournalFileOptions::new(machine_id, boot_id, seqnum_id)` and append APIs already take monotonic values. `journal-log-writer` changes default identity mode to strict and stops generating SDK-local machine/boot IDs silently. | New workspace crate `systemd-journal-sdk-host` with lib name `journal_host`; existing `journal-common::system::{load_machine_id, load_boot_id}` becomes deprecated compatibility surface or moves behind a feature that core/log writers do not import. | `LocalJournalProvider::load(options) -> Result<Self>`; `machine_id() -> Uuid`; `boot_id() -> Uuid`; `entry_timestamps() -> EntryTimestamps`; `monotonic_usec() -> u64`; FreeBSD state options. |
FreeBSD state-file contract:

- Default state data path: `/var/run/systemd-journal-sdk/bootid.$UID.state`, where `$UID` is the numeric effective UID. Callers can override the path. Packaged consumers that need cross-user sharing must provision the directory and permissions explicitly.
- State data file contains exactly two logical fields as ASCII lines:
  - `last_estimated_boottime=<decimal unix microseconds>`
  - `last_boot_id=<32 lowercase hex UUID bytes>`
- The helper must create new state files with `0600` permissions unless the caller points to an already-provisioned path.
- Locking must serialize helper initialization across processes before the file is read or written. The implementation may use a sidecar lock file if required for atomic replace semantics, but the data file format remains the two fields above and the SOW/review must record the exact locking primitive per language.
- Write policy:
  - Missing state: generate a new UUID, write current estimated boot time and boot-id.
  - Corrupt/unparseable state: preserve a best-effort `.corrupt` copy when safe, generate a new UUID, write a clean state.
  - Same boot: do not rewrite, refresh, or "improve" the estimated boot time.
  - New boot: write the new estimated boot time and new boot-id.
- New boot test: `estimated_boottime > last_estimated_boottime + 30s`, where `estimated_boottime = realtime_now - uptime_now`.
- Reboot within 30 seconds is explicitly accepted as indistinguishable for FreeBSD in this design. The SOW records this as a tolerated edge case from the user decision.

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

- **Correctness of synthesized boot-id** - an unstable or per-process boot-id breaks reader ordering and dedup within a boot. Mitigation: FreeBSD uses the accepted locked state-backed strategy; macOS/Windows stateless synthesis must document native reboot-marker stability or return for a new user decision if implementation evidence shows state is also needed.
- **Purity regression** - leaking host probing into core paths would violate `AGENTS.md:75-117`. Mitigated by keeping all probing in explicitly named optional files and reusing the project's purity grep/tests.
- **Monotonic vs boot-id epoch mismatch** - the monotonic reading must be anchored to the same boot as the boot-id. On all supported OS platforms the chosen monotonic source ("time since boot") is consistent with a boot-time-derived boot-id; document the contract so callers don't mix sources.
- **Wall-clock-derived monotonic drift** - deriving per-entry monotonic timestamps from realtime `now - cached_boot_time` would be vulnerable to NTP steps, admin clock changes, VM time corrections, and resume behavior. Mitigated by using native monotonic/uptime clocks for the hot path and clamping generated same-boot values to strictly increase.
- **Breaking writer contract change** - Go and Rust already generate monotonic timestamps when the caller omits them. Some existing paths are process-relative or wall-clock-derived. The accepted design removes these fallbacks for the three journal anchors, so callers/tests/docs must migrate to explicit values or the optional helper API/package.
- **Rust/Go drift** - if implemented per language without a shared contract/spec, bindings could diverge. Mitigated by writing the contract into `product-scope.md` first.
- **macOS/Windows CI** - real-host verification may be limited by available runners; fall back to injected-source deterministic tests + document the gap.

## Pre-Implementation Gate

Status: blocked-on-user-decisions-after-repeat-review

Problem / root-cause model:

- The writer surfaces can write journal bytes anywhere, but current defaults blur ownership of the three mandatory anchors: Go generates fallback monotonic values, Rust has partial helper behavior, and neither product language exposes one portable local-host helper contract. For Netdata, that ambiguity is wrong: SNMP traps may describe a remote device, NetFlow may use synthetic or fixed values, and local collector identity is only one caller choice. Evidence: Go writer fallback lines listed above, Rust helper files listed above, `AGENTS.md:75-117`, downstream Linux gate at netdata `collector.go:162-163`.

Evidence reviewed:

- SDK: `AGENTS.md:75-117`, `.agents/sow/specs/product-scope.md:62-106`, `go/journal/writer_init.go`, `go/journal/lock_owner_*.go` @ `a2361ab`, `go/journal/boot_id_*.go` @ `a2361ab`.
- Downstream: netdata `snmp_traps` `collector.go`, `journal_writer.go`, `monotonic_linux.go`, `monotonic_fallback.go` (read-only, in a separate repo; not modified by this SOW).
- OSS references: `shirou/gopsutil @ df9e25f20` (per-OS HostID/BootTime); `elastic/go-sysinfo @ efea16f` (per-OS machine-id).

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

1. Write the strict writer and helper contract into `product-scope.md` (mandatory explicit anchors, helper package boundary, machine-id/boot-id/monotonic sources per platform, boot-id synthesis rule, native-only constraint, monotonic epoch contract).
2. Reconcile existing in-tree helpers and defaults: Rust `journal-common` identity/time helpers, Go writer-start fallback, and Rust log-writer monotonic generation. Remove or gate fallbacks so the writer fails fast when the three anchors are missing.
3. Implement Rust and Go helper APIs/packages against one shared contract: machine-id, synthesized boot-id (injectable reboot marker/state source), and a fast local-host monotonic timestamp source that callers can use when local-host values are appropriate.
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
  - Cross-process FreeBSD state tests prove serialized initialization and byte-identical same-boot `boot_id`.
  - Corrupt FreeBSD state tests prove recovery without crashing and without world-writable files.
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

Open decisions:

- Decision 8 - Non-Linux boot-id strategy. Repeat reviewers agreed macOS and Windows boot-id synthesis lacks a named stable native reboot marker and fallback. The user must choose whether to generalize state-backed synthesis beyond FreeBSD or accept an explicitly documented best-effort/native-marker path after proof.
- Decision 9 - Strict writer migration boundary. Repeat reviewers agreed the SOW must decide how existing `LogIdentityAuto`/fallback behavior is removed or retained, and whether low-level fixture/raw writer paths keep an explicit escape hatch for byte-exact/corrupt-test construction.
- Decision 10 - State-file locking/security and Rust helper relocation. Repeat reviewers agreed the SOW must record exact locking and symlink/atomic-write rules and choose whether Rust host-probing helpers move fully out of `journal-common` or remain behind compatibility/deprecation features.

## Implications And Decisions

User decisions required (numbered for fast reply, e.g. "1: B, 2: A ..."). Options carry pros/cons/risks; each has a recommendation.

### Decision 1 - API placement

- **User decision 2026-06-16: 1C accepted, later refined by Decision 6A.**
- **1A. Separate public helper only; caller passes everything per append.** Pros: clean purity boundary. Cons/RISK: every caller must own monotonic state and clamping, which invites repeated downstream bugs. Acceptable only as a low-level escape hatch.
- **1B. Auto-discover inside the core writer constructor.** Pros: zero caller effort. Cons/RISK: directly violates Layer-1 purity (core must not probe host identity), breaks the "synthetic identities in tests" rule, and contradicts the spec; high architectural debt. Rejected.
- **1C. Public opt-in helper outside the core writer (ACCEPTED, refined).** Caller explicitly invokes the host helper when local-host values are desired, then passes explicit values to the writer. Decision 6A supersedes the earlier wrapper-owned-state framing: the helper may own local-host discovery and monotonic provider state, but the writer boundary stays explicit and strict. Pros: honors Runtime Purity Architecture (`AGENTS.md:75-117`) and `product-scope.md:62-80`; core stays testable with synthetic identities; callers can choose local-host, remote-device, fixed, synthetic, or auto-increment values. Cons: callers must wire explicit values. Risk: low if helper names and errors make the ownership obvious.
- Reasoning: 1C is the only option that satisfies both constraints: host discovery remains explicit, and the writer does not silently choose an event identity source.

### Decision 2 - Non-Linux boot-id strategy

- **User decision 2026-06-16: 2A accepted for FreeBSD, with a minimal locked state file.**
- **2A. State-backed synthesis under caller-approved runtime storage (ACCEPTED for FreeBSD, long-term-best).** Derive or generate one UUID for the current OS boot, persist it with the best available native reboot marker, and reuse it while the marker still identifies the same boot. Pros: robust same-boot cross-process identity even when a platform's stateless boot marker can move or has weak precision; keeps core writer pure because only the explicit helper writes state. Cons: requires the caller to provide/approve a state path and locking protocol for the helper state file. Risk: storage permissions and stale-state recovery must be tested.
  - FreeBSD default state path: under `/var/run` using an SDK/app-specific filename; API consumers can override the path.
  - State file contents: exactly `last_estimated_boottime` and `last_boot_id`.
  - Locking: helper initialization opens and locks the state file so concurrent processes serialize.
  - Estimated boot time: `estimated_boottime = realtime_now - uptime_now`.
  - New boot rule: `estimated_boottime > last_estimated_boottime + 30s`.
  - Write policy: write only on missing/corrupt state or detected new boot; never refresh or rewrite on same-boot drift.
  - Permission policy: no world-writable `0666` state file; caller/package owns the `/var/run` path and permissions, and non-default deployments pass an explicit override path.
- **2B. Stateless deterministic synthesis from machine-id + native reboot marker.** Pros: no state file, no helper-side writes, simpler deployment. Cons/RISK: only safe where the marker is proven stable for one boot; FreeBSD `kern.boottime` is documented as recomputed after wall-clock changes/resume, and macOS/Windows marker semantics need explicit proof. Acceptable only as a documented best-effort mode or for OSes where evidence proves stability.
- **2C. Random UUID generated at helper init.** Pros: trivial. Cons/RISK: differs per process and per restart within the same boot - breaks multi-writer and restart ordering under one boot. Rejected.
- Reasoning: journal readers correlate monotonic timestamps within a boot-id; only a stable per-boot value is correct. FreeBSD does not provide Linux's kernel boot UUID, and its `kern.boottime` can drift during one boot, so a small locked state file is the accepted FreeBSD strategy.

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

## Plan

1. **Spec first.** Update `product-scope.md` with the strict writer contract, helper package boundary, platform source table, and FreeBSD state-file contract.
2. **Strict writer contract.** Remove silent `machine_id`, `boot_id`, and `monotonic_usec` fallbacks in Rust and Go writer/log layers; keep file-id/seqnum-id defaults and realtime fallback scoped as documented.
3. **Helper package/module skeleton.** Add the separate helper surfaces:
   - Go: `go/journalhost`.
   - Rust: `systemd-journal-sdk-host` / `journal_host`.
4. **FreeBSD state-backed boot-id.** Implement the locked two-field state file with injected-source tests before wiring it into platform-specific helpers.
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
- User scope constraint recorded: Rust and Go helpers are mandatory in this SOW because Netdata has immediate Rust NetFlow and Go `snmp_traps` consumers.
- Earlier all-binding scope was superseded by SOW-0116; Rust and Go remain mandatory product targets.
- User decision recorded: hard native-only/no-subprocess policy is confirmed. No subprocess fallback and no Go CGO may be introduced without returning to the user with evidence and a new decision.
- User decision recorded: the writer must be strict and OS-agnostic. The three anchors (`_MACHINE_ID`, `_BOOT_ID`, and generated-entry `__MONOTONIC_TIMESTAMP`) are mandatory caller-provided values; the SDK will provide a separate optional helper API/package for callers that intentionally want local-host values.
- SOW hardening after Decision 6A: added the concrete strict-writer contract, reference API/package shape, FreeBSD state-file format and locking contract, performance benchmark contract, and validation matrix. Retired-language portions were later superseded by SOW-0116.
- User decision recorded by SOW-0116: Python and Node.js are retired product targets. This SOW's implementation scope is Rust and Go only.
- External review batch requested by user and run read-only with `claude`, `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen`. Result: 7/7 reviewers voted NOT READY TO IMPLEMENT. Review-discovered blockers and accepted/disputed dispositions are recorded below.
- Repeat external review batch requested by user and run read-only against the hardened SOW with `claude`, `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen`. Result: 6/7 reviewers voted NOT READY TO IMPLEMENT; `deepseek` voted READY TO IMPLEMENT with non-blocking issues. The SOW remains open in `pending/` and blocked on user decisions.

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
4. **Boot-id synthesis needs implementation-level detailing.** Addressed for FreeBSD by the default path, two-field format, permissions, locking, write policy, corrupt-state recovery, and 30-second rule above. macOS/Windows are still subject to implementation evidence; if native stateless markers are not proven stable, the implementer must return to the user before extending state-backed synthesis beyond the accepted FreeBSD scope.
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

- NOT READY TO IMPLEMENT. The SOW stays `open` in `pending/` with Pre-Implementation Gate `blocked-on-user-decisions-after-repeat-review`.

## Validation

Repeat external review of the hardened SOW completed on 2026-06-16: 6/7 NOT READY TO IMPLEMENT, 1/7 READY TO IMPLEMENT. Validation is blocked pending user decisions and SOW hardening.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

- Downstream: once the Go helper lands, open a netdata-repo SOW to drop the `snmp_traps` Linux gate (`collector.go:162-163`) and adopt the helper; consider the same for OTEL logs.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
