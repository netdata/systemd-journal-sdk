use anyhow::{Result, anyhow};

const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:-_.\\@";

pub(super) fn normalize(value: &[u8]) -> Result<(String, Option<String>)> {
    if value.is_empty() || value.contains(&0) {
        return Err(anyhow!(
            "Failed to mangle unit name '{}': Invalid argument",
            String::from_utf8_lossy(value)
        ));
    }
    let has_suffix = |name: &[u8]| {
        super::UNIT_SUFFIXES
            .iter()
            .any(|s| name.ends_with(s.as_bytes()))
    };
    let is_glob = |name: &[u8]| name.iter().any(|c| b"*?[".contains(c));
    let glob_char = |c: &u8| CHARS.contains(c) || b"[]!*?".contains(c);
    let valid_name = |name: &[u8]| {
        name.len() < 256
            && name.iter().rposition(|c| *c == b'.').is_some_and(|i| i > 0)
            && name.first() != Some(&b'@')
            && has_suffix(name)
            && name.iter().all(|c| CHARS.contains(c))
    };
    if valid_name(value) || is_glob(value) && value.iter().all(glob_char) {
        return Ok((String::from_utf8(value.to_vec())?, None));
    }
    if value[0] == b'/' {
        let mut components = Vec::new();
        for part in value.split(|c| *c == b'/') {
            if part.is_empty() || part == b"." || part == b".." && components.is_empty() {
                continue;
            }
            components.push(part);
        }
        let path = components.join(&b'/');
        if !components.contains(&b"..".as_slice())
            && components.iter().all(|p| p.len() <= 255)
            && path.len() < 4095
        {
            let suffix = if path.starts_with(b"dev/") || path.starts_with(b"sys/") {
                ".device"
            } else {
                ".mount"
            };
            let mut name = String::new();
            if path.is_empty() {
                name.push('-');
            }
            for (i, &c) in path.iter().enumerate() {
                if c == b'/' {
                    name.push('-');
                } else if c == b'-'
                    || c == b'\\'
                    || c == b'@'
                    || c == b'.' && i == 0
                    || !CHARS.contains(&c)
                {
                    escape(&mut name, c);
                } else {
                    name.push(c as char);
                }
            }
            name.push_str(suffix);
            if name.len() >= 256 {
                let key = [
                    0xec, 0xf2, 0x37, 0xfb, 0x58, 0x32, 0x4a, 0x32, 0x84, 0x9f, 0x06, 0x9b, 0x0d,
                    0x21, 0xeb, 0x9a,
                ];
                let hash = hex::encode(
                    journal_core::file::hash::siphash24_parts([name.as_bytes(), b"\0"], &key)
                        .to_le_bytes(),
                );
                name.truncate(256 - 1 - suffix.len() - 16 - 1);
                name.push('_');
                name.push_str(&hash);
                name.push_str(suffix);
            }
            return Ok((name, None));
        }
    }
    let mut name = String::new();
    for &c in value {
        if c == b'/' {
            name.push('-');
        } else if !glob_char(&c) {
            escape(&mut name, c);
        } else {
            name.push(c as char);
        }
    }
    let escaped = (name.as_bytes() != value).then(|| name.clone());
    if !is_glob(name.as_bytes()) && !valid_name(name.as_bytes()) {
        name.push_str(".service");
    }
    Ok((name, escaped))
}

fn escape(out: &mut String, c: u8) {
    const HEX: &[u8] = b"0123456789abcdef";
    out.push_str("\\x");
    out.push(HEX[(c >> 4) as usize] as char);
    out.push(HEX[(c & 15) as usize] as char);
}
