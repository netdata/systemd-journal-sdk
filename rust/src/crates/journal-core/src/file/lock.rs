use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const LOCK_VERSION: &str = "systemd-journal-sdk-lock-v1";
const STALE_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockOwner {
    pid: u32,
    boot_id: String,
    start_time: String,
}

#[derive(Debug)]
pub(crate) struct WriterLock {
    path: Option<PathBuf>,
}

impl WriterLock {
    pub(crate) fn acquire(journal_path: &str) -> io::Result<Self> {
        let lock_path = PathBuf::from(format!("{journal_path}.lock"));
        let owner = current_owner()?;

        loop {
            if let Some(parent) = lock_path.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent)?;
            }
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    write_owner(&mut file, &owner)?;
                    file.sync_all()?;
                    return Ok(Self {
                        path: Some(lock_path),
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    let (stale, holder) = lock_file_is_stale(&lock_path);
                    if !stale {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            format!("journal writer lock held by {holder}"),
                        ));
                    }
                    match fs::remove_file(&lock_path) {
                        Ok(()) => {}
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                        Err(err) => return Err(err),
                    }
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub(crate) fn release(&mut self) -> io::Result<()> {
        let Some(path) = self.path.take() else {
            return Ok(());
        };
        let current = current_owner()?;
        match read_owner(&path) {
            Ok(owner) if owner == current => match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err),
            },
            Ok(_) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn write_owner(file: &mut File, owner: &LockOwner) -> io::Result<()> {
    write!(
        file,
        "{LOCK_VERSION}\npid={}\nboot_id={}\nstart_time={}\n",
        owner.pid, owner.boot_id, owner.start_time
    )
}

fn lock_file_is_stale(path: &Path) -> (bool, String) {
    let owner = match read_owner(path) {
        Ok(owner) => owner,
        Err(_) => {
            if let Ok(metadata) = fs::metadata(path)
                && let Ok(modified) = metadata.modified()
                && SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default()
                    <= STALE_GRACE
            {
                return (false, "partially-created lock".to_string());
            }
            return (true, "malformed stale lock".to_string());
        }
    };

    if owner.boot_id != boot_id() {
        return (true, format!("pid {} from previous boot", owner.pid));
    }
    match owner_process_is_alive(&owner) {
        Ok(true) => (false, format!("pid {}", owner.pid)),
        Ok(false) => (true, format!("stale pid {}", owner.pid)),
        Err(_) => (false, format!("pid {} with unknown liveness", owner.pid)),
    }
}

fn current_owner() -> io::Result<LockOwner> {
    let pid = std::process::id();
    Ok(LockOwner {
        pid,
        boot_id: boot_id(),
        start_time: process_start_time(pid)?,
    })
}

fn boot_id() -> String {
    platform_boot_id()
}

#[cfg(target_os = "linux")]
fn platform_boot_id() -> String {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
fn platform_boot_id() -> String {
    journal_common::load_boot_id()
        .map(|boot_id| boot_id.as_simple().to_string())
        .unwrap_or_default()
}

fn process_start_time(pid: u32) -> io::Result<String> {
    platform_process_start_time(pid)
}

fn owner_process_is_alive(owner: &LockOwner) -> io::Result<bool> {
    platform_owner_process_is_alive(owner)
}

#[cfg(target_os = "linux")]
fn platform_process_start_time(pid: u32) -> io::Result<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let end = stat
        .rfind(')')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "cannot parse proc stat"))?;
    let fields: Vec<&str> = stat[end + 2..].split_whitespace().collect();
    if fields.len() < 20 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cannot parse process start time",
        ));
    }
    Ok(fields[19].to_string())
}

#[cfg(target_os = "linux")]
fn platform_owner_process_is_alive(owner: &LockOwner) -> io::Result<bool> {
    match platform_process_start_time(owner.pid) {
        Ok(start_time) => Ok(start_time == owner.start_time),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Ok(false),
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn platform_process_start_time(_pid: u32) -> io::Result<String> {
    Ok("process-start-unavailable".to_string())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn platform_owner_process_is_alive(owner: &LockOwner) -> io::Result<bool> {
    let rc = unsafe { libc::kill(owner.pid as libc::pid_t, 0) };
    if rc == 0 {
        return Ok(true);
    }

    let err = io::Error::last_os_error();
    match err.raw_os_error() {
        Some(code) if code == libc::ESRCH || code == libc::EINVAL => Ok(false),
        Some(code) if code == libc::EPERM => Ok(true),
        _ => Err(err),
    }
}

#[cfg(windows)]
fn platform_process_start_time(pid: u32) -> io::Result<String> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, FILETIME,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        let err = io::Error::last_os_error();
        return match err.raw_os_error() {
            Some(code) if code == ERROR_INVALID_PARAMETER as i32 => {
                Err(io::Error::new(io::ErrorKind::NotFound, err))
            }
            Some(code) if code == ERROR_ACCESS_DENIED as i32 => {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, err))
            }
            _ => Err(err),
        };
    }

    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut kernel = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut user = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };

    let ok = unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    let creation_ticks = ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
    Ok(creation_ticks.to_string())
}

#[cfg(windows)]
fn platform_owner_process_is_alive(owner: &LockOwner) -> io::Result<bool> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, WAIT_FAILED, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    match platform_process_start_time(owner.pid) {
        Ok(start_time) if start_time != owner.start_time => return Ok(false),
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => return Ok(true),
        Err(err) => return Err(err),
    }

    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, owner.pid) };
    if handle.is_null() {
        let err = io::Error::last_os_error();
        return match err.raw_os_error() {
            Some(code) if code == ERROR_INVALID_PARAMETER as i32 => Ok(false),
            Some(code) if code == ERROR_ACCESS_DENIED as i32 => Ok(true),
            _ => Err(err),
        };
    }

    let wait = unsafe { WaitForSingleObject(handle, 0) };
    unsafe {
        CloseHandle(handle);
    }
    match wait {
        WAIT_TIMEOUT => Ok(true),
        WAIT_OBJECT_0 => Ok(false),
        WAIT_FAILED => Err(io::Error::last_os_error()),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("unexpected WaitForSingleObject result {wait}"),
        )),
    }
}

fn read_owner(path: &Path) -> io::Result<LockOwner> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines();
    if lines.next() != Some(LOCK_VERSION) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid lock metadata",
        ));
    }
    let mut pid = None;
    let mut boot_id = None;
    let mut start_time = None;
    for line in lines {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "pid" => {
                pid =
                    Some(value.parse::<u32>().map_err(|err| {
                        io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                    })?)
            }
            "boot_id" => boot_id = Some(value.to_string()),
            "start_time" => start_time = Some(value.to_string()),
            _ => {}
        }
    }
    let Some(pid) = pid else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing lock pid",
        ));
    };
    let Some(start_time) = start_time.filter(|s| !s.is_empty()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing lock start time",
        ));
    };
    Ok(LockOwner {
        pid,
        boot_id: boot_id.unwrap_or_default(),
        start_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_dead_pid_lock_is_reclaimed() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let journal_path = dir.path().join("stale.journal");
        let lock_path = PathBuf::from(format!("{}.lock", journal_path.display()));
        let owner = LockOwner {
            pid: u32::MAX,
            boot_id: boot_id(),
            start_time: "not-a-real-process-start".to_string(),
        };
        let mut file = File::create(&lock_path).expect("create stale lock");
        write_owner(&mut file, &owner).expect("write stale lock");
        drop(file);

        let _lock = WriterLock::acquire(journal_path.to_str().expect("utf8 path"))
            .expect("stale lock should be reclaimed");
        let live_owner = read_owner(&lock_path).expect("read new lock");
        assert_eq!(live_owner.pid, std::process::id());
    }
}
