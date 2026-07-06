use crate::{Diagnostics, LoadOptions, LocalJournalProvider, clock_gettime_usec, parse_uuid_text};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub(crate) fn load(options: LoadOptions) -> io::Result<LocalJournalProvider> {
    let (machine_id, machine_id_source) =
        load_machine_id(options.host_filesystem_prefix.as_deref())?;
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

fn load_machine_id(host_filesystem_prefix: Option<&Path>) -> io::Result<(Uuid, String)> {
    load_machine_id_from_root(Path::new("/"), host_filesystem_prefix)
}

fn load_machine_id_from_root(
    root: &Path,
    host_filesystem_prefix: Option<&Path>,
) -> io::Result<(Uuid, String)> {
    let rel_paths = [
        Path::new("etc/machine-id"),
        Path::new("var/lib/dbus/machine-id"),
    ];
    if let Some(prefix) = host_filesystem_prefix.filter(|path| !path.as_os_str().is_empty()) {
        let prefix_root = rooted_path(root, prefix);
        for rel in rel_paths {
            let source = format!("linux:{}", display_prefixed_path(prefix, rel));
            match read_machine_id_candidate(&prefix_root.join(rel), &source) {
                Ok(id) => return Ok(id),
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(err) => return Err(err),
            }
        }
    }

    for rel in rel_paths {
        let source = format!("linux:/{}", rel.display());
        match read_machine_id_candidate(&root.join(rel), &source) {
            Ok(id) => return Ok(id),
            Err(_) => continue,
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "linux machine-id not found",
    ))
}

fn read_machine_id_candidate(path: &Path, source: &str) -> io::Result<(Uuid, String)> {
    fs::read_to_string(path)
        .and_then(|text| parse_uuid_text(&text))
        .map(|id| (id, source.to_string()))
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                err
            } else {
                io::Error::new(err.kind(), format!("failed to read {source}: {err}"))
            }
        })
}

fn rooted_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        match path.strip_prefix("/") {
            Ok(rel) => root.join(rel),
            Err(_) => path.to_path_buf(),
        }
    } else {
        root.join(path)
    }
}

fn display_prefixed_path(prefix: &Path, rel: &Path) -> String {
    let mut prefix = prefix.display().to_string();
    while prefix.ends_with('/') && prefix.len() > 1 {
        prefix.pop();
    }
    if prefix == "/" {
        format!("/{}", rel.display())
    } else {
        format!("{}/{}", prefix, rel.display())
    }
}

fn load_boot_id() -> io::Result<Uuid> {
    let text = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    parse_uuid_text(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const CONTAINER_ID: &str = "00112233445566778899aabbccddeeff";
    const HOST_ID: &str = "ffeeddccbbaa99887766554433221100";
    const DBUS_ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn machine_id_uses_container_paths_by_default() {
        let dir = TempDir::new().unwrap();
        write_machine_id(dir.path(), "etc/machine-id", CONTAINER_ID);
        write_machine_id(dir.path(), "host/etc/machine-id", HOST_ID);

        let (id, source) = load_machine_id_from_root(dir.path(), None).unwrap();

        assert_eq!(id, Uuid::parse_str(CONTAINER_ID).unwrap());
        assert_eq!(source, "linux:/etc/machine-id");
    }

    #[test]
    fn machine_id_prefers_explicit_host_prefix() {
        let dir = TempDir::new().unwrap();
        write_machine_id(dir.path(), "etc/machine-id", CONTAINER_ID);
        write_machine_id(dir.path(), "host/etc/machine-id", HOST_ID);

        let (id, source) = load_machine_id_from_root(dir.path(), Some(Path::new("/host"))).unwrap();

        assert_eq!(id, Uuid::parse_str(HOST_ID).unwrap());
        assert_eq!(source, "linux:/host/etc/machine-id");
    }

    #[test]
    fn machine_id_falls_back_when_host_prefix_absent() {
        let dir = TempDir::new().unwrap();
        write_machine_id(dir.path(), "var/lib/dbus/machine-id", DBUS_ID);

        let (id, source) = load_machine_id_from_root(dir.path(), Some(Path::new("/host"))).unwrap();

        assert_eq!(id, Uuid::parse_str(DBUS_ID).unwrap());
        assert_eq!(source, "linux:/var/lib/dbus/machine-id");
    }

    #[test]
    fn machine_id_checks_host_dbus_path_before_container_paths() {
        let dir = TempDir::new().unwrap();
        write_machine_id(dir.path(), "etc/machine-id", CONTAINER_ID);
        write_machine_id(dir.path(), "host/var/lib/dbus/machine-id", DBUS_ID);

        let (id, source) = load_machine_id_from_root(dir.path(), Some(Path::new("/host"))).unwrap();

        assert_eq!(id, Uuid::parse_str(DBUS_ID).unwrap());
        assert_eq!(source, "linux:/host/var/lib/dbus/machine-id");
    }

    #[test]
    fn machine_id_errors_on_invalid_explicit_host_prefix_file() {
        let dir = TempDir::new().unwrap();
        write_machine_id(dir.path(), "etc/machine-id", CONTAINER_ID);
        write_text(dir.path(), "host/etc/machine-id", "not-a-machine-id\n");

        let err = load_machine_id_from_root(dir.path(), Some(Path::new("/host"))).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("linux:/host/etc/machine-id"));
    }

    #[test]
    fn machine_id_errors_on_invalid_first_host_file_even_when_host_dbus_exists() {
        let dir = TempDir::new().unwrap();
        write_text(dir.path(), "host/etc/machine-id", "not-a-machine-id\n");
        write_machine_id(dir.path(), "host/var/lib/dbus/machine-id", DBUS_ID);

        let err = load_machine_id_from_root(dir.path(), Some(Path::new("/host"))).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("linux:/host/etc/machine-id"));
    }

    #[test]
    fn machine_id_empty_host_prefix_keeps_container_default() {
        let dir = TempDir::new().unwrap();
        write_machine_id(dir.path(), "etc/machine-id", CONTAINER_ID);
        write_machine_id(dir.path(), "host/etc/machine-id", HOST_ID);

        let (id, source) = load_machine_id_from_root(dir.path(), Some(Path::new(""))).unwrap();

        assert_eq!(id, Uuid::parse_str(CONTAINER_ID).unwrap());
        assert_eq!(source, "linux:/etc/machine-id");
    }

    fn write_machine_id(root: &Path, rel: &str, value: &str) {
        write_text(root, rel, &format!("{value}\n"));
    }

    fn write_text(root: &Path, rel: &str, value: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }
}
