# Deterministic Ingestion Dataset

This directory contains the language-neutral ingestion dataset used by the
systemd C, Rust, and Go ingesters.

## Files

- `schema.schema.json` documents the manifest, accepted-record, rejected-record,
  and value descriptor format.
- `ingestion-manifest.json` records corpus counts, SHA-256 hashes, deterministic
  IDs, timestamps, and required coverage tags.
- `correctness/corpus.jsonl` contains accepted journal entries for byte-level
  writer compatibility tests.
- `rejections/corpus.jsonl` contains invalid records with expected rejection
  reasons and error classes.
- `performance/manifest.json` records the 200k-row performance corpus profile
  and stream hash. The full performance JSONL is generated on demand under
  `.local/` and is not committed.

Each JSONL line is canonical JSON: UTF-8, sorted object keys, compact separators,
and one record per line.

## Value Descriptors

Field values are language-neutral descriptors:

- `{"kind":"utf8","text":"..."}` for UTF-8 text values.
- `{"kind":"bytes","base64":"...","size":N}` for arbitrary bytes, including
  embedded NUL bytes.
- `{"kind":"repeat","byte":N,"size":M,"preview":"..."}` for large synthetic
  values that should be materialized by ingesters without committing huge text.

Rejection records may use `input.raw_payload` for pre-parsed byte sequences.
Ingesters should convert that JSON string to UTF-8 bytes directly and verify the
declared rejection outcome, rather than treating it as a normal accepted field.

Records tagged with `entry-array-growth` or `data-entry-array-growth` create
cumulative fanout pressure. They are not asserting a single systemd threshold;
they are shared pressure fixtures for later byte-compatibility and performance
work.

Records tagged with `hash-collision-chain` contain full DATA payloads that
collide into the same systemd v260 keyed DATA hash bucket under the deterministic
file ID. The byte-identity harness requires the resulting
`data_hash_chain_depth` to match the systemd reference value, so this corpus
exercises `next_hash_offset` traversal instead of only the no-collision path.

## Generate

Regenerate committed corpus and manifest files:

```bash
python3 tests/datasets/generate.py committed
```

Generate the 200k-row performance corpus locally:

```bash
python3 tests/datasets/generate.py performance --output .local/datasets/performance-corpus.jsonl
```

Hash the 200k-row performance stream without writing it:

```bash
python3 tests/datasets/generate.py performance-hash
```

## Validate

```bash
python3 tests/datasets/validate.py
```

The validator checks:

- schema and manifest JSON are parseable;
- manifest, correctness, and rejection records conform to `schema.schema.json`
  using Python `jsonschema`;
- correctness and rejection record counts match the manifest;
- all required coverage tags are present;
- accepted field names are systemd-valid;
- binary descriptors decode to their declared sizes;
- committed corpus hashes match the manifest;
- regenerated corpus outputs are deterministic;
- the 200k-row performance stream hash matches `performance/manifest.json`.
