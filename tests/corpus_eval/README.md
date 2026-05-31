# Real-World Journal Corpus Evaluation

This tooling evaluates real journal files incrementally without copying the
full corpus or writing raw journal content to durable reports.

Run mode copies one input journal at a time into `.local` scratch before
reader/writer comparisons, then deletes that snapshot. This keeps comparisons
stable for active journals without retaining a second full corpus copy.

Safe modes:

```bash
python tests/corpus_eval/run_corpus_eval.py --mode dry-run --root /path/to/journals --out .local/corpus-eval/dry-run
python tests/corpus_eval/run_corpus_eval.py --mode smoke --out .local/corpus-eval/smoke
```

Full corpus execution is intentionally guarded:

```bash
python tests/corpus_eval/run_corpus_eval.py --mode run --allow-full-run --root /path/to/journals --out .local/corpus-eval/full
```

Focused raw-reader and spool-writer experiments are separate from the full
regeneration harness. They measure:

- raw SDK reader throughput with no materialization beyond counts and a
  length-prefixed hash;
- binary-safe spool dumping in the systemd Journal Export Format shape;
- pure writer throughput from that spool for selected output options;
- stock `journalctl --verify --file` and canonical digest parity after
  regeneration.

Single-file smoke:

```bash
python tests/corpus_eval/run_spool_experiment.py --input /path/to/file.journal --out .local/corpus-eval/spool-experiment-single
```

Bounded batch:

```bash
python tests/corpus_eval/run_spool_experiment.py --root /path/to/journals --max-files 100 --out .local/corpus-eval/spool-experiment-100
```

The Rust and Go `corpus_experiment raw-read` helpers support measurement modes:

```bash
corpus_experiment raw-read --input /path/to/file.journal --output json --hash sha256 --binary-stats true
corpus_experiment raw-read --input /path/to/file.journal --output json --hash none --binary-stats true
corpus_experiment raw-read --input /path/to/file.journal --output json --hash none --binary-stats false
corpus_experiment raw-read --input /path/to/file.journal --output json --hash none --binary-stats false --separator-stats false
```

Use `--hash none --binary-stats false` for minimal row/payload/byte counting
when payload bytes should be discarded without hashing or content
classification. Add `--separator-stats false` when the measurement should not
scan payload bytes even to find the `FIELD=value` separator.

Durable `report.json`, `report.md`, and `state.json` records contain sanitized
file identifiers, counts, digests, status codes, and metrics only. They must
not contain raw field names, field values, messages, hostnames, IP addresses,
usernames, or binary payload dumps.
