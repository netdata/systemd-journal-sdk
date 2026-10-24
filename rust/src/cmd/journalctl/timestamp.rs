use crate::timestamp_zone::Zone;
use anyhow::{Result, anyhow};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};

const MAX: u64 = 253_402_214_399_000_000;

#[derive(Default)]
pub struct Parser {
    local_guess: i64,
    utc_guess: i64,
    local: Option<Zone>,
}

pub fn duration(value: &str) -> Result<u64> {
    let error = || anyhow!("failed to parse duration: {value}");
    let units = [
        ("seconds", 1_000_000),
        ("second", 1_000_000),
        ("sec", 1_000_000),
        ("s", 1_000_000),
        ("minutes", 60_000_000),
        ("minute", 60_000_000),
        ("min", 60_000_000),
        ("months", 2_629_800_000_000),
        ("month", 2_629_800_000_000),
        ("M", 2_629_800_000_000),
        ("msec", 1_000),
        ("ms", 1_000),
        ("m", 60_000_000),
        ("hours", 3_600_000_000),
        ("hour", 3_600_000_000),
        ("hr", 3_600_000_000),
        ("h", 3_600_000_000),
        ("days", 86_400_000_000),
        ("day", 86_400_000_000),
        ("d", 86_400_000_000),
        ("weeks", 604_800_000_000),
        ("week", 604_800_000_000),
        ("w", 604_800_000_000),
        ("years", 31_557_600_000_000),
        ("year", 31_557_600_000_000),
        ("y", 31_557_600_000_000),
        ("usec", 1),
        ("us", 1),
        ("μs", 1),
        ("µs", 1),
    ];
    let mut rest = value.trim_start_matches(ascii_space);
    if rest.trim_end_matches(ascii_space) == "infinity" {
        return Ok(u64::MAX);
    }
    let mut total = 0u64;
    let mut seen = false;
    loop {
        rest = rest.trim_start_matches(ascii_space);
        if rest.is_empty() {
            return if seen { Ok(total) } else { Err(error()) };
        }
        if rest.starts_with('-') {
            return Err(error());
        }
        let start = usize::from(rest.starts_with('+'));
        let end = start
            + rest.as_bytes()[start..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        let integer = if end > start {
            rest[start..end].parse::<i64>().map_err(|_| error())? as u64
        } else if start == 0 && rest.starts_with('.') {
            0
        } else {
            return Err(error());
        };
        let mut n = end;
        let mut fraction = "";
        if rest[n..].starts_with('.') {
            n += 1;
            let end = n + rest.as_bytes()[n..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if end == n {
                return Err(error());
            }
            fraction = &rest[n..end];
            n = end;
        }
        let after = &rest[n..];
        let mut suffix = after.trim_start_matches(ascii_space);
        let mut multiplier = 1_000_000u64;
        let mut matched = false;
        for (name, unit) in units {
            if let Some(next) = suffix.strip_prefix(name) {
                multiplier = unit;
                suffix = next;
                matched = true;
                break;
            }
        }
        if !matched && after.len() == suffix.len() && !suffix.is_empty() {
            return Err(error());
        }
        if integer >= u64::MAX / multiplier {
            return Err(error());
        }
        total = total
            .checked_add(integer * multiplier)
            .filter(|v| *v < u64::MAX)
            .ok_or_else(error)?;
        let mut weight = multiplier / 10;
        for digit in fraction.bytes() {
            total = total
                .checked_add(u64::from(digit - b'0') * weight)
                .filter(|v| *v < u64::MAX)
                .ok_or_else(error)?;
            weight /= 10;
        }
        rest = suffix;
        seen = true;
    }
}

fn ascii_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{b}' | '\u{c}')
}

fn utc_offset(value: &str) -> Option<i64> {
    if value == "Z" {
        return Some(0);
    }
    if !matches!(value.len(), 3 | 5 | 6) || !matches!(value.as_bytes()[0], b'+' | b'-') {
        return None;
    }
    let pair = |s: &[u8]| -> Option<i64> {
        (s.len() == 2 && s.iter().all(u8::is_ascii_digit))
            .then(|| i64::from(s[0] - b'0') * 10 + i64::from(s[1] - b'0'))
    };
    let bytes = value.as_bytes();
    let hour = pair(&bytes[1..3])?;
    let minute = if bytes.len() == 3 {
        0
    } else if bytes.len() == 6 {
        if bytes[3] != b':' {
            return None;
        }
        pair(&bytes[4..])?
    } else {
        pair(&bytes[3..])?
    };
    let offset = hour * 3600 + minute * 60;
    if minute >= 60 || offset > 86400 {
        return None;
    }
    Some(if bytes[0] == b'-' { -offset } else { offset })
}

impl Parser {
    pub fn parse(&mut self, value: &str) -> Result<u64> {
        if let Some(epoch) = value.strip_prefix('@') {
            return duration(epoch);
        }
        let mut local = self.local.take().unwrap_or_else(Zone::local);
        let result = self.parse_calendar_value(value, &mut local);
        self.local = Some(local);
        result
    }

    fn parse_calendar_value(&mut self, value: &str, local: &mut Zone) -> Result<u64> {
        if value.len() > 2 && value.ends_with('Z') {
            if let Ok(result) =
                self.parse_in_zone(value, &value[..value.len() - 1], local, true, -1, 0, true)
            {
                return Ok(result);
            }
        }
        let bytes = value.as_bytes();
        if value.len() > 7
            && bytes[value.len() - 7] != b' '
            && matches!(bytes[value.len() - 6], b'+' | b'-')
        {
            if let Some(offset) = value.get(value.len() - 6..).and_then(utc_offset) {
                return self.parse_in_zone(
                    value,
                    &value[..value.len() - 6],
                    local,
                    true,
                    -1,
                    offset,
                    true,
                );
            }
        }
        if let Some((prefix, suffix)) = value.rsplit_once(' ') {
            if suffix == "UTC" {
                return self.parse_in_zone(value, prefix, local, true, -1, 0, true);
            }
            if let Some(offset) = utc_offset(suffix) {
                return self.parse_in_zone(value, prefix, local, true, -1, offset, true);
            }
            if let Some(dst) = local.abbreviation(suffix) {
                return self.parse_in_zone(value, prefix, local, false, dst, 0, true);
            }
            if let Some(mut named) = Zone::named(suffix) {
                let result = self.parse_in_zone(value, prefix, &mut named, false, -1, 0, true);
                local.restore_after_named(&named);
                return result;
            }
        }
        self.parse_in_zone(value, value, local, false, -1, 0, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_in_zone(
        &mut self,
        value: &str,
        mut text: &str,
        zone: &mut Zone,
        utc: bool,
        isdst: i32,
        offset: i64,
        with_zone: bool,
    ) -> Result<u64> {
        let error = || anyhow!("failed to parse timestamp: {value}");
        let now = chrono::Utc::now();
        if !with_zone {
            if value == "now" {
                return u64::try_from(now.timestamp_micros()).map_err(|_| error());
            }
            let relative = if let Some(d) = value.strip_prefix('+') {
                Some((d, false))
            } else if let Some(d) = value.strip_prefix('-') {
                Some((d, true))
            } else if let Some(d) = value.strip_suffix(" ago") {
                Some((d, true))
            } else {
                value.strip_suffix(" left").map(|d| (d, false))
            };
            if let Some((d, negative)) = relative {
                let delta = duration(d)?;
                let base = u64::try_from(now.timestamp_micros()).map_err(|_| error())?;
                let result = if negative {
                    base.checked_sub(delta)
                } else {
                    base.checked_add(delta)
                }
                .filter(|v| *v <= MAX)
                .ok_or_else(error)?;
                return Ok(result);
            }
        }
        let (seed, _, _) = zone.civil(now.timestamp(), utc)?;
        let mut weekday = None;
        let (civil, second, fraction) = if matches!(text, "today" | "yesterday" | "tomorrow") {
            let day = seed.date().and_hms_opt(0, 0, 0).ok_or_else(error)?;
            let shift = match text {
                "yesterday" => -1,
                "tomorrow" => 1,
                _ => 0,
            };
            (day + chrono::Duration::days(shift), 0, 0)
        } else {
            for (i, name) in [
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
            ]
            .iter()
            .enumerate()
            {
                for prefix in [*name, &name[..3]] {
                    if text
                        .get(..prefix.len())
                        .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
                        && text.as_bytes().get(prefix.len()) == Some(&b' ')
                    {
                        weekday = Some(i as u32);
                        text = &text[prefix.len() + 1..];
                        break;
                    }
                }
                if weekday.is_some() {
                    break;
                }
            }
            calendar(text, seed).ok_or_else(error)?
        };
        if weekday.is_some_and(|w| w != civil.weekday().num_days_from_sunday()) {
            return Err(error());
        }
        let guess = if utc {
            &mut self.utc_guess
        } else {
            &mut self.local_guess
        };
        let seconds = inverse(civil, zone, utc, isdst, guess)? + i64::from(second)
            - i64::from(civil.second())
            - offset;
        let micros = u64::try_from(seconds)
            .map_err(|_| error())?
            .checked_mul(1_000_000)
            .and_then(|v| v.checked_add(fraction))
            .filter(|v| *v <= MAX)
            .ok_or_else(error)?;
        Ok(micros)
    }
}

fn calendar(value: &str, seed: NaiveDateTime) -> Option<(NaiveDateTime, u32, u64)> {
    let formats = [
        "%y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%b %d %H:%M:%S",
        "%y-%m-%d %H:%M",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
        "%y-%m-%d",
        "%Y-%m-%d",
        "%H:%M:%S",
        "%H:%M",
    ];
    for format in formats {
        let (mut year, mut month, mut day) = (seed.year(), seed.month(), seed.day());
        let (mut hour, mut minute, mut second) = if format.contains("%H") {
            (
                seed.hour(),
                seed.minute(),
                if format.contains("%S") {
                    seed.second()
                } else {
                    0
                },
            )
        } else {
            (0, 0, 0)
        };
        let mut text = value;
        let mut valid = true;
        let mut chars = format.chars();
        while let Some(c) = chars.next() {
            if c == ' ' {
                text = text.trim_start_matches(ascii_space);
                continue;
            }
            if c != '%' {
                if let Some(next) = text.strip_prefix(c) {
                    text = next;
                } else {
                    valid = false;
                    break;
                }
                continue;
            }
            let code = chars.next()?;
            if code == 'b' {
                let mut found = false;
                for (m, name) in [
                    "January",
                    "February",
                    "March",
                    "April",
                    "May",
                    "June",
                    "July",
                    "August",
                    "September",
                    "October",
                    "November",
                    "December",
                ]
                .iter()
                .enumerate()
                {
                    for prefix in [*name, &name[..3]] {
                        if text
                            .get(..prefix.len())
                            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
                        {
                            month = m as u32 + 1;
                            text = &text[prefix.len()..];
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
                if !found {
                    valid = false;
                    break;
                }
                continue;
            }
            text = text.trim_start_matches(ascii_space);
            let width = if code == 'Y' { 4 } else { 2 };
            let n = text
                .bytes()
                .take(width)
                .take_while(u8::is_ascii_digit)
                .count();
            if n == 0 {
                valid = false;
                break;
            }
            let number: u32 = text[..n].parse().ok()?;
            text = &text[n..];
            match code {
                'y' => {
                    year = if number >= 69 {
                        1900 + number as i32
                    } else {
                        2000 + number as i32
                    }
                }
                'Y' => year = number as i32,
                'm' => {
                    month = number;
                    valid = (1..=12).contains(&number);
                }
                'd' => {
                    day = number;
                    valid = (1..=31).contains(&number);
                }
                'H' => {
                    hour = number;
                    valid = number <= 23;
                }
                'M' => {
                    minute = number;
                    valid = number <= 59;
                }
                'S' => {
                    second = number;
                    valid = number <= 61;
                }
                _ => unreachable!(),
            }
            if !valid {
                break;
            }
        }
        let mut fraction = 0;
        if valid && format.contains("%S") && text.starts_with('.') {
            text = &text[1..];
            if text.is_empty() {
                valid = false;
            }
            for (i, c) in text.bytes().enumerate() {
                if !c.is_ascii_digit() {
                    valid = false;
                    break;
                }
                if i < 6 {
                    fraction = fraction * 10 + u64::from(c - b'0');
                } else if i == 6 && c >= b'5' {
                    fraction += 1;
                }
            }
            if text.len() < 6 {
                for _ in text.len()..6 {
                    fraction *= 10;
                }
            }
            text = "";
        }
        if valid && text.is_empty() {
            let date = NaiveDate::from_ymd_opt(year, month, 1)?
                .checked_add_signed(chrono::Duration::days(i64::from(day) - 1))?;
            let civil = date.and_hms_opt(hour, minute, second.min(59))?;
            return Some((civil, second, fraction));
        }
    }
    None
}

// Cached UTC-to-civil inversion follows glibc's fixed-point/oscillation policy.
fn inverse(
    civil: NaiveDateTime,
    zone: &mut Zone,
    utc: bool,
    isdst: i32,
    guess: &mut i64,
) -> Result<i64> {
    let target = civil.and_utc().timestamp();
    let mut t = target + *guess;
    let (mut t1, mut t2) = (t, t);
    let mut previous_dst = -1;
    let mut matched = false;
    let mut oscillated = false;
    for _ in 0..6 {
        let (actual, dst, _) = zone.civil(t, utc)?;
        let diff = target - actual.and_utc().timestamp();
        if diff == 0 {
            matched = true;
            break;
        }
        if t == t1
            && t != t2
            && (dst < 0 || isdst < 0 && previous_dst <= dst || isdst >= 0 && dst != isdst)
        {
            matched = true;
            oscillated = true;
            break;
        }
        t1 = t2;
        t2 = t;
        previous_dst = dst;
        t += diff;
    }
    if !matched {
        return Err(anyhow!("timestamp conversion did not converge"));
    }
    let (_, dst, _) = zone.civil(t, utc)?;
    if isdst >= 0 && dst != isdst && !oscillated {
        let mut found = false;
        let mut delta = 601_200;
        while delta < 457_243_209 / 2 + 601_200 && !found {
            for direction in [-1, 1] {
                let candidate = t + direction * delta;
                let Ok((actual, other_dst, _)) = zone.civil(candidate, utc) else {
                    continue;
                };
                if other_dst == isdst {
                    t = candidate + target - actual.and_utc().timestamp();
                    found = true;
                    break;
                }
            }
            delta += 601_200;
        }
        if !found {
            t += i64::from(dst - isdst) * 3600;
        }
        zone.civil(t, utc)?;
    }
    *guess = t - target;
    // Distinguish an inserted second from the following minute's zero second.
    if civil.second() == 0 && zone.leap_hit(t) && zone.civil(t, utc)?.0.second() == 0 {
        t += 1;
        zone.civil(t, utc)?;
    }
    Ok(t)
}
