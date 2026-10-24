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
