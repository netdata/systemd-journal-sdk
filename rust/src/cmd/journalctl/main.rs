use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};
use journal::{
    FacadeError, FileReader, OutputMode, SdJournal, SdJournalAddDisjunction, SdJournalAddMatch,
    SdJournalEnumerateFields, SdJournalGetEntry, SdJournalListBoots, SdJournalNext, SdJournalOpen,
    SdJournalPrevious, SdJournalProcessOutput, SdJournalSeekHead, SdJournalSeekTail,
    SdJournalSetOutputMode, parse_match_string, verify_file, verify_file_with_key,
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
    let directory_input = path.is_dir();
    if directory_input {
        files = collect_journal_files_for_verify(path)?;
    } else {
        files.push(path.to_path_buf());
    }

    if files.is_empty() {
        if directory_input {
            return Ok(());
        }
        return Err(anyhow!("verify: no journal files found"));
    }

    let mut first_err: Option<anyhow::Error> = None;
    for file in &files {
        let sealed = match is_file_sealed(file) {
            Ok(v) => v,
            Err(err) => {
                if directory_input {
                    continue;
                }
                eprintln!("FAIL: {} ({err})", file.display());
                if first_err.is_none() {
                    first_err = Some(err);
                }
                continue;
            }
        };

        if sealed && verify_key.is_some() {
            match verify_file_with_key(file, verify_key.unwrap()) {
                Ok(()) => eprintln!("PASS: {}", file.display()),
                Err(err) => {
                    eprintln!("FAIL: {} ({err})", file.display());
                    if first_err.is_none() {
                        first_err = Some(anyhow!("{err}"));
                    }
                }
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

fn collect_journal_files_for_verify(path: &Path) -> Result<Vec<PathBuf>> {
    let entries: Vec<_> = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    let mut files = Vec::new();

    for entry in &entries {
        let file_path = entry.path();
        if file_path.is_file() && is_journal_file_name(&file_path) {
            files.push(file_path);
        }
    }

    for entry in &entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_journal_subdir_name(&name) {
            continue;
        }
        let child_path = entry.path();
        if !child_path.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&child_path) else {
            continue;
        };
        for child in children.flatten() {
            let file_path = child.path();
            if file_path.is_file() && is_journal_file_name(&file_path) {
                files.push(file_path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn is_journal_subdir_name(name: &str) -> bool {
    if name.contains('.') {
        return false;
    }
    id128_string_valid(name)
}

fn id128_string_valid(s: &str) -> bool {
    match s.len() {
        32 => s.bytes().all(|byte| byte.is_ascii_hexdigit()),
        36 => s.bytes().enumerate().all(|(idx, byte)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        }),
        _ => false,
    }
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
    let (end, ok) = consume_hex(bytes, next + 1);
    ok && end == bytes.len() && bytes[next + 1..end].iter().any(|b| *b != b'0')
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
    use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
    use journal_core::repository::File as RepoFile;
    use journal_core::seal::SealOptions;
    use std::path::Path;

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
        run_verify(dir.path(), None).expect("empty directory verification should pass");
    }

    fn test_uuid(n: u8) -> uuid::Uuid {
        let mut bytes = [0u8; 16];
        bytes[15] = n;
        uuid::Uuid::from_bytes(bytes)
    }

    fn verification_key(opts: &SealOptions) -> String {
        let seed_hex = opts
            .seed
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        let start = opts.start_usec / opts.interval_usec;
        format!(
            "{seed_hex}/{start:x}-{interval:x}",
            interval = opts.interval_usec
        )
    }

    fn write_sealed_file(path: &Path) -> SealOptions {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path)
            .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
        let seal = SealOptions::new([0u8; 12], 1_000_000, 1_000_000);
        let mut journal_file = JournalFile::<MmapMut>::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=sealed verify".as_slice()],
                1_500_000,
                100,
            )
            .expect("write sealed entry");
        journal_file.sync().expect("sync journal");
        seal
    }

    #[test]
    fn verify_sealed_without_key_requires_key() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir
            .path()
            .join("00000000-0000-0000-0000-000000000001/system.journal");
        write_sealed_file(&path);
        let err = run_verify(&path, None).expect_err("verify should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("verification key"),
            "expected verification key error, got: {msg}"
        );
    }

    #[test]
    fn verify_key_sealed_passes_and_wrong_key_fails() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir
            .path()
            .join("00000000-0000-0000-0000-000000000001/system.journal");
        let seal = write_sealed_file(&path);
        let key = verification_key(&seal);

        run_verify(&path, Some(&key)).expect("sealed verify with key should pass");
        run_verify(&path, Some("000000000000000000000001/1-f4240"))
            .expect_err("wrong verification key should fail");
    }
}
