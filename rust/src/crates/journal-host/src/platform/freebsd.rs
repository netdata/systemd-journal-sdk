use crate::state::{StateBackedProbe, load_state_backed_boot_id};
use crate::{
    BootIdSource, Diagnostics, LoadOptions, LocalJournalProvider, clock_gettime_usec,
    parse_uuid_text, realtime_usec, reject_zero_uuid, sysctl_raw, sysctl_string,
};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) fn load(options: LoadOptions) -> io::Result<LocalJournalProvider> {
    let machine_id = parse_uuid_text(&sysctl_string("kern.hostuuid")?)?;
    let mut diagnostics = Diagnostics {
        machine_id_source: "freebsd:kern.hostuuid".to_string(),
        monotonic_source_detail: "CLOCK_UPTIME".to_string(),
        ..Diagnostics::default()
    };

    let (boot_id, boot_source, boot_path, degraded_reason) = match load_native_boot_id() {
        Ok(id) => (id, BootIdSource::Native, None, None),
        Err(native_err) => {
            let path = options.resolve_state_path(
                PathBuf::from("/var/run/systemd-journal-sdk"),
                format!("bootid.{}.state", effective_uid()),
            );
            let outcome = load_state_backed_boot_id(
                &path,
                StateBackedProbe {
                    marker_now: Arc::new(|| clock_gettime_usec(libc::CLOCK_MONOTONIC)),
                    realtime_now: Arc::new(realtime_usec),
                    new_uuid: Arc::new(|| Ok(Uuid::new_v4())),
                },
            )?;
            let reason = outcome
                .degraded_reason
                .or_else(|| Some(format!("native kern.boot_id unavailable: {native_err}")))
                .filter(|_| outcome.source == BootIdSource::Degraded);
            (outcome.boot_id, outcome.source, Some(outcome.path), reason)
        }
    };

    diagnostics.boot_id_source = boot_source;
    diagnostics.boot_id_path = boot_path;
    diagnostics.degraded_reason = degraded_reason;
    let monotonic = options
        .monotonic_now
        .clone()
        .unwrap_or_else(|| Arc::new(|| clock_gettime_usec(libc::CLOCK_UPTIME)));
    let label = options
        .monotonic_label
        .clone()
        .unwrap_or_else(|| "CLOCK_UPTIME".to_string());

    Ok(LocalJournalProvider::new(
        machine_id,
        boot_id,
        diagnostics,
        label,
        monotonic,
    ))
}

fn load_native_boot_id() -> io::Result<Uuid> {
    let raw = sysctl_raw("kern.boot_id")?;
    if raw.len() < 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "kern.boot_id returned fewer than 16 bytes",
        ));
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&raw[..16]);
    reject_zero_uuid(Uuid::from_bytes(bytes))
}

fn effective_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and returns the current effective UID.
    unsafe { libc::geteuid() }
}
