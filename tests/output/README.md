# Entry Output Goldens

`entry-output.journal.zst` holds three synthetic entries that exercise
systemd's export and JSON rules: repeated fields, binary and newline values,
C1 controls, Unicode noncharacters, values at and above the 4096-byte JSON
threshold, a `_BOOT_ID` DATA field, protected `__` names, and invalid field
names (export stops at them; JSON skips them).

The goldens are the output of systemd v262 `journalctl` (tag `v262`,
`8cc40e0c5e9234bf45084751ac53b1fbfe70b492`) on the uncompressed file:

| Golden | Options |
|---|---|
| `export.golden` | `-o export` |
| `export-fields.golden` | `-o export --output-fields=MESSAGE,MULTI,BIN,__CURSOR,_BOOT_ID,AFTER,LAST` |
| `json.golden` | `-o json` |
| `json-all.golden` | `-o json --all` |
| `json-fields.golden` | `-o json --output-fields=` (same list) |
| `json-pretty.golden` | `-o json-pretty` |

systemd orders JSON keys by hash-table iteration, so tests compare JSON by
value and export byte for byte.

Regenerate the fixture from `go/` with
`go run ./internal/testcmd/entry_output_fixture ../tests/output/entry-output.journal`,
rerun the commands above, then `zstd -19` the journal and delete the
uncompressed copy.
