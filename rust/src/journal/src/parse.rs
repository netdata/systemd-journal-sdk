use std::error::Error;

pub type ParseError = Box<dyn Error + Send + Sync>;
pub type ParsedCursor = (String, String, u64, u64);

pub fn parse_match_string(s: &str) -> std::result::Result<Vec<u8>, ParseError> {
    parse_match_bytes(s.as_bytes())
}

pub fn parse_cursor(cursor: &str) -> std::result::Result<ParsedCursor, ParseError> {
    let mut seqnum_id = String::new();
    let mut boot_id = String::new();
    let mut realtime = None;
    let mut seqnum = None;

    for part in cursor.split(';') {
        let split = part.split_once('=');
        if split.is_none() {
            return Err("invalid cursor: malformed segment".into());
        }
        let (key, value) = split.unwrap();
        if key.is_empty() {
            return Err("invalid cursor: empty key".into());
        }
        match key {
            "s" => seqnum_id = value.to_string(),
            "j" => boot_id = value.to_string(),
            "c" => realtime = Some(u64::from_str_radix(value, 16)?),
            "n" => seqnum = Some(value.parse()?),
            _ => {}
        }
    }

    if seqnum_id.is_empty() || boot_id.is_empty() {
        return Err("invalid cursor: missing id".into());
    }

    Ok((
        seqnum_id,
        boot_id,
        realtime.ok_or("invalid cursor: missing realtime")?,
        seqnum.ok_or("invalid cursor: missing seqnum")?,
    ))
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
