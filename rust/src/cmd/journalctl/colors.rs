use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MatchRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Colors {
    level: u16,
    pub underline: bool,
}

pub(super) const NORMAL: &str = "\x1b[0m";
pub(super) const HIGHLIGHT: &str = "\x1b[0;1;39m";
pub(super) const RED: &str = "\x1b[0;1;31m";
pub(super) const BLUE: &str = "\x1b[0;34m";
pub(super) const GREEN: &str = "\x1b[0;32m";

impl Colors {
    pub fn detect() -> Self {
        let mode = std::env::var("SYSTEMD_COLORS").unwrap_or_default();
        let level = match mode.as_str() {
            "off" => 0,
            "16" => 16,
            "256" => 256,
            "24bit" => 24,
            _ => {
                let force = super::parse_bool_option("SYSTEMD_COLORS", &mode).ok();
                if force == Some(false) {
                    return Self::default();
                }
                if force != Some(true) {
                    let term = std::env::var_os("TERM");
                    if std::env::var_os("NO_COLOR").is_some()
                        || term.as_deref().is_none_or(|v| v == "dumb")
                        || (std::process::id() != 1
                            && !(std::io::stdout().is_terminal()
                                && std::io::stderr().is_terminal()))
                    {
                        return Self::default();
                    }
                }
                match mode.as_str() {
                    "auto-16" => 16,
                    "auto-256" => 256,
                    "auto-24bit" => 24,
                    _ if std::env::var("COLORTERM")
                        .is_ok_and(|v| matches!(v.as_str(), "truecolor" | "24bit")) =>
                    {
                        24
                    }
                    _ => 256,
                }
            }
        };
        Self {
            level,
            underline: level != 0
                && std::env::var_os("TERM")
                    .as_deref()
                    .is_none_or(|v| v != "linux"),
        }
    }

    pub fn enabled(self) -> bool {
        self.level != 0
    }

    pub fn grey(self) -> &'static str {
        match self.level {
            16 => "\x1b[0;90m",
            24 => "\x1b[0;38:5:245m",
            _ => "\x1b[0;90m\x1b[0;38:5:245m",
        }
    }

    pub fn log(self, priority: u8, audit: bool) -> (&'static str, &'static str, &'static str) {
        if !self.enabled() {
            return ("", "", "");
        }
        let (mut on, highlight) = match priority {
            0..=3 => (RED, HIGHLIGHT),
            4 => (
                match self.level {
                    16 => "\x1b[0;1;33m",
                    24 => "\x1b[0;1;38:5:185m",
                    _ => "\x1b[0;1;33m\x1b[0;1;38:5:185m",
                },
                HIGHLIGHT,
            ),
            5 => (HIGHLIGHT, RED),
            7.. => (self.grey(), RED),
            _ => ("", ""),
        };
        if audit && on.is_empty() {
            on = BLUE;
        }
        (on, if on.is_empty() { "" } else { NORMAL }, highlight)
    }

    pub fn boot(self, id: &str) -> String {
        if self.enabled() {
            format!("{HIGHLIGHT}-- Boot {id} --{NORMAL}\n")
        } else {
            format!("-- Boot {id} --\n")
        }
    }

    pub fn table_header(self, id: &str) -> String {
        if !self.enabled() || !self.underline {
            return format!("IDX {id:<32} FIRST ENTRY                 LAST ENTRY\n");
        }
        let on = if self.underline { "\x1b[4m" } else { "" };
        let fields = [
            "IDX".to_owned(),
            format!("{id:<32}"),
            format!("{:<27}", "FIRST ENTRY"),
            format!("{:<27}", "LAST ENTRY"),
        ];
        format!(
            "{on}{}{NORMAL}\n",
            fields.join(&format!("{NORMAL}{on} {NORMAL}{on}"))
        )
    }
}

// This scans an already valid JSON body; callers retain their existing framing.
pub(super) fn json(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let start = i;
        match data[i] {
            b'"' => {
                i += 1;
                while i < data.len() && data[i] != b'"' {
                    if data[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                out.push(b'"');
                out.extend_from_slice(GREEN.as_bytes());
                out.extend_from_slice(&data[start + 1..i]);
                out.extend_from_slice(NORMAL.as_bytes());
                out.push(b'"');
                i += 1;
            }
            b'-' | b'0'..=b'9' | b'n' | b't' | b'f' => {
                let on = if matches!(data[i], b'n' | b't' | b'f') {
                    HIGHLIGHT
                } else {
                    "\x1b[0;1;34m"
                };
                while i < data.len() && !b",]} \t\r\n".contains(&data[i]) {
                    i += 1;
                }
                out.extend_from_slice(on.as_bytes());
                out.extend_from_slice(&data[start..i]);
                out.extend_from_slice(NORMAL.as_bytes());
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}
