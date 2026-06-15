# SOW Status

Last updated: 2026-06-15

This root file is a short convenience index. The canonical detailed SOW ledger
is `.agents/sow/SOW-status.md`; if summaries differ, the canonical ledger wins.

## Current

- `SOW-0111-20260614-cross-language-reader-api-parity.md` - in-progress. Enforces Rust-source-of-truth reader parity: hide Rust whole-file mmap from public consumers, make Go/Python read-at internal/test-only where practical and prominently non-production, split Node.js native mmap behind an optional package or equivalent explicit opt-in boundary, and prove no production reader/verify path whole-maps or whole-loads journal files.
- `SOW-0009-20260523-benchmark-profile-optimize.md` - paused umbrella. Writer and reader performance work is split into focused child SOWs.

## Pending
- `SOW-0106-20260611-python-node-docs-and-verified-examples.md` - open. Activates after Python/Node parity closure. Adds Python-API/Node-API wiki pages and Python/Node columns to shared pages; extends the verified-examples harness to all four languages.
- `SOW-0066-20260530-v1-release-and-registry-publication.md` - open. Publish the final stable `v1.0.0` SDK across language registries and versioned module tags after compatibility, portability, real-corpus validation, parity closure, and release checks pass.
- `SOW-0048-20260528-netdata-otel-writer-sdk-integration.md` - open. Integrate the SDK compact-default structured writer into Netdata OTEL logs ingestion after the inventory and writer gates are accepted.
- `SOW-0049-20260528-netdata-reader-plugin-sdk-integration.md` - open. Integrate SDK reader/facade paths into Netdata OTEL signal viewer, no-libsystemd systemd-journal plugin mode, and static packaging after reader gates are accepted.
- `SOW-0050-20260528-netdata-vendored-journal-removal.md` - open. Remove old Netdata vendored journal code only after all Netdata component integrations are complete and fresh searches prove no production references remain.
- `SOW-0094-20260606-rust-explorer-lazy-compressed-field-inference.md` - open. Deferred Rust Explorer optimization experiment for compressed DATA field inference and delayed decompression.
- `SOW-0097-20260607-go-codacy-metric-debt-refactor.md` - open. Follow-up from the Codacy Rust/Go metrics audit for Go production file-size/ownership and duplication reduction.
- `SOW-0098-20260607-rust-legacy-core-duplication-debt.md` - open. Follow-up from the Codacy Rust/Go metrics audit for real Rust `jf`/`journal-core` duplication reduction.
## Done

- `SOW-0112-20260615-netdata-sampling-contract-clarification.md` - completed. Records the SDK sampling and slice contract: data-only non-delta requests stay exact and unsampled, data-only delta analysis may use sampling while returned rows remain exact, `slice:false` fallback semantics are outside the SDK contract, and current Rust/Go implementation needs no change.
- `SOW-0047-20260528-netdata-netflow-sdk-integration.md` - completed. Read-only verification of `ktsaou/netdata @ 36050079cfa9` showed the NetFlow plugin uses published `systemd-journal-sdk-*` crates at `0.7.0` for writer, reader/query, and facet paths; no Netdata build/test was run in this SDK closeout because that would write outside this repository.
- `SOW-0110-20260614-v0-7-0-release.md` - completed. Rust crates were published to crates.io at `0.7.0`, `master` was pushed through release commit `556e79e1e9eabab84becb5f6d0658b6f39ba7075`, and annotated tags `v0.7.0` plus `go/v0.7.0` were pushed; both peel to the same release commit. Go tests, wiki validation, verified examples, Rust package dry-runs, `git diff --check`, and SOW audit passed.
- `SOW-0109-20260614-python-node-netdata-edge-parity.md` - completed. Python and Node now match the Rust reference for the remaining scoped Netdata edge parity gaps discovered after SOW-0107: Python `data_only && delta` keeps analysis enabled, Python/Node expose and wire candidate-row callbacks, and Node no longer drops Explorer rows only because `commitRealtime` is zero. Focused tests, full Python/Node suites, Netdata function comparators, `git diff --check`, SOW audit, and all six reviewer votes passed.
- `SOW-0105-20260611-node-explorer-netdata-parity.md` - completed after reconciliation. The Node Explorer API, Netdata function API, stdin wrapper, source selector labels, facade unique-values visitor, and TypeScript definitions landed in SOW-0105; the remaining review-discovered parity gaps were completed by SOW-0107 and SOW-0109 before closure.
- `SOW-0107-20260613-python-node-explorer-sampling-engine.md` - completed. Python and Node now match the Rust Explorer/Netdata behavior for the scoped parity gaps: row-level sampling decision/estimation, Python Netdata FTS, Python `PRIORITY` facet sorting, Python/Node Index Compare validation, and O(1) indexed row collection. Local validation, four-peer high-row comparator evidence, `git diff --check`, and six production-grade reviewer votes passed; remaining reviewer-discovered edge parity items are tracked by SOW-0109.
- `SOW-0108-20260614-cross-language-reader-window-accessor.md` - completed after regression repair. Rust and Go public file-path verification APIs now use bounded reader-backed byte sources instead of materializing whole journal files; Go verifier source slices return owned buffers to avoid mmap/window aliasing hazards. Focused Rust/Go tiny-window sealed verifier tests, `go test ./...`, affected Rust package tests, the shared verify matrix, wiki docs validation, `git diff --check`, SOW audit, and all six reviewer votes passed.
- `SOW-0104-20260611-python-explorer-netdata-parity.md` - completed. The Python SDK carries the full Rust feature surface (Explorer, Netdata logs function API with source selector labels and the complete tail/delta/sampling contract, stdin wrapper, `python/pyproject.toml` metadata, facade unique-values visitor). Parity proven by three-peer content comparison against the Rust wrapper and the installed Netdata plugin: 10/10 one-shot fixtures on the live journal and 5/5 stateful sequences on a frozen fixture; 5/5 `PRODUCTION GRADE: YES` reviewer verdicts in one round.
- `SOW-0103-20260611-docs-api-perception-and-verified-examples.md` - completed. Consumer wiki restructured around API-perception decision paths with a new `Journalctl-CLI` page; all 31 Rust/Go wiki examples are machine-verified against synthetic fixtures by `tests/docs/verify_examples.py`, enforced by the extended wiki validator and the new `docs-examples.yml` workflow; new `project-docs-authoring` skill; reviewer round 3 returned 5/5 `PRODUCTION GRADE: YES`.
- `SOW-0065-20260530-parallel-language-parity-closure.md` - closed without implementation on 2026-06-11, superseded by the docs-and-parity program SOW-0103 through SOW-0106 after all prerequisites completed and Go parity was already delivered by SOW-0095/SOW-0102; the user chose sequential SOWs with external implementer `llm-netdata-cloud/minimax-m3-coder` and the other `llm-netdata-cloud` pool models as read-only reviewers.
- `SOW-0102-20260611-netdata-function-source-selector-labels.md` - completed. Rust and Go Netdata function configs now expose source selector name/help metadata for the stable `__logs_sources` wire id, preserving `Journal Sources` defaults while allowing consumers such as SNMP traps to show domain wording like `Trap Jobs`; focused tests passed, docs/specs were updated, all six approved reviewers returned `PRODUCTION GRADE`, Rust crates were published to crates.io at `0.6.4`, and release tags are `v0.6.4` plus `go/v0.6.4`.
- `SOW-0101-20260609-netdata-function-stateful-equivalence.md` - completed. Added stateful SDK-wrapper versus installed Netdata `systemd-journal.plugin` side-by-side tests for anchors, forward/backward paging, tail 304 behavior, filtered tail empty-200 behavior, and delta facets/histograms; final validation passed 10/10 one-shot request fixtures plus all five stateful sequences. Rust crates were published to crates.io at `0.6.3`, and release tags are `v0.6.3` plus `go/v0.6.3`.
- `SOW-0093-20260605-netdata-function-boundary-reader-comparison.md` - completed after tail-anchor regression repair. Rust and Go now use libnetdata-compatible tail stop-anchor semantics, backward page anchors are exclusive, tail no-change returns `304`, focused paging/tail/delta contract tests pass, five available reviewers returned `PRODUCTION GRADE`, Kimi was unavailable due quota, Rust crates were published to crates.io at `0.6.2`, and release tags are `v0.6.2` plus `go/v0.6.2`.
- `SOW-0100-20260608-consumer-docs-github-wiki.md` - completed after regression repair. GitHub wiki navigation now uses `[[Target|Label]]` wiki links, the wiki has professional API overview plus Rust and Go language guides with examples, and the docs validator rejects production `*.md` wiki links while allowing fenced anti-pattern examples.
- `SOW-0099-20260608-rust-crates-io-publication.md` - completed. Rust SDK packages were published to crates.io at `0.6.0` under `systemd-journal-sdk` plus project-prefixed internal package names; release tags are created on the SOW close commit.
- `SOW-0096-20260607-codacy-metrics-and-coverage-hygiene.md` - completed. Go and Rust coverage reports now remove test/test-harness paths before Codacy upload, the Rust/Go Codacy metrics audit is committed, GitHub code scanning has zero open alerts on final implementation commit `7e3d3e5d`, Codacy reports `issuesCount = 0`, coverage `73%`, complexity `46%`, and duplication `30%`; remaining production metric debt is tracked by SOW-0097 and SOW-0098.
- `SOW-0084-20260602-code-scanning-and-codacy-gate.md` - completed after regression repair. GitHub CodeQL alert `3341` is closed on head `1d7006ae`; GitHub code scanning has zero open alerts; Codacy Cloud reports `issuesCount = 0` and `codacy issues` returns zero issues on the same head.
- `SOW-0095-20260607-go-explorer-netdata-parity.md`
- `SOW-0075-20260601-vm-historical-systemd-validation.md`
- `SOW-0076-20260601-independent-selective-real-corpus-verification.md`
- `SOW-0064-20260530-real-world-journal-corpus-evaluation.md`
- `SOW-0055-20260529-rust-seek-cursor-systemd-parity.md`
- `SOW-0026-20260526-netdata-sdk-integration.md`
- `SOW-0063-20260530-cross-platform-portability.md`
- `SOW-0071-20260530-runtime-purity-and-optional-platform-services.md`
- `SOW-0067-20260530-go-cross-platform-portability.md`
- `SOW-0068-20260530-rust-cross-platform-portability.md`
- `SOW-0069-20260530-python-cross-platform-portability.md`
- `SOW-0070-20260530-node-cross-platform-portability.md`
- `SOW-0072-20260530-dependency-and-package-hygiene.md`
- `SOW-0062-20260530-rust-go-writer-absolute-performance.md`
- `SOW-0061-20260529-cross-language-row-scoped-facade-lifetime.md`
- `SOW-0060-20260529-rust-reader-absolute-hot-path-profiling.md`
- `SOW-0059-20260529-standard-benchmark-reporting.md`
- `SOW-0058-20260529-rust-data-header-fast-path.md`
- `SOW-0057-20260529-rust-live-whole-file-mmap-reader-option.md`
- `SOW-0035-20260527-derived-rotation-policy.md`
- `SOW-0034-20260526-file-backed-journalctl-query-parity.md`
- `SOW-0033-20260526-full-verification-parity.md`
- `SOW-0032-20260526-live-feature-compatibility-matrix.md`
- `SOW-0031-20260526-compressed-compact-structural-parity.md`
- `SOW-0030-20260526-monotonic-writer-validity.md`
- `SOW-0029-20260526-compression-threshold-parity.md`
- `SOW-0001-20260523-project-bootstrap-and-orchestration.md`
- `SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `SOW-0004-20260523-rust-sdk-and-journalctl.md`
- `SOW-0005-20260523-go-sdk-and-journalctl.md`
- `SOW-0006-20260523-node-sdk-and-journalctl.md`
- `SOW-0007-20260523-python-sdk-and-journalctl.md`
- `SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- `SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `SOW-0012-20260523-go-writer-binary-fields.md`
- `SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- `SOW-0014-20260524-deterministic-ingestion-dataset.md`
- `SOW-0015-20260524-deterministic-ingesters.md`
- `SOW-0016-20260524-byte-identical-writer-compatibility.md`
- `SOW-0017-20260524-xz-lz4-data-writing.md`
- `SOW-0018-20260524-compact-journal-format.md`
- `SOW-0019-20260524-forward-secure-sealing.md`
- `SOW-0020-20260524-directory-traversal-parity.md`
- `SOW-0021-20260524-nodejs-xz-data-compression.md`
- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `SOW-0023-20260525-netdata-ingestion-writer-api.md`
- `SOW-0024-20260526-mixed-format-directory-readers.md`
- `SOW-0025-20260526-retention-enforcement-on-open.md`
- `SOW-0027-20260526-netdata-reader-api-and-jf-facade.md`
- `SOW-0028-20260526-historical-header-parsing-parity.md`

## Notes

- The deterministic dataset must separate accepted rows from expected rejection cases.
- SOW-0015 produced deterministic ingesters for systemd C, Rust, Go, Node.js, and Python.
- SOW-0016 consumed the deterministic ingester outputs and is completed.
- SOW-0016 validation shows byte-for-byte identity for the accepted uncompressed corpus across online, offline, and archived final states, including exact DATA hash-chain depth parity, plus passing closed-file, binary, live, and zstd compression interoperability matrices.
- SOW-0017 completed xz/lz4 DATA writing for Rust/Go/Python and lz4 DATA writing for Node.js, with Node.js xz split to SOW-0021.
- SOW-0021 completed Node.js xz DATA reader/writer support through `node-liblzma@5.0.1` using the WASM-only runtime path accepted by user decision option B.
- SOW-0018 completed compact journal support. Every writer exposes explicit regular/compact output selection while regular remains the default. `run_compact_matrix.py` passes 56/56 for each compression mode (`none`, `zstd`, `xz`, `lz4`) across Go, Rust, Node.js, Python, stock journalctl, and stock libsystemd on systemd 260.1-2-manjaro.
- SOW-0019 Phase 2A added pure FSPRG primitives and vector tests in Rust, Go, Node.js, and Python. The primitives match committed systemd v260.1 vectors.
- SOW-0019 Phase 2B added unsealed journal verification APIs (`VerifyFile`, `verify_file`, etc.) in all four languages with controlled error types (`VerificationError`). The conformance case `journal-verify-corruption-detection` now produces real PASS/FAIL behavior instead of adapter skips.
- SOW-0019 Phase 3 added file-backed journalctl `--verify`, existing `--verify-only`, and `--verify-key` behavior in Rust, Go, Node.js, and Python. The rewrites parse `--verify-key` before verification, match stock invalid-key behavior on repo-local files, verify unsealed files through Phase 2B APIs, and follow symlinks to regular journal files during directory verification.
- SOW-0019 Phase 4 added sealed journal writers in Rust, Go, Node.js, and Python with deterministic test keys and configurable sealing intervals. Stock `journalctl --verify --verify-key` validates generated sealed files.
- SOW-0019 Phase 5 added sealed TAG/HMAC verification APIs and file-backed journalctl `--verify-key` validation in Rust, Go, Node.js, and Python. The shared `journal-verify-sealed` adapter case now runs real behavior in every language.
- SOW-0009 was originally sequenced last. The 2026-05-26 SNMP traps performance report made it a critical Netdata integration gate. Later focused writer and reader performance SOWs completed the hot-path work, and SOW-0026 completed the Netdata integration inventory/cut plan.
- SOW-0025 completed open-time retention enforcement for Rust, Go, Node.js, and Python high-level directory writers. Eager/existing-active open enforces during construction; lazy archived-only construction remains side-effect-free until first append opens the active file, then retention runs before the first entry is written.
- SOW-0027 completed the accepted file-backed Netdata `jf`/libsystemd-like reader facade across Rust, Go, Node.js, and Python, including open file/directory/files, close, seek head/tail/realtime/cursor, next/previous/skip, match groups, current-entry data enumeration, field enumeration, unique enumeration, realtime/monotonic/seqnum/cursor metadata, boot listing, and binary/repeated value support. SOW-0026 completed the Netdata integration inventory and cut plan; component integrations remain tracked by SOW-0047 through SOW-0050.
- SOW-0020 completed directory traversal parity for SDK readers and file-backed `journalctl --directory` across Rust, Go, Node.js, and Python. `run_directory_matrix.py` passes against stock `journalctl` from systemd 260.1 and all repository rewrites for root files, one machine-id subdirectory level, interleaved ordering, matching, fields, boots, corrupt-skip, verify-skip, empty directories, and the repository `.journal.zst` directory extension.
- SOW-0024 completed mixed-format directory reader validation across stock journalctl plus Rust, Go, Node.js, and Python file-backed rewrites. `run_mixed_directory_matrix.py` passes 72/72 for mixed regular/compact files, uncompressed and zstd/xz/lz4 DATA-compressed files, sealed/unsealed files, active/archive names, directory verification key behavior, and repository whole-file `.journal.zst` / `.journal~.zst` extension discovery. No reader implementation changes were required.
- SOW-0022 was completed as a compatibility planning/triage SOW on 2026-05-26. Its stale gaps were closed by SOW-0019, SOW-0020, and SOW-0024 where applicable; the remaining executable work was split into SOW-0028 through SOW-0034.
- SOW-0028 completed historical header parsing parity. Rust, Go, Node.js, and Python now expose historical extension fields according to each field's on-disk `header_size` containment boundary, with added intermediate/future/truncated-prefix validation and matching Rust coverage in both `journal-core` and `jf/journal_file`.
- SOW-0032 completed live feature compatibility validation. `run_live_matrix.py` now validates regular, zstd/xz/lz4 DATA-compressed, compact, compact plus DATA-compressed, and sealed/FSS active journal files across Go, Rust, Node.js, and Python writers; stock `journalctl --file`; stock libsystemd; Go/Rust/Node.js/Python readers; final `journalctl --verify --file`; sealed `--verify-key`; and structural feature checks. The default run passed 36/36 on `systemd 260 (260.1-2-manjaro)`.
- SOW-0033 completed full verification parity for the supported fixture envelope. `run_verify_matrix.py` passes against stock `journalctl --verify --file` and Rust, Go, Node.js, and Python verification paths for 9 positive files and 12 negative corruption classes on `systemd 260 (260.1-2-manjaro)`.
- SOW-0063 tracks mandatory cross-platform SDK support for Linux, FreeBSD, macOS, and Windows. Stock systemd validation remains Linux-based; files generated on non-Linux targets must be validated on Linux with stock systemd tooling after transfer.
- SOW-0064 is closed. SOW-0076 independently repeated selective real-corpus verification after SOW-0064 and is also closed.
- SOW-0065 tracks the future parallel language parity/performance closure phase after Rust, portability, corpus, and integration gates are stable. Actual git worktree creation and external implementer routing still require explicit user approval at activation time.
- SOW-0066 tracks the final `v1.0.0` release and language registry publication. Registry credentials must never be written to durable artifacts.
- Byte-for-byte writer identity is the target for deterministic uncompressed journals. Any feature slice that cannot be made byte-identical must return with evidence before the acceptance condition is changed.
- The external systemd source checkout is read-only for this project. Build outputs and generated files must remain inside this repository or `/tmp`.
