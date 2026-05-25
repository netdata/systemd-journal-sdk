# SOW-0022 - Compatibility Test Gap Audit

## Status

Status: open

Sub-state: Evidence captured for user discussion. Implementation is blocked until the user decides scope, sequencing, and overlap with active FSS work.

## Requirements

### Purpose

Ensure the Rust, Go, Node.js, and Python SDKs only produce, read, verify, and query systemd journal files in ways that are compatible with the systemd v260.1 journal implementation. Feature parity must be enforced by tests so incompatible journal files or journalctl behavior cannot silently pass.

### User Request

Create a SOW that records the testing gaps and feature/logic gaps found during the read-only compatibility review, with concrete evidence, so the user can discuss the findings with the assistant currently implementing FSS.

Constraints:

- Do not implement fixes in this SOW creation step.
- Do not create conflicts with the active FSS implementation work.
- Record concrete evidence, not high-level conclusions.
- Use systemd v260.1 as the compatibility baseline.

### Assistant Understanding

Facts:

- The project compatibility baseline is `systemd/systemd` tag `v260.1`, commit `c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`.
- Active SOW-0019 is implementing Forward Secure Sealing work and has recently added unsealed entry/data-object structural verification APIs in all four languages.
- Pending SOW-0020 already tracks directory traversal parity.
- Existing tests include strong deterministic byte identity for regular uncompressed writer output.
- Existing compression and compact matrices prove closed-file semantic compatibility, but not live append compatibility or exact systemd layout parity.

Inferences:

- Some findings are pure test gaps; some are likely implementation gaps that tests should expose.
- Full verification parity overlaps with SOW-0019, so this SOW must remain pending until the user decides whether to keep verification-parity tests here, merge part of them into SOW-0019, or split the work.
- Directory traversal should probably stay in SOW-0020 to avoid duplicate implementation ownership.

Unknowns:

- Whether the user wants compressed and compact writer outputs to be byte-for-byte identical to systemd, or whether semantic compatibility plus exact public/header/object invariants are sufficient when compression libraries produce different byte streams.
- Whether full `journalctl --follow`, `--verify`, `--verify-key`, `--boot`, `--since`, and `--until` parity should be implemented in this SOW or split into journalctl-specific SOWs.
- Whether active SOW-0019 will expand verification from shallow entry/data walking to systemd-like object graph verification.

### Acceptance Criteria

- The SOW records every identified compatibility test gap with file/line evidence.
- The SOW separates already-tracked gaps from new gaps.
- The SOW records user decisions required before implementation.
- No implementation files are changed by this SOW creation step.

## Analysis

Sources checked:

- `.agents/skills/project-agent-orchestration/SKILL.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/sow/specs/product-scope.md`
- `SOW-status.md`
- `tests/interoperability/README.md`
- `tests/conformance/manifests/conformance-v01.json`
- Rust, Go, Node.js, and Python reader/writer/verification/journalctl sources
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Regular deterministic uncompressed writer output has byte-for-byte systemd parity coverage for the accepted corpus. Evidence: `tests/interoperability/README.md:16-26`.
- Live append testing exists for the regular current writer slice. Evidence: `tests/interoperability/README.md:62-98`.
- The live matrix explicitly excludes compression, compact journal, and FSS. Evidence: `tests/interoperability/README.md:100-115`.
- Compression and compact matrices validate closed-file stock-reader and repository-reader compatibility. Evidence: `tests/interoperability/README.md:206-225`, `tests/interoperability/README.md:245-259`.
- Product scope requires live concurrency compatibility for every writer and reader feature slice. Evidence: `.agents/sow/specs/product-scope.md:35-62`.

Risks:

- A writer feature can pass closed-file verification but still be incompatible while the file is online and being appended.
- Shallow verification can give false confidence by accepting files that systemd rejects.
- Header parsing bugs for historical files can silently corrupt metadata and verification behavior.
- Divergent compression thresholds can make repository writers do more than systemd by default.
- File-backed journalctl rewrites may remain below the declared project target if query options are treated as unsupported indefinitely.

### Gap 1 - Live compatibility does not cover compression, compact layout, or FSS

Evidence:

- Product contract: every writer must be readable by stock `journalctl --file` and stock libsystemd while appending, and `journalctl --verify --file` must pass for the feature slice. Evidence: `.agents/sow/specs/product-scope.md:37-44`.
- Product contract: each repository reader must handle online state, tail metadata changes, entry-array growth, hash-table chaining, and observable file-size changes. Evidence: `.agents/sow/specs/product-scope.md:45-62`.
- Current live matrix excludes compression, compact journal, and FSS. Evidence: `tests/interoperability/README.md:112-115`.
- Compression matrix is closed-file oriented. Evidence: `tests/interoperability/README.md:216-225`.
- Compact matrix is closed-file oriented. Evidence: `tests/interoperability/README.md:245-256`.

Implication:

- Compressed DATA, compact ENTRY/ENTRY_ARRAY layout, and sealed/FSS append windows can be incompatible even when closed-file tests pass.

Needed tests:

- Extend or add live matrices for each implemented feature slice:
  - regular plus zstd/xz/lz4 DATA compression;
  - compact uncompressed;
  - compact plus zstd/xz/lz4 DATA compression;
  - FSS/sealed files after SOW-0019 implements writer sealing.
- Each live feature matrix must include stock `journalctl --file`, stock libsystemd, and every repository reader while the writer is active.
- Final closed-file validation must include `journalctl --verify --file`.

### Gap 2 - Current verification APIs are much shallower than systemd verification

Evidence:

- Go `VerifyFile()` opens the file, seeks, walks entries, and gets each entry; this verifies the entry/data path but not the full systemd object graph. Evidence: `go/journal/verify.go:19-55`.
- Rust `verify_file()` opens/decompresses the file, walks entry offsets strictly, and verifies referenced DATA objects strictly; this verifies the entry/data path but not the full systemd object graph. Evidence: `rust/src/journal/src/lib.rs:356-368`, `rust/src/journal/src/lib.rs:661-725`.
- Node.js `verifyFile()` parses entry offsets and referenced DATA objects. Evidence: `node/src/lib/verify.js:16-65`.
- Python `verify_file()` parses entry offsets and referenced DATA objects. Evidence: `python/journal/verify.py:15-56`.
- The conformance corruption case currently uses one truncated zstd frame fixture. Evidence: `tests/conformance/manifests/conformance-v01.json:218-230`.
- systemd verifies object type/size/hash/offset rules. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:141-205`.
- systemd verifies DATA entry-array linkage and sortedness. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:425-505`.
- systemd verifies DATA hash table membership, hash bucket, cycles, and tail pointers. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:527-601`.
- systemd verifies seqnum and same-boot monotonic ordering. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:1027-1065`.
- systemd verifies header object counters and existence of the main entry array. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:1262-1325`.
- systemd performs second-pass reachability checks through entry arrays and hash tables. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:1372-1396`.

Implication:

- Repository verification APIs can still accept object-graph corruption classes that stock `journalctl --verify` rejects.
- The current conformance case is too narrow to enforce systemd verification feature parity.

Needed tests:

- Add shared corrupted fixtures for every systemd verification class that is practical to generate safely:
  - wrong object type or compressed non-DATA object;
  - DATA payload hash mismatch;
  - invalid DATA hash-chain pointer;
  - hash table head/tail mismatch;
  - DATA entry array too short, cyclic, unsorted, or pointing to non-entry;
  - ENTRY object pointing to missing DATA;
  - header `n_objects`, `n_entries`, `n_data`, `n_fields`, `n_tags`, and `n_entry_arrays` counter mismatches;
  - missing main entry array;
  - entry seqnum regression;
  - same-boot monotonic timestamp regression;
  - invalid TAG object or FSS key-required behavior after SOW-0019 defines the public API.
- For each fixture, assert that stock `journalctl --verify --file` rejects it and every repository verification API rejects it with a controlled verification error.

### Gap 3 - Directory traversal interleaving is already known incomplete

Evidence:

- SOW-status records `SOW-0020-20260524-directory-traversal-parity.md` as open for SDK directory readers and file-backed journalctl `--directory` parity. Evidence: `SOW-status.md:9`.
- Product scope records sequential directory iteration as a current limitation for Go, Rust, Node.js, and Python. Evidence: `.agents/sow/specs/product-scope.md:206-213`, `.agents/sow/specs/product-scope.md:257-264`, `.agents/sow/specs/product-scope.md:308-315`, `.agents/sow/specs/product-scope.md:362-369`.
- Go sorts files by head metadata then drains the current file before moving to the next. Evidence: `go/journal/reader.go:860-867`, `go/journal/reader.go:887-906`.
- Rust sorts files then drains the current file before moving to the next. Evidence: `rust/src/journal/src/lib.rs:413-414`, `rust/src/journal/src/lib.rs:431-442`.
- Node.js sorts files then drains the current file before moving to the next. Evidence: `node/src/lib/directory-reader.js:27-33`, `node/src/lib/directory-reader.js:53-66`.
- Python sorts files then drains the current file before moving to the next. Evidence: `python/journal/directory_reader.py:29-30`, `python/journal/directory_reader.py:42-49`.
- systemd journal output is interleaved from all accessible files. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml:74-80`.
- systemd selects the next entry by comparing current locations across all files. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/sd-journal.c:1124-1173`.

Implication:

- Directory-mode readers and journalctl rewrites can return a different global order than systemd when journal files overlap in realtime or monotonic ranges.

Needed tests:

- Keep implementation ownership in SOW-0020 unless the user chooses to merge.
- Add overlapping multi-file directory fixtures and compare repository readers/journalctl rewrites against stock systemd ordering.
- Include forward, backward, seek, match-filtered, and `--list-boots` behavior where applicable.

### Gap 4 - Historical header field parsing is likely wrong in Go, Node.js, and Python

Evidence:

- systemd header fields were added incrementally: `n_data`, `n_fields`, `n_tags`, `n_entry_arrays`, hash-chain depths, and later tail-entry-array fields. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-def.h:227-240`.
- systemd uses a per-field `JOURNAL_HEADER_CONTAINS()` check based on `header_size`, not an all-or-nothing current-header-size check. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.h:179-180`.
- Product scope requires header parsing to respect on-disk `header_size` for historical files and not expose absent fields. Evidence: `.agents/sow/specs/product-scope.md:150-155`.
- Go reads all extension fields only when `headerSize >= headerSize` for the current 272-byte header. Evidence: `go/journal/format.go:255-272`.
- Node.js initializes extension fields to zero and reads them only when `header_size >= HEADER_SIZE`. Evidence: `node/src/lib/header.js:127-145`.
- Python initializes extension fields to zero and reads them only when `header_size >= HEADER_SIZE`. Evidence: `python/journal/header.py:104-125`.
- Rust already applies per-field sanitization for historical header sizes. Evidence: `rust/src/crates/journal-core/src/file/file.rs:343-371`.
- Existing conformance header parsing only requires signature, state, compatible flags, incompatible flags, and `header_size`. Evidence: `tests/conformance/manifests/conformance-v01.json:310-319`.
- Bundled `fixtures/systemd/test-data/no-rtc/*.zst` files have `header_size=256`, where `n_data`, `n_fields`, `n_tags`, `n_entry_arrays`, `data_hash_chain_depth`, and `field_hash_chain_depth` are present but v252/v254 tail-entry-array fields are absent. Evidence from read-only fixture inspection: all seven no-RTC fixture headers reported `256`.

Implication:

- Go, Node.js, and Python likely report zero for valid historical header fields that are present on disk.
- Verification, count reporting, and future append/repair behavior can be wrong for valid older journal files.

Needed tests:

- Add shared header fixtures or fixture assertions for at least `header_size=208`, `216`, `224`, `232`, `240`, `248`, `256`, `260`, `264`, and `272`.
- Assert present fields are exposed and absent fields are zero/default per systemd semantics.
- Run the same assertions through Rust, Go, Node.js, and Python adapters.

### Gap 5 - Compression default threshold differs from systemd

Evidence:

- systemd default compression threshold is 512 bytes and minimum threshold is 8 bytes. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.c:51-52`.
- systemd uses default 512 when the caller passes the default sentinel and otherwise clamps to at least 8. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.c:4127-4133`.
- systemd only compresses DATA objects when the payload reaches the threshold and compressed output is smaller than the source. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.c:1824-1837`.
- systemd has explicit threshold boundary tests. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/test-journal.c:243-260`.
- Go default threshold is 64. Evidence: `go/journal/format.go:81`, `go/journal/writer.go:46-48`.
- Rust high-level log config default threshold is 64. Evidence: `rust/src/crates/journal-log-writer/src/log/config.rs:96-103`.
- Node.js default threshold is 64. Evidence: `node/src/lib/writer.js:31-35`.
- Python default threshold is 64. Evidence: `python/journal/writer.py:34-38`.
- Current compression matrix intentionally uses a low threshold for coverage. Evidence: `tests/interoperability/README.md:206-214`.

Implication:

- Repository writers can compress fields that systemd would leave uncompressed by default, so they can do more than systemd.
- Compatibility tests currently do not enforce systemd threshold boundaries.

Needed tests:

- Add threshold boundary tests matching systemd:
  - default threshold does not compress below 512;
  - default threshold compresses above 512 only when the compressor yields a smaller blob;
  - configured threshold below 8 is clamped to 8;
  - exact configured threshold boundary compresses at threshold and not below threshold.
- Decide whether repository writer defaults must change to 512.

### Gap 6 - File-backed journalctl parity remains incomplete

Evidence:

- Product target says journalctl rewrites must cover file-backed/query behavior. Evidence: `.agents/sow/specs/product-scope.md:371-373`.
- Go marks `--follow`, `--boot`, `--since`, `--until`, `--verify`, and `--verify-only` unsupported. Evidence: `go/cmd/journalctl/main.go:37-49`, `go/cmd/journalctl/main.go:70-75`.
- Rust marks `--follow`, `--verify`, and `--verify-only` unsupported. Evidence: `rust/src/cmd/journalctl/main.rs:31-44`, `rust/src/cmd/journalctl/main.rs:74-85`.
- Node.js marks `--follow`, `--verify`, `--verify-only`, `--boot`, `--since`, and `--until` unsupported. Evidence: `node/cmd/journalctl/index.js:51-60`.
- Python marks `--follow`, `--verify`, `--verify-only`, `--boot`, `--since`, and `--until` unsupported. Evidence: `python/cmd/journalctl.py:44-53`, `python/cmd/journalctl.py:59-78`.
- systemd documents interleaved file-backed output and file/directory selection. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml:74-80`.
- systemd documents `--follow` as continuously printing new entries as appended. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml:773-783`.
- systemd documents `--verify` and `--verify-key`. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml:863-869`, `man/journalctl.xml:990-997`.
- systemd documents `--list-boots`. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml:908-916`.

Implication:

- Daemon-only commands are correctly unsupported, but query/file-backed options are not all daemon-only.
- The project may not meet its journalctl rewrite target until these options are either implemented, explicitly descoped by user decision, or split into tracked SOWs.

Needed tests:

- Add conformance cases comparing stock `journalctl --file` or `--directory` behavior for:
  - `--follow` on an actively appended file;
  - `--verify`;
  - `--verify-key` once FSS support exists;
  - `--boot` and `--list-boots` across overlapping boot IDs;
  - `--since` and `--until` realtime filtering.

### Gap 7 - Byte identity does not cover compressed or compact output

Evidence:

- Byte identity currently covers deterministic uncompressed regular writers. Evidence: `tests/interoperability/README.md:16-26`.
- Compression matrix validates flags, stock verification, stock reads, repository reads, and match behavior, but not byte identity. Evidence: `tests/interoperability/README.md:216-225`.
- Compact matrix validates compact structure, stock verification, stock reads, and repository reads, but not byte identity. Evidence: `tests/interoperability/README.md:245-256`.
- Product scope says deterministic regular uncompressed files must be byte-identical and compact interoperability is validated structurally. Evidence: `.agents/sow/specs/product-scope.md:133-163`.

Implication:

- If the compatibility goal means exact writer layout parity beyond regular uncompressed output, the current tests are not strict enough for compressed and compact files.
- If semantic compatibility is sufficient for compressed or compact output, that policy needs to be recorded explicitly.

Needed tests:

- Create a decision-backed test strategy:
  - exact byte identity for deterministic compact uncompressed files if systemd can generate an equivalent compact reference;
  - exact byte identity for compressed files only if compressor determinism and library versions make that meaningful;
  - otherwise, field-by-field/object-by-object layout parity checks for object order, offsets, flags, hash chains, counters, and tail metadata.

### Gap 8 - Public writer APIs may allow invalid same-boot monotonic order

Evidence:

- systemd verification rejects decreasing monotonic timestamps for entries with the same boot ID. Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:1050-1063`.
- Current live writer tests normally use monotonically increasing sequence/timestamps, so the negative case is not exercised. Evidence: `tests/interoperability/README.md:81-98`.

Implication:

- Public writer APIs that accept caller-supplied monotonic timestamps can produce files that stock `journalctl --verify --file` rejects unless writers clamp, reject, or clearly document this as caller responsibility.

Needed tests:

- Add negative writer tests where entries share a boot ID and monotonic timestamps decrease.
- Decide expected behavior:
  - writer rejects the append;
  - writer clamps to preserve systemd validity;
  - writer allows invalid input, but tests prove stock verification rejects the resulting file and documentation warns callers.

## Pre-Implementation Gate

Status: decisions-recorded

Problem / root-cause model:

- The repository has grown several independent compatibility matrices, but the tests do not yet enforce systemd parity uniformly across all feature slices.
- Some features are validated only after close, while the project requires live compatibility during append.
- Current verification APIs are intentionally shallow in active FSS work, but systemd verification is object-graph and metadata-counter based.
- Some current defaults and parsers diverge from systemd behavior.

Evidence reviewed:

- Project compatibility contract: `.agents/sow/specs/product-scope.md:35-62`, `.agents/sow/specs/product-scope.md:133-163`, `.agents/sow/specs/product-scope.md:371-373`.
- Current SOW status: `SOW-status.md:5-11`, `SOW-status.md:43-47`.
- Current test matrix documentation: `tests/interoperability/README.md:16-26`, `tests/interoperability/README.md:62-115`, `tests/interoperability/README.md:206-225`, `tests/interoperability/README.md:245-259`.
- Current conformance manifest: `tests/conformance/manifests/conformance-v01.json:193-230`, `tests/conformance/manifests/conformance-v01.json:310-319`.
- Current source evidence listed in the gap sections above.
- Open-source reference evidence listed below.

Affected contracts and surfaces:

- Rust, Go, Node.js, and Python writer behavior.
- Rust, Go, Node.js, and Python reader behavior.
- Rust, Go, Node.js, and Python verification APIs.
- Rust, Go, Node.js, and Python journalctl rewrites.
- Shared conformance manifest and adapters.
- Interoperability runners under `tests/interoperability/`.
- Live concurrency harnesses under `tests/conformance/live/`.
- Product scope spec if user decisions change compatibility policy.

Existing patterns to reuse:

- `tests/interoperability/run_live_matrix.py` for live append validation.
- `tests/interoperability/run_compression_matrix.py` for compressed DATA validation.
- `tests/interoperability/run_compact_matrix.py` for compact layout validation.
- `tests/interoperability/run_byte_identity.py` for deterministic layout comparison.
- Conformance manifest adapter structure under `tests/conformance/`.
- Existing stock `journalctl --verify --file` and stock libsystemd checks already used by current matrices.

Risk and blast radius:

- High compatibility risk: the gaps affect journal file validity, not cosmetic behavior.
- High cross-language blast radius: most findings affect all four SDKs.
- Medium implementation risk: verification parity and directory interleaving require careful object graph and ordering logic.
- Medium sequencing risk: verification/FSS items overlap active SOW-0019.
- Low security risk from this SOW creation step: only public source paths and fixture names are recorded.

Sensitive data handling plan:

- Use only repository file paths, line numbers, fixture names, and systemd source references.
- Do not copy raw journal payloads, private host journal content, secrets, credentials, customer data, personal data, public customer-identifying IPs, private endpoints, or proprietary incidents.
- Any future generated corruption fixture must be synthetic or derived from committed public fixtures with no sensitive payload.

Implementation plan:

1. Apply the recorded user decisions below: keep SOW-0019 focused on its FSS/journalctl phase, keep directory traversal in SOW-0020, and keep this SOW as the tracker for the remaining compatibility gaps.
2. Split or refine this SOW before implementation if the remaining compatibility gaps become too broad for one execution chunk.
3. Add tests before implementation fixes where practical, especially for header parsing, compression thresholds, and verification corruption classes.
4. Implement fixes exposed by those tests in all affected languages.
5. Validate every affected language and stock systemd behavior.

Validation plan:

- Run shared conformance tests for Rust, Go, Node.js, and Python.
- Run relevant interoperability matrices:
  - live matrix extensions for compression and compact;
  - compression matrix;
  - compact matrix;
  - byte identity or layout parity matrix as decided by the user;
  - directory interleaving matrix if merged from SOW-0020.
- Run stock `journalctl --verify --file` against all positive and negative fixtures.
- Run stock libsystemd reader checks where existing harnesses support them.
- Run reviewer agents read-only after implementation, using the project reviewer pool.

Artifact impact plan:

- AGENTS.md: likely unaffected unless the user changes compatibility policy.
- Runtime project skills: update `project-journal-compatibility` if new mandatory validation gates are established.
- Specs: update `.agents/sow/specs/product-scope.md` if compressed/compact byte identity, verification parity, or journalctl option scope decisions change product reality.
- End-user/operator docs: update only if public API behavior or journalctl support changes.
- End-user/operator skills: likely unaffected; revisit if docs/spec changes create exported operator guidance.
- SOW lifecycle: keep this pending until the user decides scope and sequencing.
- SOW-status.md: add this SOW under Pending.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `man/journalctl.xml:74-80`
  - `man/journalctl.xml:773-783`
  - `man/journalctl.xml:863-869`
  - `man/journalctl.xml:908-916`
  - `man/journalctl.xml:990-997`
  - `src/libsystemd/sd-journal/journal-def.h:227-240`
  - `src/libsystemd/sd-journal/journal-file.c:51-52`
  - `src/libsystemd/sd-journal/journal-file.c:1824-1837`
  - `src/libsystemd/sd-journal/journal-file.c:4127-4133`
  - `src/libsystemd/sd-journal/journal-file.h:179-180`
  - `src/libsystemd/sd-journal/journal-verify.c:141-205`
  - `src/libsystemd/sd-journal/journal-verify.c:425-505`
  - `src/libsystemd/sd-journal/journal-verify.c:527-601`
  - `src/libsystemd/sd-journal/journal-verify.c:1027-1065`
  - `src/libsystemd/sd-journal/journal-verify.c:1262-1325`
  - `src/libsystemd/sd-journal/journal-verify.c:1372-1396`
  - `src/libsystemd/sd-journal/sd-journal.c:1124-1173`
  - `src/libsystemd/sd-journal/test-journal.c:243-260`

Recorded decisions:

1. Verification parity ownership:
   - Option A: Keep shallow unsealed verification in SOW-0019 and implement full systemd verification parity in this SOW later.
     - Pros: avoids disrupting active FSS implementation; gives full verification parity a dedicated test/fixture design.
     - Cons: SOW-0019 may close with verification APIs that are known to be incomplete unless explicitly scoped as provisional.
     - Implications: SOW-0019 should record the limitation and link this SOW.
     - Risks: users may misunderstand current verification APIs as systemd-equivalent.
   - Option B: Move full unsealed verification parity into SOW-0019 now.
     - Pros: verification API lands closer to production-grade.
     - Cons: increases active SOW scope significantly and may delay FSS.
     - Implications: this SOW should drop or narrow Gap 2 after SOW-0019 absorbs it.
     - Risks: larger active SOW raises review and conflict risk.
   - Recommendation: Option A, but require SOW-0019 to explicitly document shallow verification as provisional and keep this SOW pending for full parity.
   - User decision 2026-05-25: Option A.

2. Directory traversal ownership:
   - Option A: Keep directory interleaving in SOW-0020.
     - Pros: avoids duplicate ownership; SOW-0020 already exists for this exact surface.
     - Cons: this SOW will depend on another pending SOW for full journalctl/directory parity.
     - Implications: this SOW should only reference the gap and not implement it.
     - Risks: full compatibility remains split across SOWs.
   - Option B: Merge SOW-0020 into this SOW.
     - Pros: one compatibility umbrella.
     - Cons: broadens this SOW too much and violates the one-SOW-at-a-time discipline.
     - Implications: SOW lifecycle work would be needed to merge pending SOWs.
     - Risks: harder review and higher conflict risk.
   - Recommendation: Option A.
   - User decision 2026-05-25: Option A.

3. Compressed/compact layout parity target:
   - Option A: Require byte-for-byte identity for every deterministic compressed and compact output that systemd can generate deterministically.
     - Pros: strongest compatibility signal.
     - Cons: compression output can depend on library/version/level details; exact bytes may be impractical across languages.
     - Implications: may require pinning compressor behavior or accepting explicit deltas.
     - Risks: brittle tests if compressor output differs while journal semantics remain valid.
   - Option B: Require exact structural parity for compressed/compact output, plus stock verification and stock read parity, but not compressed-byte identity unless deterministic.
     - Pros: focuses on journal format correctness and avoids compressor-byte brittleness.
     - Cons: weaker than full byte identity.
     - Implications: tests must inspect object order, offsets, flags, counters, hash chains, and tail metadata.
     - Risks: subtle byte-level differences can remain.
   - Recommendation: Option B by default, with byte identity required for compact uncompressed if systemd can produce an equivalent reference.
   - User decision 2026-05-25: Option B.

4. Writer behavior for invalid monotonic timestamps:
   - Option A: Reject appends that would make same-boot monotonic timestamps go backwards.
     - Pros: prevents writers from producing files stock systemd rejects.
     - Cons: changes public writer behavior for callers that pass raw timestamps.
     - Implications: each writer needs state tracking and error semantics.
     - Risks: caller-visible breaking change.
   - Option B: Clamp monotonic timestamps to maintain valid order.
     - Pros: preserves file validity.
     - Cons: mutates caller-provided timestamps.
     - Implications: docs must be explicit; may be surprising for users.
     - Risks: hidden data distortion.
   - Option C: Allow invalid input but document it and add negative tests proving stock verification rejects it.
     - Pros: minimal behavior change.
     - Cons: SDKs can write incompatible journal files.
     - Implications: incompatible with a strict "must not produce invalid files" policy.
     - Risks: invalid files in production.
   - Recommendation: Option A.
   - User decision 2026-05-25: Option A.

5. journalctl file-backed option scope:
   - Option A: Implement `--follow`, `--verify`, `--verify-key`, `--boot`, `--since`, and `--until` parity in follow-up SOWs.
     - Pros: aligns with file-backed/query target.
     - Cons: more work; `--follow` requires live behavior.
     - Implications: add conformance tests before implementation.
     - Risks: delayed full journalctl parity.
   - Option B: Explicitly descope some options in product scope.
     - Pros: reduces scope.
     - Cons: contradicts the current 100% file-backed parity direction unless the user changes the goal.
     - Implications: specs and SOW status must be updated.
     - Risks: SDK journalctl rewrites remain incomplete.
   - Recommendation: Option A.
   - User decision 2026-05-25: Option A.

## Implications And Decisions

User decisions recorded on 2026-05-25:

1. Verification parity ownership: Option A. Keep shallow unsealed verification in SOW-0019 and implement full systemd verification parity in this SOW later. SOW-0019 must explicitly document that its current unsealed verification APIs and file-backed journalctl `--verify` behavior are provisional and not full systemd object-graph verification parity.
2. Directory traversal ownership: Option A. Keep directory interleaving and `--directory` parity in SOW-0020.
3. Compressed/compact layout parity target: Option B. Require structural parity plus stock verification/read parity for compressed and compact output; require byte identity only where deterministic and meaningful, including compact uncompressed if systemd can generate an equivalent reference.
4. Writer behavior for invalid monotonic timestamps: Option A. Writers must reject appends that would make same-boot monotonic timestamps go backwards.
5. journalctl file-backed option scope: Option A. Implement file-backed `--follow`, `--verify`, `--verify-key`, `--boot`, `--since`, and `--until` parity in follow-up SOWs, not by descoping them.

## Plan

1. Resolve overlap with SOW-0019 and SOW-0020.
2. Split the SOW if the user wants smaller execution units.
3. Add failing shared tests for header parsing and compression threshold parity first because they are narrow and high-confidence.
4. Add verification corruption fixture families and stock systemd oracle checks.
5. Add live feature matrices for compression and compact; add FSS live matrix after writer sealing exists.
6. Add or split journalctl file-backed parity tests.
7. Implement fixes in all affected languages.
8. Validate with stock systemd, all repository adapters, and read-only reviewer agents.

## Delegation Plan

Implementer:

- Preferred implementer: `llm-netdata-cloud/kimi-k2.6`, after the user approves scope and sequencing.
- Fallback hierarchy: `llm-netdata-cloud/qwen3.6-plus`, then `llm-netdata-cloud/glm-5.1`, then another approved model only if this SOW records why the first fallbacks were unavailable or unsuitable.

Reviewers:

- Use read-only reviewers from the project pool: `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.
- Minimax remains reviewer-only unless the user changes routing.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record implementer failure, reviewer failure, audit failure, or model unavailability in this SOW before switching models or changing scope.
- Do not advance phases while production-grade compatibility doubts remain unresolved.

## Execution Log

### 2026-05-25

- Created this pending SOW from a read-only compatibility gap review.
- No implementation files were intentionally changed.
- User requested SOW creation so the findings can be discussed with the assistant currently implementing FSS.

## Validation

Acceptance criteria evidence:

- Pending until the user decides whether this SOW remains an audit/planning SOW or becomes an implementation SOW.

Tests or equivalent validation:

- No test suite was run during SOW creation.
- Evidence was gathered through static inspection of repository files and the local read-only systemd v260.1 source checkout.

Real-use evidence:

- Not applicable to SOW creation.

Reviewer findings:

- Pending. No external reviewers were run because the user requested SOW creation, not review execution.

Same-failure scan:

- Initial same-failure scan is recorded in the gap sections. A full implementation-phase scan must be repeated before code changes.

Sensitive data gate:

- This SOW records repository paths, fixture names, line numbers, and public systemd source references only.
- No raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details were added.

Artifact maintenance gate:

- AGENTS.md: no update needed for SOW creation; project rules already cover the workflow.
- Runtime project skills: no update yet; update `project-journal-compatibility` only after user decisions create new mandatory gates.
- Specs: no update yet; update `product-scope.md` only after user decisions change scope or current reality.
- End-user/operator docs: no update needed for SOW creation.
- End-user/operator skills: no update needed for SOW creation.
- SOW lifecycle: created as `Status: open` in `.agents/sow/pending/`.
- SOW-status.md: updated to list this SOW under Pending.

Specs update:

- No spec update was made because this SOW records gaps and decisions, not completed product behavior.

Project skills update:

- No project skill update was made because no new mandatory workflow has been accepted yet.

End-user/operator docs update:

- No end-user/operator docs update was made because no public behavior changed.

End-user/operator skills update:

- No end-user/operator skill update was made because no docs/spec changes created exported operator guidance.

Lessons:

- None yet. Lessons should be extracted after implementation/review.

Follow-up mapping:

- Directory traversal parity is tracked by `SOW-0020-20260524-directory-traversal-parity.md`.
- FSS sealing and sealed verification overlap active `SOW-0019-20260524-forward-secure-sealing.md`.
- All other gaps are tracked here until the user chooses to split, merge, reject, or implement them.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
