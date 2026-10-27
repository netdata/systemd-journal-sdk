use anyhow::{Result, anyhow};
use chrono::{Datelike, NaiveDateTime, Timelike};
use jiff::tz::TimeZone;
use std::{
    env,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

struct Kind {
    offset: i64,
    dst: usize,
    name: String,
    standard: bool,
    universal: bool,
}
struct Transition {
    at: i64,
    kind: usize,
}
struct Leap {
    at: i64,
    correction: i64,
}
struct Tzif {
    kinds: Vec<Kind>,
    transitions: Vec<Transition>,
    leaps: Vec<Leap>,
    first_name: String,
    footer: [String; 2],
    footer_rule: String,
}
pub struct Zone {
    provider: TimeZone,
    data: Option<Tzif>,
    names: [String; 2],
    initial_names: [String; 2],
    file: Option<(PathBuf, std::fs::Metadata)>,
}

fn zone_dirs() -> Vec<PathBuf> {
    if let Some(dir) = env::var_os("TZDIR").filter(|d| !d.is_empty()) {
        return vec![dir.into()];
    }
    [
        "/usr/share/zoneinfo",
        "/usr/share/lib/zoneinfo",
        "/usr/lib/locale/TZ",
    ]
    .map(PathBuf::from)
    .to_vec()
}

impl Zone {
    fn read(path: &Path) -> Option<Self> {
        let mut bytes = Vec::new();
        let file = File::open(path).ok()?;
        let info = file.metadata().ok()?;
        if !info.is_file() {
            return None;
        }
        file.take((10 << 20) + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 10 << 20 {
            return None;
        }
        let provider = TimeZone::tzif("local", &bytes).ok()?;
        let data = Tzif::parse(&bytes)?;
        let mut names = [String::new(), String::new()];
        for t in data.transitions.iter().rev() {
            let k = &data.kinds[t.kind];
            if names[k.dst].is_empty() {
                names[k.dst] = k.name.clone();
            }
        }
        if names[0].is_empty() {
            names[0] = data.first_name.clone();
        }
        if names[1].is_empty() {
            names[1] = names[0].clone();
        }
        Some(Self {
            provider,
            data: Some(data),
            initial_names: names.clone(),
            names,
            file: Some((path.canonicalize().ok()?, info)),
        })
    }

    pub fn local() -> Self {
        if let Some(value) = env::var_os("TZ") {
            let value = value.to_string_lossy();
            if value.is_empty() {
                return Self::fallback("Universal".to_owned(), 0);
            }
            let value = value.strip_prefix(':').unwrap_or(&value);
            if Path::new(value).is_absolute() {
                if let Some(z) = Self::read(Path::new(value)) {
                    return z;
                }
            }
            if !value.is_empty() {
                for dir in zone_dirs() {
                    if let Some(z) = Self::read(&dir.join(value)) {
                        return z;
                    }
                }
            }
            if !value.is_empty() {
                if let Some(zone) = Self::default_rules(value) {
                    return zone;
                }
                if let Ok(provider) = TimeZone::posix(&normalize_rule(value)) {
                    return Self {
                        provider,
                        data: None,
                        names: Default::default(),
                        initial_names: Default::default(),
                        file: None,
                    };
                }
                if zone_dirs().iter().any(|p| p.is_dir()) || TimeZone::get(value).is_err() {
                    let (name, offset) = fallback_rule(value);
                    return Self::fallback(name, offset);
                }
            }
        } else if let Some(z) = Self::read(Path::new("/etc/localtime")) {
            return z;
        }
        Self {
            provider: TimeZone::system(),
            data: None,
            names: Default::default(),
            initial_names: Default::default(),
            file: None,
        }
    }

    // Default-file transitions and names are prepared once, never per row.
    fn default_rules(rule: &str) -> Option<Self> {
        let normalized = normalize_rule(rule);
        let rule = normalized.strip_suffix(',').unwrap_or(&normalized);
        if rule.contains(',') {
            return None;
        }
        let (standard, _) = fallback_rule(rule);
        if standard.is_empty() {
            return None;
        }
        let end = standard.len() + if rule.starts_with('<') { 2 } else { 0 };
        let rest = rule[end..]
            .trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | ':'));
        if fallback_rule(rest).0.is_empty() {
            return None;
        }
        let provider = TimeZone::posix(&format!("{rule},M3.2.0,M11.1.0")).ok()?;
        let names = footer_names(rule);
        let mut offsets = [0; 2];
        for (i, seconds) in [1705276800, 1721001600].into_iter().enumerate() {
            offsets[i] = i64::from(
                provider
                    .to_offset_info(jiff::Timestamp::from_second(seconds).ok()?)
                    .offset()
                    .seconds(),
            );
        }
        for dir in zone_dirs() {
            let Some(mut z) = Self::read(&dir.join("posixrules")) else {
                continue;
            };
            let meta = z.data.as_mut()?;
            if meta.kinds.len() < 2 {
                continue;
            }
            // Native default-file conversion starts with zero remembered DST offset.
            let mut old_offsets = [meta.kinds[0].offset, 0];
            for transition in meta.transitions.iter().rev() {
                let kind = &meta.kinds[transition.kind];
                if kind.dst == 0 {
                    old_offsets[0] = kind.offset;
                    break;
                }
            }
            let mut previous_dst = 0;
            for transition in &mut meta.transitions {
                let kind = &meta.kinds[transition.kind];
                let shift_kind = usize::from(previous_dst != 0 && !kind.standard);
                if !kind.universal {
                    transition.at = transition
                        .at
                        .checked_add(offsets[shift_kind] - old_offsets[shift_kind])?;
                }
                transition.kind = kind.dst;
                previous_dst = kind.dst;
            }
            meta.kinds = (0..2)
                .map(|dst| Kind {
                    offset: offsets[dst],
                    dst,
                    name: names[dst].clone(),
                    standard: false,
                    universal: false,
                })
                .collect();
            meta.first_name = names[0].clone();
            if !meta.footer[0].is_empty() {
                meta.footer = names.clone();
                if let Ok(footer) = TimeZone::posix(&meta.footer_rule) {
                    z.provider = footer;
                }
            }
            z.names = names.clone();
            z.initial_names = names.clone();
            z.file = None;
            return Some(z);
        }
        Some(Self {
            provider,
            data: None,
            names: names.clone(),
            initial_names: names,
            file: None,
        })
    }

    fn fallback(name: String, offset: i64) -> Self {
        Self {
            provider: TimeZone::UTC,
            data: Some(Tzif {
                kinds: vec![Kind {
                    offset,
                    dst: 0,
                    name: name.clone(),
                    standard: false,
                    universal: false,
                }],
                transitions: Vec::new(),
                leaps: Vec::new(),
                first_name: name.clone(),
                footer: Default::default(),
                footer_rule: String::new(),
            }),
            names: [name.clone(), name.clone()],
            initial_names: [name.clone(), name],
            file: None,
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        if name.is_empty()
            || name.len() >= 4096
            || name.starts_with('/')
            || name.ends_with('/')
            || name.contains("//")
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_+/".contains(&b))
        {
            return None;
        }
        let mut installed = false;
        for dir in zone_dirs() {
            installed |= dir.is_dir();
            if let Some(z) = Self::read(&dir.join(name)) {
                return Some(z);
            }
        }
        if installed {
            return None;
        }
        let provider = TimeZone::get(name).ok()?;
        if provider.iana_name() != Some(name) {
            return None;
        }
        Some(Self {
            provider,
            data: None,
            names: Default::default(),
            initial_names: Default::default(),
            file: None,
        })
    }

    pub fn restore_after_named(&mut self, named: &Self) {
        if self.data.is_none() {
            return;
        }
        let same = match (&self.file, &named.file) {
            (Some((a, am)), Some((b, bm))) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    let _ = (a, b);
                    am.dev() == bm.dev() && am.ino() == bm.ino() && am.mtime() == bm.mtime()
                }
                #[cfg(not(unix))]
                {
                    a == b && am.modified().ok() == bm.modified().ok()
                }
            }
            _ => false,
        };
        self.names = if same {
            named.names.clone()
        } else {
            self.initial_names.clone()
        };
    }

    pub fn abbreviation(&self, name: &str) -> Option<i32> {
        if self.data.is_some() {
            return self.names.iter().position(|n| n == name).map(|n| n as i32);
        }
        let year = chrono::Utc::now().year();
        for month in [1, 7] {
            let seconds = chrono::NaiveDate::from_ymd_opt(year, month, 1)?
                .and_hms_opt(12, 0, 0)?
                .and_utc()
                .timestamp();
            let info = self
                .provider
                .to_offset_info(jiff::Timestamp::from_second(seconds).ok()?);
            if info.abbreviation() == name {
                return Some(i32::from(info.dst().is_dst()));
            }
        }
        None
    }

    // Native calendar conversion retains TZif leaps and ignores a footer before
    // the first transition; neither behavior is supplied by the zone provider.
    pub fn civil(&mut self, seconds: i64, utc: bool) -> Result<(NaiveDateTime, i32, i64)> {
        let table_kind = self.data.as_ref().and_then(|data| {
            let i = data.transitions.partition_point(|t| t.at <= seconds);
            if i == 0 {
                Some(data.kinds.iter().position(|k| k.dst == 0).unwrap_or(0))
            } else if i == data.transitions.len() && !data.footer[0].is_empty() {
                None
            } else {
                Some(data.transitions[i - 1].kind)
            }
        });
        let (mut offset, mut dst) = if utc {
            (0, 0)
        } else if let (Some(data), Some(kind)) = (&self.data, table_kind) {
            (data.kinds[kind].offset, data.kinds[kind].dst as i32)
        } else {
            let timestamp = jiff::Timestamp::from_second(seconds)?;
            let info = self.provider.to_offset_info(timestamp);
            (
                i64::from(info.offset().seconds()),
                i32::from(info.dst().is_dst()),
            )
        };
        if let Some(data) = &self.data {
            if !utc {
                let i = data.transitions.partition_point(|t| t.at <= seconds);
                if let Some(kind) = table_kind {
                    self.names = Default::default();
                    let k = &data.kinds[kind];
                    if i == 0 {
                        self.names[0] = k.name.clone();
                        if let Some(k) = data.kinds.iter().find(|k| k.dst == 1) {
                            self.names[1] = k.name.clone();
                        }
                    } else {
                        self.names[k.dst] = k.name.clone();
                        for t in &data.transitions[i..] {
                            let k = &data.kinds[t.kind];
                            if self.names[k.dst].is_empty() {
                                self.names[k.dst] = k.name.clone();
                            }
                        }
                    }
                    if self.names[0].is_empty() {
                        self.names[0] = self.names[1].clone();
                    }
                    if self.names[1].is_empty() {
                        self.names[1] = self.names[0].clone();
                    }
                    offset = k.offset;
                    dst = k.dst as i32;
                } else {
                    self.names = data.footer.clone();
                }
            }
            let i = data.leaps.partition_point(|l| l.at <= seconds);
            if i > 0 {
                let leap = &data.leaps[i - 1];
                offset -= leap.correction;
                if leap.at == seconds
                    && (i == 1 && leap.correction > 0
                        || i > 1 && leap.correction > data.leaps[i - 2].correction)
                {
                    offset += 1;
                    let mut j = i - 1;
                    while j > 0
                        && data.leaps[j].at == data.leaps[j - 1].at + 1
                        && data.leaps[j].correction == data.leaps[j - 1].correction + 1
                    {
                        offset += 1;
                        j -= 1;
                    }
                }
            }
        }
        let civil = chrono::DateTime::from_timestamp(seconds + offset, 0)
            .ok_or_else(|| anyhow!("timestamp outside calendar range"))?
            .naive_utc();
        Ok((civil, dst, offset))
    }

    // Rendering borrows names instead of updating the parser's abbreviation state.
    pub fn format_display(
        &self,
        seconds: i64,
        nanos: u32,
        utc: bool,
        pattern: &str,
        named: bool,
        numeric: bool,
    ) -> Result<String> {
        let provider;
        let (name, offset) = if utc {
            ("UTC", 0)
        } else if let Some(kind) = self.data.as_ref().and_then(|data| {
            let i = data.transitions.partition_point(|t| t.at <= seconds);
            if i == 0 {
                Some(&data.kinds[data.kinds.iter().position(|k| k.dst == 0).unwrap_or(0)])
            } else if i < data.transitions.len() || data.footer[0].is_empty() {
                Some(&data.kinds[data.transitions[i - 1].kind])
            } else {
                None
            }
        }) {
            (kind.name.as_str(), kind.offset)
        } else {
            provider = self
                .provider
                .to_offset_info(jiff::Timestamp::from_second(seconds)?);
            (
                provider.abbreviation(),
                i64::from(provider.offset().seconds()),
            )
        };
        let (correction, leap) = self.data.as_ref().map_or((0, false), |data| {
            let i = data.leaps.partition_point(|l| l.at <= seconds);
            if i == 0 {
                return (0, false);
            }
            let leap = &data.leaps[i - 1];
            (
                leap.correction,
                leap.at == seconds
                    && (i == 1 && leap.correction > 0
                        || i > 1 && leap.correction > data.leaps[i - 2].correction),
            )
        });
        let dt = chrono::DateTime::from_timestamp(seconds + offset - correction, 0)
            .ok_or_else(|| anyhow!("invalid realtime timestamp"))?
            .naive_utc();
        let dt = dt
            .with_nanosecond(nanos + if leap { 1_000_000_000 } else { 0 })
            .ok_or_else(|| anyhow!("invalid leap timestamp"))?;
        let mut text = dt.format(pattern).to_string();
        if named && !name.is_empty() {
            text.push(' ');
            text.push_str(name);
        }
        if numeric {
            use std::fmt::Write;
            let sign = if offset < 0 { '-' } else { '+' };
            let offset = offset.abs();
            write!(text, "{sign}{:02}:{:02}", offset / 3600, offset % 3600 / 60)?;
        }
        Ok(text)
    }

    pub fn leap_hit(&self, seconds: i64) -> bool {
        let Some(data) = &self.data else {
            return false;
        };
        let i = data.leaps.partition_point(|l| l.at < seconds);
        i < data.leaps.len()
            && data.leaps[i].at == seconds
            && (i == 0 && data.leaps[i].correction > 0
                || i > 0 && data.leaps[i].correction > data.leaps[i - 1].correction)
    }
}

impl Tzif {
    fn parse(bytes: &[u8]) -> Option<Self> {
        fn counts(b: &[u8]) -> Option<[u64; 6]> {
            if b.len() < 44 || &b[..4] != b"TZif" {
                return None;
            }
            let mut n = [0; 6];
            for (i, v) in n.iter_mut().enumerate() {
                *v = u64::from(u32::from_be_bytes(
                    b[20 + i * 4..24 + i * 4].try_into().ok()?,
                ));
            }
            Some(n)
        }
        fn size(n: [u64; 6], w: usize) -> u64 {
            n[3] * (w as u64 + 1) + n[4] * 6 + n[5] + n[2] * (w as u64 + 4) + n[0] + n[1]
        }
        fn signed(b: &[u8], width: usize) -> Option<i64> {
            Some(if width == 4 {
                i64::from(i32::from_be_bytes(b.get(..4)?.try_into().ok()?))
            } else {
                i64::from_be_bytes(b.get(..8)?.try_into().ok()?)
            })
        }
        let mut bytes = bytes;
        let mut n = counts(bytes)?;
        let mut width = 4;
        if bytes[4] != 0 {
            let skip = usize::try_from(44 + size(n, 4)).ok()?;
            bytes = bytes.get(skip..)?;
            n = counts(bytes)?;
            width = 8;
        }
        let size = usize::try_from(size(n, width)).ok()?;
        if size > bytes.len() - 44 || n[4] == 0 || n[4] > 256 || n[0] > n[4] || n[1] > n[4] {
            return None;
        }
        let block = &bytes[44..44 + size];
        let mut at = 0;
        let mut transitions: Vec<Transition> = Vec::new();
        for _ in 0..n[3] {
            let epoch = signed(&block[at..], width)?;
            at += width;
            if transitions.last().is_some_and(|t| epoch <= t.at) {
                return None;
            }
            transitions.push(Transition { at: epoch, kind: 0 });
        }
        for t in &mut transitions {
            t.kind = usize::from(block[at]);
            at += 1;
            if t.kind as u64 >= n[4] {
                return None;
            }
        }
        let kinds_bytes = &block[at..at + n[4] as usize * 6];
        at += kinds_bytes.len();
        let names = &block[at..at + n[5] as usize];
        at += names.len();
        let name_at = |i: usize| -> Option<String> {
            let b = names.get(i..)?;
            let end = b.iter().position(|b| *b == 0)?;
            Some(std::str::from_utf8(&b[..end]).ok()?.to_owned())
        };
        let first_name = name_at(0)?;
        let mut kinds = Vec::new();
        for b in kinds_bytes.chunks_exact(6) {
            if b[4] > 1 {
                return None;
            }
            kinds.push(Kind {
                offset: i64::from(i32::from_be_bytes(b[..4].try_into().ok()?)),
                dst: usize::from(b[4]),
                name: name_at(usize::from(b[5]))?,
                standard: false,
                universal: false,
            });
        }
        let mut leaps: Vec<Leap> = Vec::new();
        for _ in 0..n[2] {
            let epoch = signed(&block[at..], width)?;
            at += width;
            let correction = i64::from(i32::from_be_bytes(block[at..at + 4].try_into().ok()?));
            at += 4;
            if leaps.last().is_some_and(|l| epoch <= l.at) {
                return None;
            }
            leaps.push(Leap {
                at: epoch,
                correction,
            });
        }
        for kind in kinds.iter_mut().take(n[1] as usize) {
            kind.standard = block[at] != 0;
            at += 1;
        }
        for kind in kinds.iter_mut().take(n[0] as usize) {
            kind.universal = block[at] != 0;
            at += 1;
        }
        let footer_rule = if width == 8 {
            std::str::from_utf8(&bytes[44 + size..])
                .ok()?
                .trim_matches('\n')
                .to_owned()
        } else {
            String::new()
        };
        let footer = footer_names(&footer_rule);
        Some(Self {
            kinds,
            transitions,
            leaps,
            first_name,
            footer,
            footer_rule,
        })
    }
}

fn footer_names(mut text: &str) -> [String; 2] {
    fn name(text: &mut &str) -> String {
        if text.starts_with('<') {
            if let Some(i) = text.find('>').filter(|i| *i > 1) {
                let n = text[1..i].to_owned();
                *text = &text[i + 1..];
                return n;
            }
            return String::new();
        }
        let i = text.bytes().take_while(u8::is_ascii_alphabetic).count();
        let n = text[..i].to_owned();
        *text = &text[i..];
        n
    }
    let standard = name(&mut text);
    text = text.trim_start_matches([
        '+', '-', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':',
    ]);
    let daylight = name(&mut text);
    [
        standard.clone(),
        if daylight.is_empty() {
            standard
        } else {
            daylight
        },
    ]
}

// A parsed standard name/offset survives a malformed remainder of a TZ rule.
fn fallback_rule(rule: &str) -> (String, i64) {
    let bytes = rule.as_bytes();
    let (name, mut rest) = if bytes.first() == Some(&b'<') {
        let Some(end) = bytes.iter().position(|&b| b == b'>') else {
            return (String::new(), 0);
        };
        let name = &bytes[1..end];
        if name.len() < 3
            || !name
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'-')
        {
            return (String::new(), 0);
        }
        (&rule[1..end], &rule[end + 1..])
    } else {
        let end = bytes.iter().take_while(|b| b.is_ascii_alphabetic()).count();
        if end < 3 {
            return (String::new(), 0);
        }
        (&rule[..end], &rule[end..])
    };
    let sign = if rest.starts_with('-') {
        rest = &rest[1..];
        1
    } else {
        if rest.starts_with('+') {
            rest = &rest[1..];
        }
        -1
    };
    let mut values = [0; 3];
    for (i, value) in values.iter_mut().enumerate() {
        let end = rest.bytes().take_while(u8::is_ascii_digit).count();
        if end == 0 {
            break;
        }
        let parsed = rest[..end].parse::<u64>().unwrap_or(u64::MAX) as u16;
        *value = i64::from(parsed.min(if i == 0 { 24 } else { 59 }));
        rest = &rest[end..];
        if !rest.starts_with(':') {
            break;
        }
        rest = &rest[1..];
    }
    (
        name.to_owned(),
        sign * (values[0] * 3600 + values[1] * 60 + values[2]),
    )
}

fn normalize_rule(mut rule: &str) -> String {
    let mut result = String::with_capacity(rule.len());
    for _ in 0..2 {
        let (name, _) = fallback_rule(rule);
        if name.is_empty() {
            break;
        }
        let end = name.len() + if rule.starts_with('<') { 2 } else { 0 };
        result.push_str(&rule[..end]);
        rule = &rule[end..];
        if rule.starts_with(['+', '-']) {
            result.push_str(&rule[..1]);
            rule = &rule[1..];
        }
        for part in 0..3 {
            let end = rule.bytes().take_while(u8::is_ascii_digit).count();
            if end == 0 {
                break;
            }
            let value = rule[..end].parse::<u64>().unwrap_or(u64::MAX) as u16;
            result.push_str(&value.min(if part == 0 { 24 } else { 59 }).to_string());
            rule = &rule[end..];
            if part == 2 || !rule.starts_with(':') {
                break;
            }
            result.push(':');
            rule = &rule[1..];
        }
    }
    result.push_str(rule);
    result
}
