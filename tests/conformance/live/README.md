# Live Concurrency Harness

This directory contains reusable compatibility helpers for the required
one-writer/multiple-reader journal contract.

The harness validates that a repository writer can append to a journal file
while stock readers observe the same file safely:

- stock `journalctl --file` polling readers;
- stock `journalctl --file --follow --no-tail --boot=all` readers;
- stock libsystemd readers built from `libsystemd_live_reader.c`.

Writers are invoked as external commands. A writer command must:

- create or open the journal path passed by the language-specific test;
- write at least one entry;
- create the ready-file path after the first entry is committed;
- continue appending until the requested entry count is reached;
- include a monotonically increasing `LIVE_SEQ` field starting at `000000`;
- exit with the expected status.

All entries used by this harness are synthetic. The harness must not read host
journals or durable runtime data.

The default match is `PRIORITY=6`, and the default sequence field is `LIVE_SEQ`.
The sequence field is configurable so the same stock-reader oracles can be
reused by every language writer test.

Polling and follow `journalctl --file` readers, plus stock libsystemd readers,
retry transient active-writer `ENODATA` open/read failures or partial snapshots.
After the writer exits, the same reader must observe a complete ordered snapshot
or stream, and `journalctl --verify` must pass.
