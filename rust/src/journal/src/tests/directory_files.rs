//! `directory_files` lists exactly what a directory read opens.

use crate::directory_files;
use std::path::{Path, PathBuf};

/// Builds the systemd layout cases the listing must accept or ignore and
/// returns the expected sorted files.
fn layout(root: &Path) -> Vec<PathBuf> {
    let plain_id = "00112233445566778899aabbccddeeff";
    let dashed_id = "00112233-4455-6677-8899-AABBCCDDEEFF";
    for dir in [
        plain_id,
        dashed_id,
        "not-an-id",
        "00112233445566778899aabbccddeef",
        "0011223344556677.899aabbccddeeff",
        &format!("{plain_id}/nested"),
        "dir.journal",
    ] {
        std::fs::create_dir_all(root.join(dir)).expect("create layout directory");
    }
    let touch = |rel: &str| {
        let path = root.join(rel);
        std::fs::write(&path, b"").expect("create file");
        path
    };
    let mut want = vec![
        touch("system.journal"),
        touch("system@archived.journal~"),
        touch("whole.journal.zst"),
        touch("whole.journal~.zst"),
        touch(&format!("{plain_id}/user-1000.journal")),
        touch(&format!("{dashed_id}/system.journal")),
    ];
    #[cfg(unix)]
    {
        let target = touch("target.bin");
        std::os::unix::fs::symlink(target, root.join("linked.journal")).expect("symlink");
        want.push(root.join("linked.journal"));
    }
    for ignored in [
        "notes.log",
        "system.journal.bak",
        "not-an-id/ignored.journal",
        "00112233445566778899aabbccddeef/ignored.journal",
        "0011223344556677.899aabbccddeeff/ignored.journal",
        &format!("{plain_id}/nested/too-deep.journal"),
        &format!("{plain_id}/notes.log"),
    ] {
        touch(ignored);
    }
    want.sort();
    want
}

#[test]
fn directory_files_follows_systemd_layout() {
    let dir = tempfile::tempdir().expect("temp dir");
    let want = layout(dir.path());
    assert_eq!(directory_files(dir.path()).expect("list files"), want);
}

#[test]
fn directory_files_missing_directory_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let err = directory_files(dir.path().join("missing")).expect_err("missing directory");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}
