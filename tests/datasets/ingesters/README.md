# Deterministic Dataset Ingesters

This directory contains test helpers that ingest the frozen dataset with each
writer implementation.

Run every ingester on accepted and rejected corpora:

```bash
python3 tests/datasets/ingesters/run_dataset_ingesters.py --both
```

Generated binaries, cloned systemd source, build trees, logs, and journal files
are written under `.local/`.

The systemd helper is a C program built inside a `.local/` checkout of
`systemd/systemd` v260.1. It uses systemd internal `journal_file_open()` and
`journal_file_append_entry()` APIs and is a reference test helper only.
