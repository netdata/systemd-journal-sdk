use std::fs::Metadata;
use std::path::Path;

fn earlier(current: u64, candidate: u64) -> u64 {
    if candidate > 0 && candidate < current {
        candidate
    } else {
        current
    }
}

pub(super) fn oldest_realtime(metadata: &Metadata, mut realtime: u64) -> u64 {
    if let Some(stamp) = metadata
        .accessed()
        .ok()
        .and_then(super::system_time_to_usec)
    {
        realtime = earlier(realtime, stamp);
    }
    if let Some(stamp) = metadata.created().ok().and_then(super::system_time_to_usec) {
        if !cfg!(target_os = "linux") || stamp >= 1_000_000 {
            realtime = earlier(realtime, stamp);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(seconds), Ok(nanos)) = (
            u64::try_from(metadata.ctime()),
            u64::try_from(metadata.ctime_nsec()),
        ) {
            if let Some(stamp) = seconds
                .checked_mul(1_000_000)
                .and_then(|value| value.checked_add(nanos / 1_000))
            {
                realtime = earlier(realtime, stamp);
            }
        }
    }
    realtime
}

pub(super) fn creation_realtime(path: &Path, realtime: u64) -> u64 {
    legacy_creation_time(path).map_or(realtime, |stamp| earlier(realtime, stamp))
}

#[cfg(target_os = "linux")]
fn legacy_creation_time(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut data = [0u8; 8];
    let size = unsafe {
        libc::getxattr(
            path.as_ptr(),
            c"user.crtime_usec".as_ptr(),
            data.as_mut_ptr().cast(),
            data.len(),
        )
    };
    (size == data.len() as libc::ssize_t).then(|| u64::from_le_bytes(data))
}

#[cfg(not(target_os = "linux"))]
fn legacy_creation_time(_path: &Path) -> Option<u64> {
    None
}
