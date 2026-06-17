use crate::{
    BootIdSource, Diagnostics, LoadOptions, LocalJournalProvider, clock_gettime_usec,
    parse_uuid_text, reject_zero_uuid, sysctl_string,
};
use std::io;
use std::sync::Arc;
use uuid::Uuid;

const CLOCK_UPTIME_RAW_MACOS: libc::clockid_t = 8;

pub(crate) fn load(options: LoadOptions) -> io::Result<LocalJournalProvider> {
    let machine_id = load_machine_id()?;
    let (boot_id, boot_source, degraded_reason) =
        match sysctl_string("kern.bootsessionuuid").and_then(|text| parse_uuid_text(&text)) {
            Ok(id) => (id, BootIdSource::Native, None),
            Err(err) => (
                Uuid::new_v4(),
                BootIdSource::Degraded,
                Some(format!("kern.bootsessionuuid unavailable: {err}")),
            ),
        };
    let diagnostics = Diagnostics {
        machine_id_source: "macos:gethostuuid".to_string(),
        boot_id_source: boot_source,
        monotonic_source_detail: "CLOCK_UPTIME_RAW".to_string(),
        degraded_reason,
        ..Diagnostics::default()
    };
    let monotonic = options
        .monotonic_now
        .clone()
        .unwrap_or_else(|| Arc::new(|| clock_gettime_usec(CLOCK_UPTIME_RAW_MACOS)));
    let label = options
        .monotonic_label
        .clone()
        .unwrap_or_else(|| "CLOCK_UPTIME_RAW".to_string());
    Ok(LocalJournalProvider::new(
        machine_id,
        boot_id,
        diagnostics,
        label,
        monotonic,
    ))
}

fn load_machine_id() -> io::Result<Uuid> {
    let mut bytes = [0u8; 16];
    let timeout = libc::timespec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    // SAFETY: bytes is a valid 16-byte output buffer and timeout is valid for
    // the duration of the call.
    let rc = unsafe { libc::gethostuuid(bytes.as_mut_ptr(), &timeout) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    reject_zero_uuid(Uuid::from_bytes(bytes))
}
