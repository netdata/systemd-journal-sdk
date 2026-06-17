use crate::state::{StateBackedProbe, load_state_backed_boot_id};
use crate::{Diagnostics, LoadOptions, LocalJournalProvider, parse_uuid_text, realtime_usec};
use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime;

pub(crate) fn load(options: LoadOptions) -> io::Result<LocalJournalProvider> {
    let machine_id = load_machine_id()?;
    let path = options.resolve_state_path(default_state_dir(), "bootid.state");
    let outcome = load_state_backed_boot_id(
        &path,
        StateBackedProbe {
            marker_now: Arc::new(|| Ok(get_tick_count64_usec())),
            realtime_now: Arc::new(realtime_usec),
            new_uuid: Arc::new(|| Ok(Uuid::new_v4())),
        },
    )?;
    let diagnostics = Diagnostics {
        machine_id_source: "windows:MachineGuid".to_string(),
        boot_id_source: outcome.source,
        boot_id_path: Some(outcome.path),
        monotonic_source_detail: "QueryUnbiasedInterruptTime".to_string(),
        degraded_reason: outcome.degraded_reason,
        ..Diagnostics::default()
    };
    let monotonic = options
        .monotonic_now
        .clone()
        .unwrap_or_else(|| Arc::new(query_unbiased_interrupt_time_usec));
    let label = options
        .monotonic_label
        .clone()
        .unwrap_or_else(|| "QueryUnbiasedInterruptTime".to_string());
    Ok(LocalJournalProvider::new(
        machine_id,
        outcome.boot_id,
        diagnostics,
        label,
        monotonic,
    ))
}

fn load_machine_id() -> io::Result<Uuid> {
    let subkey = wide("SOFTWARE\\Microsoft\\Cryptography");
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: subkey is NUL-terminated and key points to valid storage.
    let status =
        unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey.as_ptr(), 0, KEY_READ, &mut key) };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let _guard = RegKeyGuard(key);
    let name = wide("MachineGuid");
    let mut value_type = 0u32;
    let mut buffer = [0u16; 128];
    let mut byte_len = (buffer.len() * std::mem::size_of::<u16>()) as u32;
    // SAFETY: key is open, name is NUL-terminated, and the output buffer is valid.
    let status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            &mut value_type,
            buffer.as_mut_ptr().cast(),
            &mut byte_len,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    if value_type != REG_SZ {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MachineGuid is not REG_SZ",
        ));
    }
    let len = usize::try_from(byte_len)
        .ok()
        .and_then(|bytes| bytes.checked_div(std::mem::size_of::<u16>()))
        .unwrap_or(0);
    let text = String::from_utf16_lossy(&buffer[..len])
        .trim_end_matches('\0')
        .trim()
        .to_string();
    parse_uuid_text(&text)
}

fn query_unbiased_interrupt_time_usec() -> io::Result<u64> {
    let mut ticks_100ns = 0u64;
    // SAFETY: the pointer is valid for the duration of the call.
    let ok = unsafe { QueryUnbiasedInterruptTime(&mut ticks_100ns) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(ticks_100ns / 10)
}

fn get_tick_count64_usec() -> u64 {
    // SAFETY: GetTickCount64 has no preconditions.
    unsafe { GetTickCount64() }.saturating_mul(1_000)
}

fn default_state_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("systemd-journal-sdk")
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

struct RegKeyGuard(HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        // SAFETY: this guard owns the registry handle returned by RegOpenKeyExW.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
