# Selective Real-Corpus Verification Report

- Schema: `systemd-journal-sdk-selective-real-corpus-v1`
- Created: `2026-06-01T10:27:06Z`
- Selected files: `7`
- Verification status: `ok`
- Discrepancies: `0`
- Raw path manifest: `.local/sow-0076/selective-real-corpus/path-manifest.json` (not committed)

## Selection Policy

- `previous-bug-exposure`: files whose sanitized IDs match prior SOW-0064 targeted discrepancy runs
- `historical-unkeyed`: files without HEADER_INCOMPATIBLE_KEYED_HASH
- `fss-sealed`: files with sealed/FSS-compatible header flags or TAG objects
- `compact`: files using HEADER_INCOMPATIBLE_COMPACT
- `compressed-data`: files with compressed DATA support or compressed DATA objects
- `active-open-snapshot`: online-state files that must be snapshotted before driver comparison
- `archived`: archived-state files
- `multi-boot`: files where stock journalctl --file --list-boots reports more than one boot
- `high-cardinality`: files with high DATA object counts
- `high-field-count`: files with high FIELD object counts
- `large-file`: largest files within the bounded selective sample

## Selected Files

| file_id | size MiB | feature classes | reasons | entries | payload-ish DATA objects |
|---|---:|---|---|---:|---:|
| `c846bb5bce46ff77f0fedd82` | 128.00 | `archived`, `compact`, `compressed-data`, `large-file` | `compact` | 32060 | 144328 |
| `efcbdc2673c4a7dd3d5d0529` | 128.00 | `archived`, `compact`, `compressed-data` | `compressed-data` | 31999 | 144158 |
| `e6423d4f234487b9f3e43ae5` | 96.00 | `active-open-snapshot`, `compact`, `compressed-data` | `active-open-snapshot` | 121393 | 121414 |
| `1b8d2037e28033135a630893` | 128.00 | `archived`, `compact`, `compressed-data` | `archived` | 32062 | 144394 |
| `f9b161b0187326a68649aa9a` | 128.00 | `archived`, `compact`, `compressed-data`, `multi-boot` | `multi-boot` | 305093 | 1817 |
| `6583d4e497ede8590a6f66b1` | 56.00 | `archived`, `compact`, `compressed-data`, `high-cardinality` | `high-cardinality` | 91551 | 174768 |
| `b065b4dcec0c59a31a328d2c` | 24.00 | `archived`, `compact`, `compressed-data`, `high-field-count` | `high-field-count` | 22887 | 28054 |

## Missing Feature Classes

- `fss-sealed`: not-present-in-discovered-corpus-or-not-detected
- `historical-unkeyed`: not-present-in-discovered-corpus-or-not-detected
- `large-file`: covered-by-an-earlier-selected-file
- `previous-bug-exposure`: not-present-in-discovered-corpus-or-not-detected

## Verification Results

| kind | driver | mode | status | file_id |
|---|---|---|---|---|
| reader | systemd | - | ok | `c846bb5bce46ff77f0fedd82` |
| reader | rust | - | ok | `c846bb5bce46ff77f0fedd82` |
| reader | go | - | ok | `c846bb5bce46ff77f0fedd82` |
| writer | rust | regular | ok | `c846bb5bce46ff77f0fedd82` |
| writer | rust | compact | ok | `c846bb5bce46ff77f0fedd82` |
| writer | rust | compact-zstd | ok | `c846bb5bce46ff77f0fedd82` |
| writer | rust | compact-fss | ok | `c846bb5bce46ff77f0fedd82` |
| writer | go | regular | ok | `c846bb5bce46ff77f0fedd82` |
| writer | go | compact | ok | `c846bb5bce46ff77f0fedd82` |
| writer | go | compact-zstd | ok | `c846bb5bce46ff77f0fedd82` |
| writer | go | compact-fss | ok | `c846bb5bce46ff77f0fedd82` |
| reader | systemd | - | ok | `efcbdc2673c4a7dd3d5d0529` |
| reader | rust | - | ok | `efcbdc2673c4a7dd3d5d0529` |
| reader | go | - | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | rust | regular | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | rust | compact | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | rust | compact-zstd | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | rust | compact-fss | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | go | regular | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | go | compact | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | go | compact-zstd | ok | `efcbdc2673c4a7dd3d5d0529` |
| writer | go | compact-fss | ok | `efcbdc2673c4a7dd3d5d0529` |
| reader | systemd | - | ok | `e6423d4f234487b9f3e43ae5` |
| reader | rust | - | ok | `e6423d4f234487b9f3e43ae5` |
| reader | go | - | ok | `e6423d4f234487b9f3e43ae5` |
| writer | rust | regular | ok | `e6423d4f234487b9f3e43ae5` |
| writer | rust | compact | ok | `e6423d4f234487b9f3e43ae5` |
| writer | rust | compact-zstd | ok | `e6423d4f234487b9f3e43ae5` |
| writer | rust | compact-fss | ok | `e6423d4f234487b9f3e43ae5` |
| writer | go | regular | ok | `e6423d4f234487b9f3e43ae5` |
| writer | go | compact | ok | `e6423d4f234487b9f3e43ae5` |
| writer | go | compact-zstd | ok | `e6423d4f234487b9f3e43ae5` |
| writer | go | compact-fss | ok | `e6423d4f234487b9f3e43ae5` |
| reader | systemd | - | ok | `1b8d2037e28033135a630893` |
| reader | rust | - | ok | `1b8d2037e28033135a630893` |
| reader | go | - | ok | `1b8d2037e28033135a630893` |
| writer | rust | regular | ok | `1b8d2037e28033135a630893` |
| writer | rust | compact | ok | `1b8d2037e28033135a630893` |
| writer | rust | compact-zstd | ok | `1b8d2037e28033135a630893` |
| writer | rust | compact-fss | ok | `1b8d2037e28033135a630893` |
| writer | go | regular | ok | `1b8d2037e28033135a630893` |
| writer | go | compact | ok | `1b8d2037e28033135a630893` |
| writer | go | compact-zstd | ok | `1b8d2037e28033135a630893` |
| writer | go | compact-fss | ok | `1b8d2037e28033135a630893` |
| reader | systemd | - | ok | `f9b161b0187326a68649aa9a` |
| reader | rust | - | ok | `f9b161b0187326a68649aa9a` |
| reader | go | - | ok | `f9b161b0187326a68649aa9a` |
| writer | rust | regular | ok | `f9b161b0187326a68649aa9a` |
| writer | rust | compact | ok | `f9b161b0187326a68649aa9a` |
| writer | rust | compact-zstd | ok | `f9b161b0187326a68649aa9a` |
| writer | rust | compact-fss | ok | `f9b161b0187326a68649aa9a` |
| writer | go | regular | ok | `f9b161b0187326a68649aa9a` |
| writer | go | compact | ok | `f9b161b0187326a68649aa9a` |
| writer | go | compact-zstd | ok | `f9b161b0187326a68649aa9a` |
| writer | go | compact-fss | ok | `f9b161b0187326a68649aa9a` |
| reader | systemd | - | ok | `6583d4e497ede8590a6f66b1` |
| reader | rust | - | ok | `6583d4e497ede8590a6f66b1` |
| reader | go | - | ok | `6583d4e497ede8590a6f66b1` |
| writer | rust | regular | ok | `6583d4e497ede8590a6f66b1` |
| writer | rust | compact | ok | `6583d4e497ede8590a6f66b1` |
| writer | rust | compact-zstd | ok | `6583d4e497ede8590a6f66b1` |
| writer | rust | compact-fss | ok | `6583d4e497ede8590a6f66b1` |
| writer | go | regular | ok | `6583d4e497ede8590a6f66b1` |
| writer | go | compact | ok | `6583d4e497ede8590a6f66b1` |
| writer | go | compact-zstd | ok | `6583d4e497ede8590a6f66b1` |
| writer | go | compact-fss | ok | `6583d4e497ede8590a6f66b1` |
| reader | systemd | - | ok | `b065b4dcec0c59a31a328d2c` |
| reader | rust | - | ok | `b065b4dcec0c59a31a328d2c` |
| reader | go | - | ok | `b065b4dcec0c59a31a328d2c` |
| writer | rust | regular | ok | `b065b4dcec0c59a31a328d2c` |
| writer | rust | compact | ok | `b065b4dcec0c59a31a328d2c` |
| writer | rust | compact-zstd | ok | `b065b4dcec0c59a31a328d2c` |
| writer | rust | compact-fss | ok | `b065b4dcec0c59a31a328d2c` |
| writer | go | regular | ok | `b065b4dcec0c59a31a328d2c` |
| writer | go | compact | ok | `b065b4dcec0c59a31a328d2c` |
| writer | go | compact-zstd | ok | `b065b4dcec0c59a31a328d2c` |
| writer | go | compact-fss | ok | `b065b4dcec0c59a31a328d2c` |

## Discrepancies

- none

## Rerun Recipe

```bash
python tests/corpus_eval/run_selective_real_corpus.py --root <journal-root> [--root <journal-root>] --run-verification
```
