use std::error::Error;

pub type ParseError = Box<dyn Error + Send + Sync>;
pub type ParsedCursor = (String, String, u64, u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCursorLocation {
    pub seqnum_id: String,
    pub seqnum: u64,
    pub seqnum_set: bool,
    pub boot_id: String,
    pub monotonic: u64,
    pub monotonic_set: bool,
    pub realtime: u64,
    pub realtime_set: bool,
    pub xor_hash: u64,
    pub xor_hash_set: bool,
}

pub fn parse_match_string(s: &str) -> std::result::Result<Vec<u8>, ParseError> {
    parse_match_bytes(s.as_bytes())
}

pub fn parse_cursor(cursor: &str) -> std::result::Result<ParsedCursor, ParseError> {
    let location = parse_cursor_location(cursor, true)?;
    Ok((
        location.seqnum_id,
        location.boot_id,
        location.realtime,
        location.seqnum,
    ))
}

pub fn parse_cursor_location(
    cursor: &str,
    require_seek_component: bool,
) -> std::result::Result<ParsedCursorLocation, ParseError> {
    let mut seqnum_id = String::new();
    let mut boot_id = String::new();
    let mut realtime = None;
    let mut monotonic = None;
    let mut seqnum = None;
    let mut xor_hash = None;
    let mut legacy_cursor = false;

    for part in cursor.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            return Err("invalid cursor: malformed segment".into());
        };
        if key.is_empty() || value.is_empty() {
            return Err("invalid cursor: empty segment".into());
        }
        match key {
            "s" => seqnum_id = normalize_id(value),
            // Legacy SDK cursor shape.
            "j" => {
                legacy_cursor = true;
                boot_id = normalize_id(value);
            }
            "c" => {
                legacy_cursor = true;
                realtime = Some(u64::from_str_radix(value, 16)?);
            }
            "n" => {
                legacy_cursor = true;
                seqnum = Some(value.parse()?);
            }
            // Official systemd cursor shape.
            "b" => boot_id = normalize_id(value),
            "m" => monotonic = Some(u64::from_str_radix(value, 16)?),
            "t" => realtime = Some(u64::from_str_radix(value, 16)?),
            "i" => seqnum = Some(u64::from_str_radix(value, 16)?),
            "x" => xor_hash = Some(u64::from_str_radix(value, 16)?),
            _ => {}
        }
    }

    if legacy_cursor {
        if seqnum_id.is_empty() || boot_id.is_empty() || realtime.is_none() || seqnum.is_none() {
            return Err("invalid cursor: incomplete legacy cursor".into());
        }
    } else {
        let has_seqnum_cursor = !seqnum_id.is_empty() && seqnum.is_some();
        let has_monotonic_cursor = !boot_id.is_empty() && monotonic.is_some();
        let has_realtime_cursor = realtime.is_some();
        if require_seek_component
            && !(has_seqnum_cursor || has_monotonic_cursor || has_realtime_cursor)
        {
            return Err("invalid cursor: missing seek component".into());
        }
        if !require_seek_component
            && seqnum_id.is_empty()
            && seqnum.is_none()
            && boot_id.is_empty()
            && monotonic.is_none()
            && realtime.is_none()
            && xor_hash.is_none()
        {
            return Err("invalid cursor: missing cursor component".into());
        }
    }

    Ok(ParsedCursorLocation {
        seqnum_id,
        boot_id,
        realtime: realtime.unwrap_or(0),
        realtime_set: realtime.is_some(),
        monotonic: monotonic.unwrap_or(0),
        monotonic_set: monotonic.is_some(),
        seqnum: seqnum.unwrap_or(0),
        seqnum_set: seqnum.is_some(),
        xor_hash: xor_hash.unwrap_or(0),
        xor_hash_set: xor_hash.is_some(),
    })
}

fn normalize_id(value: &str) -> String {
    value.replace('-', "").to_ascii_lowercase()
}

pub fn cursor_location_matches(got: &ParsedCursorLocation, want: &ParsedCursorLocation) -> bool {
    let mut matched = false;
    if !want.seqnum_id.is_empty() {
        if got.seqnum_id != want.seqnum_id {
            return false;
        }
        matched = true;
    }
    if want.seqnum_set {
        if !got.seqnum_set || got.seqnum != want.seqnum {
            return false;
        }
        matched = true;
    }
    if !want.boot_id.is_empty() {
        if got.boot_id != want.boot_id {
            return false;
        }
        matched = true;
    }
    if want.monotonic_set {
        if !got.monotonic_set || got.monotonic != want.monotonic {
            return false;
        }
        matched = true;
    }
    if want.realtime_set {
        if !got.realtime_set || got.realtime != want.realtime {
            return false;
        }
        matched = true;
    }
    if want.xor_hash_set {
        if !got.xor_hash_set || got.xor_hash != want.xor_hash {
            return false;
        }
        matched = true;
    }
    matched
}

pub fn cursor_location_at_or_after(
    got: &ParsedCursorLocation,
    want: &ParsedCursorLocation,
) -> bool {
    if !want.seqnum_id.is_empty() && want.seqnum_set && got.seqnum_id == want.seqnum_id {
        if got.seqnum != want.seqnum {
            return got.seqnum > want.seqnum;
        }
    }
    if !want.boot_id.is_empty() && want.monotonic_set && got.boot_id == want.boot_id {
        if got.monotonic != want.monotonic {
            return got.monotonic > want.monotonic;
        }
    }
    if want.realtime_set && got.realtime != want.realtime {
        return got.realtime > want.realtime;
    }
    // systemd uses x= for exact cursor tests, but seek-cursor positioning
    // treats sequence/monotonic/realtime components as the ordering key.
    // A cursor with a mismatched x= still seeks to the row matching the
    // other components; TestCursor remains exact and checks x= above.
    true
}

pub fn parse_match_bytes(data: &[u8]) -> std::result::Result<Vec<u8>, ParseError> {
    let eq = data.iter().position(|byte| *byte == b'=');
    if eq.is_none() {
        return Err("EINVAL: missing '=' separator".into());
    }
    let eq = eq.unwrap();
    let key = &data[..eq];
    if key.is_empty() || key[0].is_ascii_digit() {
        return Err("EINVAL: invalid field name".into());
    }
    for byte in key {
        if !byte.is_ascii_uppercase() && !byte.is_ascii_digit() && *byte != b'_' {
            return Err("EINVAL: invalid field name".into());
        }
    }
    return Ok(data.to_vec());
}

#[cfg(test)]
mod tests {
    use super::{
        cursor_location_at_or_after, cursor_location_matches, parse_cursor, parse_cursor_location,
    };

    #[test]
    fn parse_cursor_accepts_legacy_sdk_shape() {
        let parsed = parse_cursor("s=abc123;j=def456;c=0000000000000001;n=42")
            .expect("legacy cursor parses");
        assert_eq!(parsed, ("abc123".to_string(), "def456".to_string(), 1, 42));
    }

    #[test]
    fn parse_cursor_accepts_official_systemd_shape() {
        let parsed =
            parse_cursor("s=ABC123;i=2a;b=01234567-89ab-cdef-0123-456789abcdef;m=9;t=10;x=ff")
                .expect("official cursor parses");
        assert_eq!(
            parsed,
            (
                "abc123".to_string(),
                "0123456789abcdef0123456789abcdef".to_string(),
                16,
                42,
            )
        );
    }

    #[test]
    fn parse_cursor_accepts_partial_systemd_shape() {
        let parsed = parse_cursor("s=ABC123;i=2a").expect("partial seqnum cursor parses");
        assert_eq!(parsed, ("abc123".to_string(), "".to_string(), 0, 42));

        let parsed = parse_cursor("t=10").expect("partial realtime cursor parses");
        assert_eq!(parsed, ("".to_string(), "".to_string(), 16, 0));
    }

    #[test]
    fn cursor_seek_order_ignores_x_hash_but_exact_match_checks_it() {
        let got = parse_cursor_location("s=abc;i=2;b=def;m=3;t=4;x=1", false)
            .expect("current cursor parses");
        let want = parse_cursor_location("s=abc;i=2;b=def;m=3;t=4;x=ffffffffffffffff", true)
            .expect("target cursor parses");

        assert!(
            cursor_location_at_or_after(&got, &want),
            "seek ordering should ignore mismatched x= when other components match"
        );
        assert!(
            !cursor_location_matches(&got, &want),
            "exact cursor matching must still include x="
        );
    }
}
