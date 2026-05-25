use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};
use journal::{
    FacadeError, FileReader, OutputMode, SdJournal, SdJournalAddDisjunction, SdJournalAddMatch,
    SdJournalEnumerateFields, SdJournalGetEntry, SdJournalListBoots, SdJournalNext, SdJournalOpen,
    SdJournalPrevious, SdJournalProcessOutput, SdJournalSeekHead, SdJournalSeekTail,
    SdJournalSetOutputMode, parse_match_string, verify_file,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

// HEADER_COMPATIBLE_SEALED from systemd journal-def.h
const COMPATIBLE_SEALED: u32 = 1;

#[derive(Parser, Debug)]
#[command(name = "journalctl")]
#[command(about = "Pure-Rust file-backed journalctl subset")]
struct Args {
    #[arg(long = "file")]
    file: Option<PathBuf>,
    #[arg(long = "directory")]
    directory: Option<PathBuf>,
    #[arg(long = "output", default_value = "default")]
    output: OutputModeArg,
    #[arg(long = "list-boots")]
    list_boots: bool,
    #[arg(long = "fields")]
    fields: bool,
    #[arg(long = "head")]
    head: Option<usize>,
    #[arg(long = "tail")]
    tail: Option<usize>,
    #[arg(long = "follow")]
    follow: bool,
    #[arg(long = "sync")]
    sync: bool,
    #[arg(long = "flush")]
    flush: bool,
    #[arg(long = "rotate")]
    rotate: bool,
    #[arg(long = "relinquish-var")]
    relinquish_var: bool,
    #[arg(long = "verify")]
    verify: bool,
    #[arg(long = "verify-only")]
    verify_only: bool,
    #[arg(long = "verify-key")]
    verify_key: Option<String>,
    #[arg(trailing_var_arg = true)]
    matches: Vec<String>,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputModeArg {
    #[value(alias = "short")]
    Default,
    Json,
    Export,
}

impl From<OutputModeArg> for OutputMode {
    fn from(value: OutputModeArg) -> Self {
        match value {
            OutputModeArg::Default => Self::Default,
            OutputModeArg::Json => Self::Json,
            OutputModeArg::Export => Self::Export,
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    if args.follow || args.sync || args.flush || args.rotate || args.relinquish_var {
        return Err(anyhow!("{}", FacadeError::Unsupported));
    }

    let path = args
        .file
        .as_ref()
        .or(args.directory.as_ref())
        .ok_or_else(|| anyhow!("use --file or --directory"))?;

    if args.verify || args.verify_only || args.verify_key.is_some() {
        return run_verify(path, args.verify_key.as_deref());
    }

    let mut journal =
        SdJournalOpen(&path.to_string_lossy(), 0).map_err(|err| anyhow!("open: {err}"))?;
    apply_matches(&mut journal, &args.matches)?;
    SdJournalSetOutputMode(&mut journal, args.output.into());

    if args.list_boots {
        for boot in SdJournalListBoots(&mut journal).map_err(|err| anyhow!("list boots: {err}"))? {
            println!(
                "{:>2} {} {} {}",
                boot.index, boot.boot_id, boot.first_entry, boot.last_entry
            );
        }
        return Ok(());
    }

    if args.fields {
        let mut fields =
            SdJournalEnumerateFields(&mut journal).map_err(|err| anyhow!("fields: {err}"))?;
        fields.sort();
        for field in fields {
            println!("{field}");
        }
        return Ok(());
    }

    if let Some(n) = args.tail {
        show_tail(&mut journal, n)
    } else {
        show_head_or_all(&mut journal, args.head)
    }
}

fn run_verify(path: &Path, verify_key: Option<&str>) -> Result<()> {
    if verify_key.is_some_and(|key| !valid_verification_key(key)) {
        eprintln!("Failed to parse seed.");
        return Err(anyhow!("failed to parse seed"));
    }

    let mut files = Vec::new();
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter(|e| is_journal_file_name(&e.path()))
            .map(|e| e.path())
            .collect();
        entries.sort();
        files = entries;
    } else {
        files.push(path.to_path_buf());
    }

    if files.is_empty() {
        return Err(anyhow!("verify: no journal files found"));
    }

    let mut first_err: Option<anyhow::Error> = None;
    for file in &files {
        let sealed = match is_file_sealed(file) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("FAIL: {} ({err})", file.display());
                if first_err.is_none() {
                    first_err = Some(err);
                }
                continue;
            }
        };

        if sealed && verify_key.is_some() {
            eprintln!(
                "FAIL: {} (sealed FSS verification is not yet implemented)",
                file.display()
            );
            if first_err.is_none() {
                first_err = Some(anyhow!("sealed FSS verification is not yet implemented"));
            }
            continue;
        }

        if sealed && verify_key.is_none() {
            eprintln!(
                "Journal file {} has sealing enabled but verification key has not been passed using --verify-key=.",
                file.display()
            );
            eprintln!(
                "FAIL: {} (verification key required for sealed journal file)",
                file.display()
            );
            if first_err.is_none() {
                first_err = Some(anyhow!("verification key required for sealed journal file"));
            }
            continue;
        }

        if let Err(err) = verify_file(file) {
            eprintln!("FAIL: {} ({err})", file.display());
            if first_err.is_none() {
                first_err = Some(anyhow!("{err}"));
            }
            continue;
        }
        eprintln!("PASS: {}", file.display());
    }

    match first_err {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn is_file_sealed(path: &Path) -> Result<bool> {
    let reader = FileReader::open(path).map_err(|err| anyhow!("open: {err}"))?;
    Ok(reader.header().compatible_flags & COMPATIBLE_SEALED != 0)
}

fn is_journal_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".journal")
                || name.ends_with(".journal~")
                || name.ends_with(".journal.zst")
                || name.ends_with(".journal~.zst")
        })
}

fn valid_verification_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut i = 0usize;
    for _ in 0..12 {
        while i < bytes.len() && bytes[i] == b'-' {
            i += 1;
        }
        if i + 2 > bytes.len() || !is_hex(bytes[i]) || !is_hex(bytes[i + 1]) {
            return false;
        }
        i += 2;
    }
    if i >= bytes.len() || bytes[i] != b'/' {
        return false;
    }
    i += 1;

    let (next, ok) = consume_hex(bytes, i);
    if !ok || next >= bytes.len() || bytes[next] != b'-' {
        return false;
    }
    let (_, ok) = consume_hex(bytes, next + 1);
    ok
}

fn consume_hex(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut i = start;
    while i < bytes.len() && is_hex(bytes[i]) {
        i += 1;
    }
    (i, i > start)
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn apply_matches(journal: &mut SdJournal, matches: &[String]) -> Result<()> {
    for item in matches {
        if item == "+" {
            SdJournalAddDisjunction(journal).map_err(|err| anyhow!("add disjunction: {err}"))?;
            continue;
        }
        let data =
            parse_match_string(item).map_err(|err| anyhow!("invalid match {item}: {err}"))?;
        SdJournalAddMatch(journal, &data).map_err(|err| anyhow!("add match: {err}"))?;
    }
    Ok(())
}

fn show_head_or_all(journal: &mut SdJournal, limit: Option<usize>) -> Result<()> {
    SdJournalSeekHead(journal).map_err(|err| anyhow!("seek head: {err}"))?;
    let mut shown = 0usize;
    loop {
        if limit.is_some_and(|limit| shown >= limit) {
            return Ok(());
        }
        match SdJournalNext(journal).map_err(|err| anyhow!("next: {err}"))? {
            0 => return Ok(()),
            _ => {
                write_current(journal)?;
                shown += 1;
            }
        }
    }
}

fn show_tail(journal: &mut SdJournal, limit: usize) -> Result<()> {
    SdJournalSeekTail(journal).map_err(|err| anyhow!("seek tail: {err}"))?;
    let mut entries = Vec::new();
    loop {
        if entries.len() >= limit {
            break;
        }
        match SdJournalPrevious(journal).map_err(|err| anyhow!("previous: {err}"))? {
            0 => break,
            _ => {
                let entry =
                    SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
                let output = SdJournalProcessOutput(journal, &entry)
                    .map_err(|err| anyhow!("output: {err}"))?;
                entries.push(output);
            }
        }
    }

    let mut stdout = std::io::stdout().lock();
    for entry in entries.iter().rev() {
        stdout.write_all(entry)?;
    }
    Ok(())
}

fn write_current(journal: &mut SdJournal) -> Result<()> {
    let entry = SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
    let output = SdJournalProcessOutput(journal, &entry).map_err(|err| anyhow!("output: {err}"))?;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_FSS_VERIFICATION_KEY: &str = "c262bd-85187f-0b1b04-877cc5/1c7af8-35a4e900";

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../")
            .canonicalize()
            .expect("repo root")
    }

    #[test]
    fn verify_valid_fixture() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        run_verify(&path, None).expect("verify should pass");
    }

    #[test]
    fn verify_corrupted_fixture() {
        let path =
            repo_root().join("fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst");
        let err = run_verify(&path, None).expect_err("verify should fail");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("corrupt") || msg.to_lowercase().contains("fail"),
            "expected corrupt/fail in error, got: {msg}"
        );
    }

    #[test]
    fn verify_key_unsealed_fixture() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        run_verify(&path, Some(VALID_FSS_VERIFICATION_KEY)).expect("verify should pass");
    }

    #[test]
    fn verify_key_invalid_seed() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        let err = run_verify(&path, Some("synthetic-test-key")).expect_err("verify should fail");
        assert!(
            err.to_string().contains("failed to parse seed"),
            "expected parse seed error, got: {err}"
        );
    }

    #[test]
    fn verify_key_empty_seed() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        let err = run_verify(&path, Some("")).expect_err("verify should fail");
        assert!(
            err.to_string().contains("failed to parse seed"),
            "expected parse seed error, got: {err}"
        );
    }

    #[test]
    fn verify_directory_follows_symlink_and_skips_directories() {
        let fixture = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        let dir = tempfile::tempdir().expect("create temp dir");
        let linked = dir.path().join("linked.journal.zst");
        std::os::unix::fs::symlink(&fixture, &linked).expect("symlink fixture");
        std::fs::create_dir(dir.path().join("skip.journal.zst")).expect("create skipped dir");

        run_verify(dir.path(), None).expect("directory verification should pass");
    }

    #[test]
    fn verify_directory_empty() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let err = run_verify(dir.path(), None).expect_err("verify should fail");
        assert!(
            err.to_string().contains("no journal files found"),
            "expected no journal files error, got: {err}"
        );
    }

    fn sealed_fixture_copy() -> tempfile::NamedTempFile {
        let fixture = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");

        // Decompress the zst fixture so we can patch the raw journal header
        let compressed = std::fs::read(&fixture).expect("read fixture bytes");
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(&compressed[..]).expect("create zstd decoder");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut bytes).expect("decompress fixture");

        // Patch compatible_flags to set HEADER_COMPATIBLE_SEALED
        let mut flags = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        flags |= COMPATIBLE_SEALED;
        bytes[8..12].copy_from_slice(&flags.to_le_bytes());
        let local = repo_root().join(".local/journalctl-tests");
        std::fs::create_dir_all(&local).expect("create .local temp dir");
        let tmp = tempfile::Builder::new()
            .prefix("journalctl-verify-sealed-")
            .suffix(".journal")
            .tempfile_in(local)
            .expect("create sealed fixture temp file");
        std::fs::write(tmp.path(), bytes).expect("write patched journal");
        tmp
    }

    #[test]
    fn verify_sealed_without_key_requires_key() {
        let tmp = sealed_fixture_copy();
        let err = run_verify(tmp.path(), None).expect_err("verify should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("verification key"),
            "expected verification key error, got: {msg}"
        );
    }

    #[test]
    fn verify_key_sealed_unsupported() {
        let tmp = sealed_fixture_copy();
        let err = run_verify(tmp.path(), Some(VALID_FSS_VERIFICATION_KEY))
            .expect_err("verify should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("not yet implemented"),
            "expected 'not yet implemented', got: {msg}"
        );
    }
}
