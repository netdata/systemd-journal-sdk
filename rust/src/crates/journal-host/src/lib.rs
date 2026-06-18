//! Optional local-host identity helpers for journal writers.
//!
//! This crate is intentionally separate from the core file-format writer. It
//! probes the local host only when callers explicitly opt in and then pass the
//! returned values to the writer themselves.

#[cfg(any(target_os = "freebsd", windows, test))]
mod state;

#[cfg(target_os = "linux")]
#[path = "platform/linux.rs"]
mod platform;
#[cfg(target_os = "freebsd")]
#[path = "platform/freebsd.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "platform/macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "platform/windows.rs"]
mod platform;
#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos",
    windows
)))]
#[path = "platform/other.rs"]
mod platform;

use journal_common::EntryTimestamps;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Classifies how the helper obtained the boot ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootIdSource {
    /// No boot ID has been loaded yet.
    Unknown,
    /// A native per-boot kernel/OS UUID was read.
    Native,
    /// A state-backed boot ID was read from or written to healthy state.
    StateBacked,
    /// A valid fresh UUID was generated after native/state discovery failed.
    ///
    /// Degraded IDs satisfy the strict writer contract for this provider
    /// instance, but they do not claim cross-process same-boot stability.
    Degraded,
}

impl BootIdSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Native => "native",
            Self::StateBacked => "state-backed",
            Self::Degraded => "degraded",
        }
    }
}

/// Diagnostics for the values returned by [`LocalJournalProvider`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    pub machine_id_source: String,
    pub boot_id_source: BootIdSource,
    pub boot_id_path: Option<PathBuf>,
    pub monotonic_source: String,
    pub monotonic_source_detail: String,
    pub degraded_reason: Option<String>,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            machine_id_source: String::new(),
            boot_id_source: BootIdSource::Unknown,
            boot_id_path: None,
            monotonic_source: String::new(),
            monotonic_source_detail: String::new(),
            degraded_reason: None,
        }
    }
}

/// Configures local-host helper loading.
///
/// The zero value uses platform defaults. State paths are used only on Windows
/// and on FreeBSD when native `kern.boot_id` is unavailable.
#[derive(Clone, Default)]
pub struct LoadOptions {
    pub state_dir: Option<PathBuf>,
    pub state_file_name: Option<String>,
    pub state_path: Option<PathBuf>,
    pub monotonic_now: Option<Arc<dyn Fn() -> io::Result<u64> + Send + Sync>>,
    pub monotonic_label: Option<String>,
}

impl LoadOptions {
    pub fn with_state_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.state_dir = Some(path.into());
        self
    }

    pub fn with_state_file_name(mut self, name: impl Into<String>) -> Self {
        self.state_file_name = Some(name.into());
        self
    }

    pub fn with_state_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.state_path = Some(path.into());
        self
    }

    pub fn with_monotonic_now(
        mut self,
        label: impl Into<String>,
        source: Arc<dyn Fn() -> io::Result<u64> + Send + Sync>,
    ) -> Self {
        self.monotonic_label = Some(label.into());
        self.monotonic_now = Some(source);
        self
    }

    #[cfg(any(target_os = "freebsd", windows))]
    pub(crate) fn resolve_state_path(
        &self,
        default_dir: impl AsRef<std::path::Path>,
        default_file_name: impl AsRef<str>,
    ) -> PathBuf {
        if let Some(path) = &self.state_path {
            return path.clone();
        }
        let dir = self
            .state_dir
            .clone()
            .unwrap_or_else(|| default_dir.as_ref().to_path_buf());
        let file_name = self
            .state_file_name
            .clone()
            .unwrap_or_else(|| default_file_name.as_ref().to_string());
        dir.join(file_name)
    }
}

/// Local-host values that callers may pass explicitly to the writer.
pub struct LocalJournalProvider {
    machine_id: Uuid,
    boot_id: Uuid,
    diagnostics: Diagnostics,
    monotonic_now: Arc<dyn Fn() -> io::Result<u64> + Send + Sync>,
    monotonic_label: String,
    last_monotonic: Mutex<u64>,
}

impl LocalJournalProvider {
    pub fn load(options: LoadOptions) -> io::Result<Self> {
        platform::load(options)
    }

    pub fn machine_id(&self) -> Uuid {
        self.machine_id
    }

    pub fn boot_id(&self) -> Uuid {
        self.boot_id
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    pub fn monotonic_source(&self) -> &str {
        &self.monotonic_label
    }

    pub fn realtime_usec(&self) -> io::Result<u64> {
        realtime_usec()
    }

    /// Returns a boot-anchored monotonic microsecond timestamp.
    ///
    /// Values are clamped to strictly increase within this provider instance.
    pub fn monotonic_usec(&self) -> io::Result<u64> {
        let mut now = (self.monotonic_now)()?;
        let mut last = self
            .last_monotonic
            .lock()
            .map_err(|_| io::Error::other("monotonic mutex poisoned"))?;
        if now <= *last {
            now = last
                .checked_add(1)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "monotonic overflow"))?;
        }
        *last = now;
        Ok(now)
    }

    /// Builds the high-level Rust log-writer timestamp options.
    pub fn entry_timestamps(&self) -> io::Result<EntryTimestamps> {
        Ok(EntryTimestamps::default()
            .with_entry_realtime_usec(self.realtime_usec()?)
            .with_entry_monotonic_usec(self.monotonic_usec()?))
    }

    pub(crate) fn new(
        machine_id: Uuid,
        boot_id: Uuid,
        diagnostics: Diagnostics,
        monotonic_label: impl Into<String>,
        monotonic_now: Arc<dyn Fn() -> io::Result<u64> + Send + Sync>,
    ) -> Self {
        let monotonic_label = monotonic_label.into();
        let mut diagnostics = diagnostics;
        diagnostics.monotonic_source = monotonic_label.clone();
        if diagnostics.monotonic_source_detail.is_empty() {
            diagnostics.monotonic_source_detail = monotonic_label.clone();
        }
        Self {
            machine_id,
            boot_id,
            diagnostics,
            monotonic_now,
            monotonic_label,
            last_monotonic: Mutex::new(0),
        }
    }
}

/// Loads local-host identity and clock values using platform defaults.
pub fn load(options: LoadOptions) -> io::Result<LocalJournalProvider> {
    LocalJournalProvider::load(options)
}

/// Loads local-host identity and panics on failure.
pub fn must_load(options: LoadOptions) -> LocalJournalProvider {
    load(options).unwrap_or_else(|err| panic!("journal host helper failed: {err}"))
}

pub(crate) fn realtime_usec() -> io::Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    u64::try_from(duration.as_micros())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "realtime microseconds overflow"))
}

pub(crate) fn parse_uuid_text(text: &str) -> io::Result<Uuid> {
    Uuid::try_parse(text.trim())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
        .and_then(reject_zero_uuid)
}

pub(crate) fn reject_zero_uuid(id: Uuid) -> io::Result<Uuid> {
    if id.is_nil() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "uuid is all zeros",
        ));
    }
    Ok(id)
}

#[cfg(any(target_os = "freebsd", windows, test))]
pub(crate) fn uuid_compact(id: Uuid) -> String {
    id.as_simple().to_string()
}

#[cfg(unix)]
pub(crate) fn clock_gettime_usec(clock_id: libc::clockid_t) -> io::Result<u64> {
    let mut ts = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: `ts` points to valid storage for clock_gettime to initialize.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let rc = unsafe { libc::clock_gettime(clock_id, ts.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: clock_gettime succeeded and initialized the whole timespec.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let ts = unsafe { ts.assume_init() };
    let sec = u64::try_from(ts.tv_sec).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "clock_gettime returned negative seconds",
        )
    })?;
    let nsec = u64::try_from(ts.tv_nsec).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "clock_gettime returned negative nanoseconds",
        )
    })?;
    sec.checked_mul(1_000_000)
        .and_then(|value| value.checked_add(nsec / 1_000))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "clock microseconds overflow"))
}

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
pub(crate) fn sysctl_string(name: &str) -> io::Result<String> {
    let raw = sysctl_raw(name)?;
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    String::from_utf8(raw[..end].to_vec())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
pub(crate) fn sysctl_raw(name: &str) -> io::Result<Vec<u8>> {
    let name = std::ffi::CString::new(name)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let mut len = 0usize;
    // SAFETY: the name is NUL-terminated and len points to valid storage.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buf = vec![0u8; len];
    // SAFETY: the output buffer is valid for len bytes and sysctl writes into it.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(len);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn provider_monotonic_is_strictly_increasing() {
        let counter = Arc::new(AtomicU64::new(5));
        let source = {
            let counter = Arc::clone(&counter);
            Arc::new(move || Ok(counter.load(Ordering::SeqCst)))
        };
        let provider = LocalJournalProvider::new(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Diagnostics::default(),
            "test",
            source,
        );
        assert_eq!(provider.monotonic_usec().unwrap(), 5);
        assert_eq!(provider.monotonic_usec().unwrap(), 6);
        counter.store(10, Ordering::SeqCst);
        assert_eq!(provider.monotonic_usec().unwrap(), 10);
    }
}
