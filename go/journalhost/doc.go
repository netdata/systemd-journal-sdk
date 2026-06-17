// Package journalhost is the optional host identity and monotonic clock helper
// for the systemd journal SDK.
//
// Core journal readers and writers in the sibling package journal are
// OS-agnostic and do not discover host identity, read host identity files,
// execute subprocesses, or lock writer files. The strict writer contract
// requires callers to pass machine_id, boot_id, and per-entry monotonic
// timestamps explicitly. journalhost is the only sanctioned way to obtain
// local-host values for callers that intentionally want the collector host as
// the event identity source. Callers still pass the helper's values
// explicitly to the core writer; the helper does not wrap the writer.
//
// journalhost is pure Go (no CGO), uses only native OS APIs, and never
// shells out to external programs. It supports Linux, FreeBSD, macOS, and
// Windows. Boot ID discovery is native-first: native Linux
// /proc/sys/kernel/random/boot_id, native FreeBSD kern.boot_id on 13+, and
// native macOS kern.bootsessionuuid. Windows and FreeBSD 12 or environments
// where kern.boot_id is unavailable use a locked state-backed synthesis
// file. State/discovery failures generate a fresh boot ID for this provider
// instance and continue with degraded diagnostics instead of hard-failing.
package journalhost
