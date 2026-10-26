use super::colors::Colors;

pub(super) fn enabled(colors: Colors) -> bool {
    std::env::var("SYSTEMD_URLIFY")
        .ok()
        .and_then(|value| super::parse_bool_option("SYSTEMD_URLIFY", &value).ok())
        .unwrap_or(colors.enabled())
}

fn terminal_link(url: &[u8], text: &[u8]) -> Vec<u8> {
    if url.len() > 2000 || url.iter().any(|b| !(32..127).contains(b)) {
        return text.to_vec();
    }
    let mut out = b"\x1b]8;;".to_vec();
    out.extend_from_slice(url);
    out.extend_from_slice(b"\x1b\\");
    out.extend_from_slice(text);
    out.extend_from_slice(b"\x1b]8;;\x1b\\");
    out
}

pub(super) fn file(value: &[u8], enabled: bool) -> Option<Vec<u8>> {
    let text = value.split(|&b| b == 0).next().unwrap_or_default();
    if text.is_empty() {
        return None;
    }
    if !enabled {
        return Some(text.to_vec());
    }
    let Ok(path) = std::str::from_utf8(text) else {
        return Some(text.to_vec());
    };
    static HOSTNAME: std::sync::OnceLock<Option<Vec<u8>>> = std::sync::OnceLock::new();
    let hostname = HOSTNAME.get_or_init(super::terminal::hostname).as_deref()?;
    #[cfg(windows)]
    {
        let Some(path) = std::path::absolute(path)
            .ok()
            .and_then(|p| p.to_str().map(str::to_owned))
        else {
            return Some(text.to_vec());
        };
        return Some(match windows_file_url(&path, hostname) {
            Some(url) => terminal_link(&url, text),
            None => text.to_vec(),
        });
    }
    #[cfg(not(windows))]
    {
        let mut url = b"file://".to_vec();
        url.extend_from_slice(hostname);
        if !std::path::Path::new(path).is_absolute() {
            let cwd = working_directory()?;
            let bytes = cwd.as_os_str().as_encoded_bytes();
            url.extend_from_slice(bytes);
            if !bytes.ends_with(&[std::path::MAIN_SEPARATOR as u8]) {
                url.push(std::path::MAIN_SEPARATOR as u8);
            }
        }
        url.extend_from_slice(text);
        Some(terminal_link(&url, text))
    }
}

#[cfg(not(windows))]
fn working_directory() -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    if let Some(pwd) = std::env::var_os("PWD")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(logical), Ok(current)) = (std::fs::metadata(&pwd), std::fs::metadata(".")) {
            if logical.dev() == current.dev() && logical.ino() == current.ino() {
                return Some(pwd);
            }
        }
    }
    std::env::current_dir().ok()
}

pub(super) fn documentation(value: &[u8], mut unicode: bool) -> Vec<u8> {
    let url = value.split(|&b| b == 0).next().unwrap_or_default();
    if !url.is_ascii()
        || ![
            b"http://".as_slice(),
            b"https://",
            b"file:/",
            b"info:",
            b"man:",
        ]
        .iter()
        .any(|prefix| url.starts_with(prefix) && url.len() > prefix.len())
    {
        return Vec::new();
    }
    if let Ok(value) = std::env::var("SYSTEMD_UTF8") {
        if let Ok(value) = super::parse_bool_option("SYSTEMD_UTF8", &value) {
            unicode = value;
        }
    }
    let glyph = if unicode { "[🡕]" } else { "[LNK]" };
    let mut out = terminal_link(url, glyph.as_bytes());
    out.push(b' ');
    out
}

#[cfg(any(windows, test))]
fn windows_file_url(path: &str, hostname: &[u8]) -> Option<Vec<u8>> {
    let path = path.replace('\\', "/");
    if path.starts_with("//?/") || path.starts_with("//./") {
        return None;
    }
    let (authority, path) = if let Some(rest) = path.strip_prefix("//") {
        let (server, share) = rest.split_once('/')?;
        if server.is_empty() || share.is_empty() {
            return None;
        }
        (server.as_bytes(), format!("/{share}"))
    } else {
        let bytes = path.as_bytes();
        if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1..3] != *b":/" {
            return None;
        }
        (hostname, format!("/{path}"))
    };
    if authority
        .iter()
        .any(|b| !b.is_ascii_alphanumeric() && !b".-".contains(b))
    {
        return None;
    }
    let mut url = b"file://".to_vec();
    url.extend_from_slice(authority);
    const HEX: &[u8] = b"0123456789ABCDEF";
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            url.push(b);
        } else {
            url.extend_from_slice(&[b'%', HEX[(b >> 4) as usize], HEX[(b & 15) as usize]]);
        }
    }
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_file_uri_paths() {
        for (path, want) in [
            (r"C:\logs\example.conf", "file://host/C:/logs/example.conf"),
            (
                r"\\server\share\é #%.conf",
                "file://server/share/%C3%A9%20%23%25.conf",
            ),
            ("C:/logs/a?b", "file://host/C:/logs/a%3Fb"),
        ] {
            assert_eq!(windows_file_url(path, b"host").unwrap(), want.as_bytes());
        }
        for path in [r"\\?\C:\logs", r"\\.\pipe\name", "relative", "//server"] {
            assert!(windows_file_url(path, b"host").is_none());
        }
    }

    #[cfg(windows)]
    #[test]
    fn relative_windows_file_link() {
        let path = "relative #.conf";
        let absolute = std::path::absolute(path).unwrap();
        let hostname = super::super::terminal::hostname().unwrap();
        let url = windows_file_url(absolute.to_str().unwrap(), &hostname).unwrap();
        assert_eq!(
            file(path.as_bytes(), true).unwrap(),
            terminal_link(&url, path.as_bytes())
        );
        assert_eq!(file(path.as_bytes(), false).unwrap(), path.as_bytes());
    }
}
