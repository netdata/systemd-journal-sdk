//! A relative watch directory must behave like the equivalent absolute one.

use journal_common::Seconds;
use journal_registry::{Monitor, Registry};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const MACHINE: &str = "00112233445566778899aabbccddeeff";

/// Builds a relative path to `target` without changing the working directory,
/// which would race with other tests in the same process.
fn relative_to_cwd(target: &Path) -> PathBuf {
    let cwd = std::env::current_dir().unwrap();
    let mut rel = PathBuf::new();
    for _ in cwd.components().skip(1) {
        rel.push("..");
    }
    rel.join(target.strip_prefix("/").unwrap())
}

fn registered(registry: &Registry) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = registry
        .find_files_in_range(Seconds(0), Seconds(u32::MAX))
        .unwrap()
        .into_iter()
        .map(|info| PathBuf::from(info.file.path()))
        .collect();
    paths.sort();
    paths
}

#[test]
fn relative_directory_registers_files_events_and_unwatch() {
    let dir = TempDir::new().unwrap();
    let rel = relative_to_cwd(dir.path());
    assert!(rel.is_relative());
    // Resolution is lexical, so registered paths keep the `..` components.
    let machine_dir = std::path::absolute(&rel).unwrap().join(MACHINE);
    std::fs::create_dir(&machine_dir).unwrap();
    let existing = machine_dir.join("system.journal");
    std::fs::write(&existing, b"").unwrap();
    let rel = rel.to_str().unwrap();

    let (monitor, mut events) = Monitor::new().unwrap();
    let registry = Registry::new(monitor);
    registry.watch_directory(rel).unwrap();
    assert_eq!(registered(&registry), vec![existing.clone()]);

    // Notify reports paths in the form they were watched; they must still parse.
    let created = machine_dir.join("user-1000.journal");
    std::fs::write(&created, b"").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while registered(&registry).len() < 2 {
        assert!(Instant::now() < deadline, "create event was not applied");
        match events.try_recv() {
            Ok(event) => registry.process_event(event).unwrap(),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut expected = vec![existing, created];
    expected.sort();
    assert_eq!(registered(&registry), expected);

    registry.unwatch_directory(rel).unwrap();
    assert!(registered(&registry).is_empty());
}

#[test]
fn empty_directory_is_refused() {
    let (monitor, _events) = Monitor::new().unwrap();
    let registry = Registry::new(monitor);
    assert!(registry.watch_directory("").is_err());
    assert!(registered(&registry).is_empty());
}
