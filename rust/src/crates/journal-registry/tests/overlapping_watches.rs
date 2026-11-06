//! Unwatch forgets exactly the files no remaining watch covers, and
//! overlapping watches keep working in either order.

use journal_common::Seconds;
use journal_registry::{Monitor, Registry};
use notify::Event;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::mpsc::UnboundedReceiver;

const MACHINE_A: &str = "00112233445566778899aabbccddeeff";
const MACHINE_B: &str = "ffeeddccbbaa99887766554433221100";

struct Fixture {
    dir: TempDir,
    registry: Registry,
    events: UnboundedReceiver<Event>,
}

impl Fixture {
    /// A root with two machine directories, each holding one active file.
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        for machine in [MACHINE_A, MACHINE_B] {
            std::fs::create_dir(dir.path().join(machine)).unwrap();
            std::fs::write(dir.path().join(machine).join("system.journal"), b"").unwrap();
        }
        let (monitor, events) = Monitor::new().unwrap();
        Self {
            dir,
            registry: Registry::new(monitor),
            events,
        }
    }

    fn root(&self) -> String {
        self.dir.path().to_str().unwrap().to_string()
    }

    fn machine(&self, machine: &str) -> PathBuf {
        self.dir.path().join(machine)
    }

    fn machine_str(&self, machine: &str) -> String {
        self.machine(machine).to_str().unwrap().to_string()
    }

    fn registered(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self
            .registry
            .find_files_in_range(Seconds(0), Seconds(u32::MAX))
            .unwrap()
            .into_iter()
            .map(|info| PathBuf::from(info.file.path()))
            .collect();
        paths.sort();
        paths
    }

    fn expect(&self, files: &[&Path]) {
        let mut expected: Vec<PathBuf> = files.iter().map(|p| p.to_path_buf()).collect();
        expected.sort();
        assert_eq!(self.registered(), expected);
    }

    /// Applies queued events until `path` is registered.
    fn wait_registered(&mut self, path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.registered().iter().any(|p| p == path) {
            assert!(Instant::now() < deadline, "no event registered {path:?}");
            self.drain_once();
        }
    }

    /// Applies queued events until `path` is no longer registered.
    fn wait_forgotten(&mut self, path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.registered().iter().any(|p| p == path) {
            assert!(Instant::now() < deadline, "no event removed {path:?}");
            self.drain_once();
        }
    }

    fn drain_once(&mut self) {
        match self.events.try_recv() {
            Ok(event) => self.registry.process_event(event).unwrap(),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }

    /// Creates a file and waits for its create event to register it.
    fn create_and_wait(&mut self, machine: &str, name: &str) -> PathBuf {
        let path = self.machine(machine).join(name);
        std::fs::write(&path, b"").unwrap();
        self.wait_registered(&path);
        path
    }
}

fn active(fixture: &Fixture, machine: &str) -> PathBuf {
    fixture.machine(machine).join("system.journal")
}

#[test]
fn unwatch_with_trailing_slash_forgets_files() {
    let f = Fixture::new();
    let with_slash = format!("{}/", f.root());
    f.registry.watch_directory(&with_slash).unwrap();
    f.expect(&[&active(&f, MACHINE_A), &active(&f, MACHINE_B)]);

    f.registry.unwatch_directory(&with_slash).unwrap();
    f.expect(&[]);

    // The slash is not part of the key: either spelling names the same watch.
    f.registry.watch_directory(&with_slash).unwrap();
    f.registry.watch_directory(&f.root()).unwrap();
    f.registry.unwatch_directory(&f.root()).unwrap();
    f.expect(&[]);
}

#[test]
fn unwatch_of_machine_directory_forgets_its_files() {
    let f = Fixture::new();
    let machine = f.machine_str(MACHINE_A);
    f.registry.watch_directory(&machine).unwrap();
    f.expect(&[&active(&f, MACHINE_A)]);

    f.registry.unwatch_directory(&machine).unwrap();
    f.expect(&[]);
}

#[test]
fn parent_then_child_unwatch_parent_keeps_child_live() {
    let mut f = Fixture::new();
    let (root, child) = (f.root(), f.machine_str(MACHINE_A));
    f.registry.watch_directory(&root).unwrap();
    f.registry.watch_directory(&child).unwrap();
    f.expect(&[&active(&f, MACHINE_A), &active(&f, MACHINE_B)]);

    f.registry.unwatch_directory(&root).unwrap();
    f.expect(&[&active(&f, MACHINE_A)]);

    // The child got its own notify watch back.
    let created = f.create_and_wait(MACHINE_A, "user-1000.journal");
    f.expect(&[&active(&f, MACHINE_A), &created]);

    f.registry.unwatch_directory(&child).unwrap();
    f.expect(&[]);
}

#[test]
fn parent_then_child_unwatch_child_keeps_parent_files() {
    let mut f = Fixture::new();
    let (root, child) = (f.root(), f.machine_str(MACHINE_A));
    f.registry.watch_directory(&root).unwrap();
    f.registry.watch_directory(&child).unwrap();

    f.registry.unwatch_directory(&child).unwrap();
    f.expect(&[&active(&f, MACHINE_A), &active(&f, MACHINE_B)]);

    // The parent's recursive watch still covers the child directory.
    let created = f.create_and_wait(MACHINE_A, "user-1000.journal");
    f.expect(&[&active(&f, MACHINE_A), &active(&f, MACHINE_B), &created]);

    f.registry.unwatch_directory(&root).unwrap();
    f.expect(&[]);
}

#[test]
fn child_then_parent_both_orders_stay_live() {
    let mut f = Fixture::new();
    let (root, child) = (f.root(), f.machine_str(MACHINE_A));
    f.registry.watch_directory(&child).unwrap();
    f.expect(&[&active(&f, MACHINE_A)]);
    f.registry.watch_directory(&root).unwrap();
    f.expect(&[&active(&f, MACHINE_A), &active(&f, MACHINE_B)]);

    // The parent took over from the child's watch; both subtrees are live.
    let in_b = f.create_and_wait(MACHINE_B, "user-1000.journal");
    let in_a = f.create_and_wait(MACHINE_A, "user-1000.journal");

    f.registry.unwatch_directory(&root).unwrap();
    f.expect(&[&active(&f, MACHINE_A), &in_a]);

    std::fs::remove_file(&in_a).unwrap();
    f.wait_forgotten(&in_a);
    f.expect(&[&active(&f, MACHINE_A)]);
    assert!(in_b.exists(), "files on disk are never touched");

    f.registry.unwatch_directory(&child).unwrap();
    f.expect(&[]);
}

#[test]
fn failed_unwatch_changes_nothing_and_never_sticks() {
    let mut f = Fixture::new();
    let (root, child) = (f.root(), f.machine_str(MACHINE_A));
    f.registry.watch_directory(&root).unwrap();
    f.registry.watch_directory(&child).unwrap();

    // With the nested directory gone, its watch cannot be re-armed when the
    // parent goes away, so unwatching the parent must fail without effect.
    std::fs::remove_dir_all(f.machine(MACHINE_A)).unwrap();
    f.wait_forgotten(&active(&f, MACHINE_A));
    assert!(f.registry.unwatch_directory(&root).is_err());
    f.expect(&[&active(&f, MACHINE_B)]);
    let created = f.create_and_wait(MACHINE_B, "user-1000.journal");

    // Unwatching the vanished nested directory first resolves it.
    f.registry.unwatch_directory(&child).unwrap();
    f.expect(&[&active(&f, MACHINE_B), &created]);
    f.registry.unwatch_directory(&root).unwrap();
    f.expect(&[]);

    // Nothing is stuck: both can be watched again.
    std::fs::create_dir(f.machine(MACHINE_A)).unwrap();
    f.registry.watch_directory(&child).unwrap();
    f.registry.watch_directory(&root).unwrap();
    f.expect(&[&active(&f, MACHINE_B), &created]);
}

#[test]
fn failed_watch_changes_nothing() {
    let mut f = Fixture::new();
    let child = f.machine_str(MACHINE_A);
    f.registry.watch_directory(&child).unwrap();

    let missing = f.dir.path().join("missing");
    assert!(
        f.registry
            .watch_directory(missing.to_str().unwrap())
            .is_err()
    );
    f.expect(&[&active(&f, MACHINE_A)]);
    let created = f.create_and_wait(MACHINE_A, "user-1000.journal");
    f.expect(&[&active(&f, MACHINE_A), &created]);
}

#[test]
fn unwatch_after_watched_directory_vanished_succeeds() {
    let mut f = Fixture::new();
    let child = f.machine_str(MACHINE_A);
    f.registry.watch_directory(&child).unwrap();

    std::fs::remove_dir_all(f.machine(MACHINE_A)).unwrap();
    f.wait_forgotten(&active(&f, MACHINE_A));
    f.registry.unwatch_directory(&child).unwrap();
    f.expect(&[]);

    std::fs::create_dir(f.machine(MACHINE_A)).unwrap();
    f.registry.watch_directory(&child).unwrap();
    f.expect(&[]);
}

#[test]
fn unwatch_root_after_subdirectory_deleted_succeeds() {
    // notify 8.2 keeps a stale entry for the deleted subdirectory; the
    // registry must still unwatch the root and be able to watch again.
    let mut f = Fixture::new();
    let root = f.root();
    f.registry.watch_directory(&root).unwrap();
    std::fs::remove_dir_all(f.machine(MACHINE_A)).unwrap();
    f.wait_forgotten(&active(&f, MACHINE_A));

    f.registry.unwatch_directory(&root).unwrap();
    f.expect(&[]);

    f.registry.watch_directory(&root).unwrap();
    f.expect(&[&active(&f, MACHINE_B)]);
    let created = f.create_and_wait(MACHINE_B, "user-1000.journal");
    f.expect(&[&active(&f, MACHINE_B), &created]);
}

#[test]
fn queued_events_do_not_restore_unwatched_files() {
    let mut f = Fixture::new();
    let root = f.root();
    f.registry.watch_directory(&root).unwrap();
    let late = f.machine(MACHINE_A).join("user-1000.journal");
    std::fs::write(&late, b"").unwrap();

    // Hold the create event back until after the unwatch.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut held = Vec::new();
    while !held.iter().any(|e: &Event| e.paths.contains(&late)) {
        assert!(Instant::now() < deadline, "no create event for {late:?}");
        match f.events.try_recv() {
            Ok(event) => held.push(event),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    f.registry.unwatch_directory(&root).unwrap();

    for event in held {
        f.registry.process_event(event).unwrap();
    }
    f.expect(&[]);
}
