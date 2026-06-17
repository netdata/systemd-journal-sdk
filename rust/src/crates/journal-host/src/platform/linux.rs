use crate::{Diagnostics, LoadOptions, LocalJournalProvider, clock_gettime_usec, parse_uuid_text};
use std::fs;
use std::io;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) fn load(options: LoadOptions) -> io::Result<LocalJournalProvider> {
    let (machine_id, machine_id_source) = load_machine_id()?;
    let (boot_id, boot_id_source, degraded_reason) = match load_boot_id() {
        Ok(id) => (id, crate::BootIdSource::Native, None),
        Err(err) => (
            Uuid::new_v4(),
            crate::BootIdSource::Degraded,
            Some(format!("linux boot_id unavailable: {err}")),
        ),
    };
    let mut diagnostics = Diagnostics {
        machine_id_source,
        boot_id_source,
        monotonic_source_detail: "CLOCK_MONOTONIC".to_string(),
        degraded_reason,
        ..Diagnostics::default()
    };
    let monotonic = options
        .monotonic_now
        .clone()
        .unwrap_or_else(|| Arc::new(|| clock_gettime_usec(libc::CLOCK_MONOTONIC)));
    let label = options
        .monotonic_label
        .clone()
        .unwrap_or_else(|| "CLOCK_MONOTONIC".to_string());
    diagnostics.monotonic_source_detail = label.clone();
    Ok(LocalJournalProvider::new(
        machine_id,
        boot_id,
        diagnostics,
        label,
        monotonic,
    ))
}

fn load_machine_id() -> io::Result<(Uuid, String)> {
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        match fs::read_to_string(path).and_then(|text| parse_uuid_text(&text)) {
            Ok(id) => return Ok((id, format!("linux:{path}"))),
            Err(_) => continue,
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "linux machine-id not found",
    ))
}

fn load_boot_id() -> io::Result<Uuid> {
    let text = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    parse_uuid_text(&text)
}
