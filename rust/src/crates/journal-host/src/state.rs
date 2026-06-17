use crate::{BootIdSource, uuid_compact};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

const MIN_REBOOT_TIME_USEC: u64 = 30_000_000;

pub(crate) struct StateBackedProbe {
    pub marker_now: Arc<dyn Fn() -> io::Result<u64> + Send + Sync>,
    pub realtime_now: Arc<dyn Fn() -> io::Result<u64> + Send + Sync>,
    pub new_uuid: Arc<dyn Fn() -> io::Result<Uuid> + Send + Sync>,
}

pub(crate) struct StateBackedOutcome {
    pub boot_id: Uuid,
    pub path: PathBuf,
    pub source: BootIdSource,
    pub degraded_reason: Option<String>,
}

pub(crate) fn load_state_backed_boot_id(
    path: &Path,
    probe: StateBackedProbe,
) -> io::Result<StateBackedOutcome> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if let Err(err) = fs::create_dir_all(parent) {
        return degraded_fresh(path, &probe, format!("mkdir state dir: {err}"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }

    let lock_path = path.with_extension(format!(
        "{}lock",
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| format!("{value}."))
            .unwrap_or_default()
    ));
    let _lock = match StateLock::acquire(&lock_path) {
        Ok(lock) => lock,
        Err(err) => return degraded_fresh(path, &probe, format!("acquire lock: {err}")),
    };

    match read_state_file(path) {
        Ok(Some(existing)) => {
            let estimated = match estimate_boottime_usec(&probe) {
                Ok(estimated) => estimated,
                Err(err) => {
                    return degraded_fresh(path, &probe, format!("estimate boot time: {err}"));
                }
            };
            if estimated
                > existing
                    .last_estimated_boottime
                    .saturating_add(MIN_REBOOT_TIME_USEC)
            {
                return write_fresh_state(path, &probe, None);
            }
            Ok(StateBackedOutcome {
                boot_id: existing.last_boot_id,
                path: path.to_path_buf(),
                source: BootIdSource::StateBacked,
                degraded_reason: None,
            })
        }
        Ok(None) => write_fresh_state(path, &probe, None),
        Err(err) => {
            let _ = preserve_corrupt_state(path);
            write_fresh_state(path, &probe, Some(format!("read state: {err}")))
        }
    }
}

struct StateFile {
    last_estimated_boottime: u64,
    last_boot_id: Uuid,
}

fn read_state_file(path: &Path) -> io::Result<Option<StateFile>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let mut last_estimated_boottime = None;
    let mut last_boot_id = None;
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(value) = line.strip_prefix("last_estimated_boottime=") {
            let parsed = value.parse::<u64>().map_err(|err| {
                io::Error::new(io::ErrorKind::InvalidData, format!("parse boottime: {err}"))
            })?;
            last_estimated_boottime = Some(parsed);
        } else if let Some(value) = line.strip_prefix("last_boot_id=") {
            let parsed = crate::parse_uuid_text(value)?;
            last_boot_id = Some(parsed);
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown state field: {line}"),
            ));
        }
    }
    let Some(last_estimated_boottime) = last_estimated_boottime else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing last_estimated_boottime",
        ));
    };
    let Some(last_boot_id) = last_boot_id else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing last_boot_id",
        ));
    };
    Ok(Some(StateFile {
        last_estimated_boottime,
        last_boot_id,
    }))
}

fn write_fresh_state(
    path: &Path,
    probe: &StateBackedProbe,
    degraded_reason: Option<String>,
) -> io::Result<StateBackedOutcome> {
    let boot_id = (probe.new_uuid)()?;
    let estimated = match estimate_boottime_usec(probe) {
        Ok(estimated) => estimated,
        Err(err) => {
            return Ok(StateBackedOutcome {
                boot_id,
                path: path.to_path_buf(),
                source: BootIdSource::Degraded,
                degraded_reason: Some(format!("estimate boot time: {err}")),
            });
        }
    };
    let contents = format!(
        "last_estimated_boottime={estimated}\nlast_boot_id={}\n",
        uuid_compact(boot_id)
    );
    match write_state_file_atomic(path, contents.as_bytes()) {
        Ok(()) => Ok(StateBackedOutcome {
            boot_id,
            path: path.to_path_buf(),
            source: if degraded_reason.is_some() {
                BootIdSource::Degraded
            } else {
                BootIdSource::StateBacked
            },
            degraded_reason,
        }),
        Err(err) => Ok(StateBackedOutcome {
            boot_id,
            path: path.to_path_buf(),
            source: BootIdSource::Degraded,
            degraded_reason: Some(format!("write state: {err}")),
        }),
    }
}

fn degraded_fresh(
    path: &Path,
    probe: &StateBackedProbe,
    reason: String,
) -> io::Result<StateBackedOutcome> {
    let boot_id = (probe.new_uuid)()?;
    Ok(StateBackedOutcome {
        boot_id,
        path: path.to_path_buf(),
        source: BootIdSource::Degraded,
        degraded_reason: Some(reason),
    })
}

fn estimate_boottime_usec(probe: &StateBackedProbe) -> io::Result<u64> {
    let realtime = (probe.realtime_now)()?;
    let marker = (probe.marker_now)()?;
    Ok(realtime.saturating_sub(marker))
}

fn write_state_file_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("bootid.state");
    let tmp_path = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let mut file = create_private_file(&tmp_path)?;
    let cleanup = TempPathCleanup(tmp_path.clone());
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp_path, path)?;
    cleanup.disarm();
    fsync_directory_best_effort(parent);
    Ok(())
}

fn create_private_file(path: &Path) -> io::Result<File> {
    let _ = fs::remove_file(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_CLOEXEC);
        options.mode(0o600);
    }
    options.open(path)
}

fn preserve_corrupt_state(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let corrupt_path = path.with_extension(format!(
        "{}corrupt",
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| format!("{value}."))
            .unwrap_or_default()
    ));
    let _ = fs::remove_file(&corrupt_path);
    fs::rename(path, corrupt_path)
}

struct TempPathCleanup(PathBuf);

impl TempPathCleanup {
    fn disarm(self) {
        std::mem::forget(self);
    }
}

impl Drop for TempPathCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(unix)]
fn fsync_directory_best_effort(path: &Path) {
    if let Ok(file) = File::open(path) {
        let _ = file.sync_all();
    }
}

#[cfg(windows)]
fn fsync_directory_best_effort(_path: &Path) {}

struct StateLock {
    #[allow(dead_code)]
    file: File,
}

impl StateLock {
    fn acquire(path: &Path) -> io::Result<Self> {
        let file = create_private_lock_file(path)?;
        lock_file(&file)?;
        Ok(Self { file })
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = unlock_file(&self.file);
    }
}

fn create_private_lock_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_CLOEXEC);
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(unix)]
fn lock_file(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: flock operates on a valid file descriptor owned by `file`.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn unlock_file(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: flock operates on a valid file descriptor owned by `file`.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn lock_file(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LockFileEx};
    use windows_sys::Win32::System::IO::OVERLAPPED;
    let mut overlapped = OVERLAPPED::default();
    // SAFETY: LockFileEx receives a valid file handle and stack OVERLAPPED.
    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn unlock_file(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
    use windows_sys::Win32::System::IO::OVERLAPPED;
    let mut overlapped = OVERLAPPED::default();
    // SAFETY: UnlockFileEx receives a valid file handle and stack OVERLAPPED.
    let ok = unsafe { UnlockFileEx(file.as_raw_handle(), 0, 1, 0, &mut overlapped) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn probe(
        marker: Arc<AtomicU64>,
        realtime: Arc<AtomicU64>,
        uuids: Arc<Mutex<Vec<Uuid>>>,
    ) -> StateBackedProbe {
        StateBackedProbe {
            marker_now: Arc::new(move || Ok(marker.load(Ordering::SeqCst))),
            realtime_now: Arc::new(move || Ok(realtime.load(Ordering::SeqCst))),
            new_uuid: Arc::new(move || {
                uuids
                    .lock()
                    .unwrap()
                    .pop()
                    .ok_or_else(|| io::Error::other("uuid queue empty"))
            }),
        }
    }

    #[test]
    fn state_backed_reuses_same_boot_id() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bootid.state");
        let marker = Arc::new(AtomicU64::new(10_000_000));
        let realtime = Arc::new(AtomicU64::new(1_000_000_000));
        let uuids = Arc::new(Mutex::new(vec![Uuid::from_u128(1)]));
        let first = load_state_backed_boot_id(
            &path,
            probe(
                Arc::clone(&marker),
                Arc::clone(&realtime),
                Arc::clone(&uuids),
            ),
        )
        .unwrap();
        assert_eq!(first.path, path);
        let second = load_state_backed_boot_id(&path, probe(marker, realtime, uuids)).unwrap();
        assert_eq!(second.path, path);
        assert_eq!(first.boot_id, second.boot_id);
        assert_eq!(second.source, BootIdSource::StateBacked);
    }

    #[test]
    fn state_backed_changes_after_reboot_threshold() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bootid.state");
        let marker = Arc::new(AtomicU64::new(10_000_000));
        let realtime = Arc::new(AtomicU64::new(1_000_000_000));
        let uuids = Arc::new(Mutex::new(vec![Uuid::from_u128(2), Uuid::from_u128(1)]));
        let first = load_state_backed_boot_id(
            &path,
            probe(
                Arc::clone(&marker),
                Arc::clone(&realtime),
                Arc::clone(&uuids),
            ),
        )
        .unwrap();
        marker.store(10_000_000, Ordering::SeqCst);
        realtime.store(1_100_000_001, Ordering::SeqCst);
        let second = load_state_backed_boot_id(&path, probe(marker, realtime, uuids)).unwrap();
        assert_ne!(first.boot_id, second.boot_id);
    }

    #[test]
    fn corrupt_state_is_preserved_and_degraded() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bootid.state");
        fs::write(&path, "bad=true\n").unwrap();
        let marker = Arc::new(AtomicU64::new(10_000_000));
        let realtime = Arc::new(AtomicU64::new(1_000_000_000));
        let uuids = Arc::new(Mutex::new(vec![Uuid::from_u128(3)]));
        let outcome = load_state_backed_boot_id(&path, probe(marker, realtime, uuids)).unwrap();
        assert_eq!(outcome.path, path);
        assert_eq!(outcome.source, BootIdSource::Degraded);
        assert!(outcome.degraded_reason.is_some());
        assert!(path.with_extension("state.corrupt").exists());
        let clean = fs::read_to_string(path).unwrap();
        assert!(clean.contains("last_estimated_boottime="));
        assert!(clean.contains("last_boot_id="));
    }
}
