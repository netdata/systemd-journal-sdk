//! System utilities for loading machine and boot identifiers.
//!
//! This module provides platform-specific functions to load system identifiers
//! that are used for journal file creation and identification.

use std::io;

/// Reads a file from the host filesystem, trying both the normal path and /host/ prefix.
///
/// This is useful when running in containers where the host filesystem may be mounted at /host.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn read_host_file(filename: &str) -> io::Result<String> {
    match std::fs::read_to_string(filename) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let filename = format!("/host/{}", filename);
            std::fs::read_to_string(filename)
        }
        Err(e) => Err(e),
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn parse_uuid_text(content: &str) -> io::Result<uuid::Uuid> {
    uuid::Uuid::try_parse(content.trim()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Loads the machine ID from the system.
///
/// On Linux, this reads from `/etc/machine-id`.
/// On macOS, this uses the native `gethostuuid(3)` API.
/// On other platforms, this returns an error.
#[cfg(target_os = "linux")]
pub fn load_machine_id() -> io::Result<uuid::Uuid> {
    let content = read_host_file("/etc/machine-id")?;
    parse_uuid_text(&content)
}

#[cfg(target_os = "macos")]
pub fn load_machine_id() -> io::Result<uuid::Uuid> {
    let mut bytes = [0u8; 16];
    let timeout = libc::timespec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::gethostuuid(bytes.as_mut_ptr(), &timeout) };
    if rc == 0 {
        return Ok(uuid::Uuid::from_bytes(bytes));
    }
    Err(io::Error::last_os_error())
}

#[cfg(target_os = "freebsd")]
pub fn load_machine_id() -> io::Result<uuid::Uuid> {
    let mut last_error = None;
    for path in [
        "/etc/machine-id",
        "/usr/local/etc/machine-id",
        "/var/db/dbus/machine-id",
        "/var/lib/dbus/machine-id",
    ] {
        match read_host_file(path).and_then(|content| parse_uuid_text(&content)) {
            Ok(machine_id) => return Ok(machine_id),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error
        .unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find machine ID")))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
pub fn load_machine_id() -> io::Result<uuid::Uuid> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Machine ID loading not supported on this platform",
    ))
}

/// Loads the boot ID from the system.
///
/// On Linux, this reads from `/proc/sys/kernel/random/boot_id`.
/// On macOS, this derives a deterministic ID from the boot time.
/// On other platforms, this returns an error.
#[cfg(target_os = "linux")]
pub fn load_boot_id() -> io::Result<uuid::Uuid> {
    let content = read_host_file("/proc/sys/kernel/random/boot_id")?;
    parse_uuid_text(&content)
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
fn load_boot_id_from_sysctl_boottime() -> io::Result<uuid::Uuid> {
    let name = b"kern.boottime\0";
    let mut boottime: libc::timeval = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::timeval>();
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr() as *const libc::c_char,
            &mut boottime as *mut _ as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    if len < std::mem::size_of::<libc::timeval>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "kern.boottime returned a truncated timeval",
        ));
    }
    let sec = u64::try_from(boottime.tv_sec).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "kern.boottime returned a negative seconds value",
        )
    })?;
    let usec = u32::try_from(boottime.tv_usec).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "kern.boottime returned an invalid microseconds value",
        )
    })?;

    if usec >= 1_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "kern.boottime returned out-of-range microseconds",
        ));
    }

    // Synthetic deterministic ID for same-boot comparison only.
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&sec.to_be_bytes());
    bytes[8..12].copy_from_slice(&usec.to_be_bytes());
    Ok(uuid::Uuid::from_bytes(bytes))
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
pub fn load_boot_id() -> io::Result<uuid::Uuid> {
    load_boot_id_from_sysctl_boottime()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
pub fn load_boot_id() -> io::Result<uuid::Uuid> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Boot ID loading not supported on this platform",
    ))
}
