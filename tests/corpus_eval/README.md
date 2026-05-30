# Real-World Journal Corpus Evaluation

This tooling evaluates real journal files incrementally without copying the
full corpus or writing raw journal content to durable reports.

Safe modes:

```bash
python tests/corpus_eval/run_corpus_eval.py --mode dry-run --root /path/to/journals --out .local/corpus-eval/dry-run
python tests/corpus_eval/run_corpus_eval.py --mode smoke --out .local/corpus-eval/smoke
```

Full corpus execution is intentionally guarded:

```bash
python tests/corpus_eval/run_corpus_eval.py --mode run --allow-full-run --root /path/to/journals --out .local/corpus-eval/full
```

Durable `report.json`, `report.md`, and `state.json` records contain sanitized
file identifiers, counts, digests, status codes, and metrics only. They must
not contain raw field names, field values, messages, hostnames, IP addresses,
usernames, or binary payload dumps.
