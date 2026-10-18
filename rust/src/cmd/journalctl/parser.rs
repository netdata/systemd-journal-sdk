use super::*;
use clap::CommandFactory;
use std::ffi::{OsStr, OsString};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    None,
    Required,
    Optional,
}

// The option kinds follow the pinned v262 surface, including hidden options.
const OPTIONS: &[(&str, Option<u8>, Kind)] = &[
    ("system", None, Kind::None),
    ("user", None, Kind::None),
    ("machine", Some(b'M'), Kind::Required),
    ("merge", Some(b'm'), Kind::None),
    ("directory", Some(b'D'), Kind::Required),
    ("file", Some(b'i'), Kind::Required),
    ("root", None, Kind::Required),
    ("image", None, Kind::Required),
    ("image-policy", None, Kind::Required),
    ("namespace", None, Kind::Required),
    ("since", Some(b'S'), Kind::Required),
    ("until", Some(b'U'), Kind::Required),
    ("cursor", Some(b'c'), Kind::Required),
    ("after-cursor", None, Kind::Required),
    ("cursor-file", None, Kind::Required),
    ("this-boot", None, Kind::None),
    ("boot", Some(b'b'), Kind::Optional),
    ("unit", Some(b'u'), Kind::Required),
    ("user-unit", None, Kind::Required),
    ("invocation", None, Kind::Required),
    ("I", Some(b'I'), Kind::None),
    ("identifier", Some(b't'), Kind::Required),
    ("exclude-identifier", Some(b'T'), Kind::Required),
    ("priority", Some(b'p'), Kind::Required),
    ("facility", None, Kind::Required),
    ("grep", Some(b'g'), Kind::Required),
    ("case-sensitive", None, Kind::Optional),
    ("dmesg", Some(b'k'), Kind::None),
    ("output", Some(b'o'), Kind::Required),
    ("output-fields", None, Kind::Required),
    ("lines", Some(b'n'), Kind::Optional),
    ("reverse", Some(b'r'), Kind::None),
    ("show-cursor", None, Kind::None),
    ("utc", None, Kind::None),
    ("catalog", Some(b'x'), Kind::None),
    ("no-hostname", Some(b'W'), Kind::None),
    ("full", Some(b'l'), Kind::None),
    ("no-full", None, Kind::None),
    ("all", Some(b'a'), Kind::None),
    ("follow", Some(b'f'), Kind::None),
    ("no-tail", None, Kind::None),
    ("truncate-newline", None, Kind::None),
    ("quiet", Some(b'q'), Kind::None),
    ("synchronize-on-exit", None, Kind::Required),
    ("no-pager", None, Kind::None),
    ("pager-end", Some(b'e'), Kind::None),
    ("interval", None, Kind::Required),
    ("verify-key", None, Kind::Required),
    ("force", None, Kind::None),
    ("help", Some(b'h'), Kind::None),
    ("version", None, Kind::None),
    ("fields", Some(b'N'), Kind::None),
    ("field", Some(b'F'), Kind::Required),
    ("list-boots", None, Kind::None),
    ("list-invocations", None, Kind::None),
    ("list-namespaces", None, Kind::None),
    ("disk-usage", None, Kind::None),
    ("vacuum-size", None, Kind::Required),
    ("vacuum-files", None, Kind::Required),
    ("vacuum-time", None, Kind::Required),
    ("verify", None, Kind::None),
    ("sync", None, Kind::None),
    ("relinquish-var", None, Kind::None),
    ("smart-relinquish-var", None, Kind::None),
    ("flush", None, Kind::None),
    ("rotate", None, Kind::None),
    ("header", None, Kind::None),
    ("list-catalog", None, Kind::None),
    ("dump-catalog", None, Kind::None),
    ("update-catalog", None, Kind::None),
    ("setup-keys", None, Kind::None),
    ("new-id128", None, Kind::None),
    ("introspect-cli", None, Kind::None),
    ("head", None, Kind::Required),
    ("tail", None, Kind::Required),
    ("verify-only", None, Kind::None),
];
fn os_slice(bytes: &[u8]) -> &OsStr {
    // SAFETY: callers split an encoded argv value only at ASCII option separators.
    unsafe { OsStr::from_encoded_bytes_unchecked(bytes) }
}

fn long_option(name: &str) -> Result<(&'static str, Kind)> {
    if let Some(&(name, _, kind)) = OPTIONS.iter().find(|&&(n, _, _)| n != "I" && n == name) {
        return Ok((name, kind));
    }
    let mut matches = OPTIONS
        .iter()
        .filter(|&&(n, _, _)| n != "I" && n.starts_with(name));
    let Some(&(found, _, kind)) = matches.next() else {
        return Err(anyhow!("unrecognized option '--{name}'"));
    };
    if matches.next().is_some() {
        return Err(anyhow!("option '--{name}' is ambiguous"));
    }
    Ok((found, kind))
}

fn option_token(name: &str, value: &OsStr) -> OsString {
    let mut out = OsString::from(format!("--{name}="));
    out.push(value);
    out
}

pub fn parse_cli(raw: Vec<OsString>) -> Result<Option<Args>> {
    let mut tokens = vec![raw.first().cloned().unwrap_or_else(|| "journalctl".into())];
    let mut operands = Vec::new();
    let mut action = "show";
    let mut field = None;
    let mut output = "short".to_string();
    let mut quiet = false;
    let mut full_width = true;
    let mut projection = Vec::new();
    let mut timestamps = timestamp::Parser::default();
    let mut since_usec = None;
    let mut until_usec = None;
    let mut i = 1;
    while i < raw.len() {
        let arg = raw[i].as_encoded_bytes();
        if arg == b"--" {
            operands.extend_from_slice(&raw[i + 1..]);
            break;
        }
        if arg == b"-" || !arg.starts_with(b"-") {
            operands.push(raw[i].clone());
            i += 1;
            continue;
        }
        let long = arg.starts_with(b"--");
        let mut token = &arg[if long { 2 } else { 1 }..];
        while !token.is_empty() {
            let name;
            let kind;
            let mut value: Option<OsString> = None;
            if long {
                let (key, inline) = match token.iter().position(|&b| b == b'=') {
                    Some(n) => (&token[..n], Some(&token[n + 1..])),
                    None => (token, None),
                };
                let key = std::str::from_utf8(key).map_err(|_| anyhow!("unrecognized option"))?;
                (name, kind) = long_option(key)?;
                value = inline.map(|bytes| os_slice(bytes).to_owned());
                token = &[];
            } else {
                let Some(&(found, _, found_kind)) =
                    OPTIONS.iter().find(|&&(_, s, _)| s == Some(token[0]))
                else {
                    return Err(anyhow!("invalid option -- '{}'", token[0] as char));
                };
                name = found;
                kind = found_kind;
                token = &token[1..];
                if kind != Kind::None && !token.is_empty() {
                    value = Some(os_slice(token).to_owned());
                    token = &[];
                }
            }
            if kind == Kind::None && value.is_some() {
                return Err(anyhow!("option '--{name}' doesn't allow an argument"));
            }
            if kind == Kind::Required && value.is_none() {
                i += 1;
                value = Some(
                    raw.get(i)
                        .ok_or_else(|| anyhow!("option '--{name}' requires an argument"))?
                        .clone(),
                );
            }
            if kind == Kind::Optional && value.is_none() {
                let mut default = match name {
                    "boot" => "0",
                    "lines" => "10",
                    _ => "true",
                }
                .into();
                if name != "case-sensitive" {
                    if let Some(next) = raw.get(i + 1).and_then(|s| s.to_str()) {
                        let valid = if name == "boot" {
                            next == "all" || parse_boot_descriptor(next).is_ok()
                        } else {
                            parse_lines_limit(Some(next)).is_ok()
                        };
                        if valid {
                            i += 1;
                            default = raw[i].clone();
                        }
                    }
                }
                value = Some(default);
            }
            let text = value.as_ref().and_then(|v| v.to_str()).unwrap_or("");
            match name {
                "help" => {
                    Args::command().print_long_help()?;
                    println!();
                    return Ok(None);
                }
                "version" => {
                    println!("journalctl (systemd-journal-sdk Rust rewrite)");
                    println!("CLI compatibility: systemd v262 (8cc40e0c5e92)");
                    println!("portable file-backed mode; PCRE2 {}", pcre::version());
                    return Ok(None);
                }
                "introspect-cli" => {
                    print_introspection(&output)?;
                    return Ok(None);
                }
                "output" => {
                    if text == "help" {
                        print_output_mode_help();
                        return Ok(None);
                    }
                    if !OUTPUT_MODE_HELP_LIST.contains(&text) {
                        return Err(anyhow!("Unknown output format '{text}'."));
                    }
                    output = text.into();
                    if matches!(text, "cat" | "export") || text.starts_with("json") {
                        quiet = true;
                    }
                }
                "facility" => {
                    for part in text.split(',').filter(|s| !s.is_empty()) {
                        if part == "help" {
                            print_facility_help(quiet);
                            return Ok(None);
                        }
                        parse_facility_filter(&[part.to_owned()])?;
                    }
                }
                "priority" => {
                    parse_priority_filter(&[text.to_owned()])?;
                }
                "lines" => {
                    parse_lines_limit(Some(text))?;
                }
                "boot" => {
                    if text != "all" {
                        parse_boot_descriptor(text)?;
                    }
                }
                "invocation" => {
                    parse_invocation_descriptor(text)?;
                }
                "case-sensitive" | "synchronize-on-exit" => {
                    parse_bool_option(&format!("--{name}"), text)?;
                }
                "since" | "until" => {
                    let parsed = timestamps
                        .parse(text)
                        .map_err(|_| anyhow!("Failed to parse timestamp: {text}"))?;
                    if name == "since" {
                        since_usec = Some(parsed);
                    } else {
                        until_usec = Some(parsed);
                    }
                }
                "vacuum-size" => {
                    parse_vacuum_size(text)?;
                }
                "vacuum-files" => {
                    parse_native_u64(text)
                        .ok_or_else(|| anyhow!("Failed to parse vacuum files: {text}"))?;
                }
                "vacuum-time" | "interval" => {
                    let duration = parse_duration_usec_allow_zero(text)?;
                    if name == "interval" && duration == 0 {
                        return Err(anyhow!("Failed to parse interval: {text}"));
                    }
                }
                "head" | "tail" => {
                    text.parse::<usize>()
                        .map_err(|_| anyhow!("--{name} must be a non-negative integer"))?;
                }
                "full" => full_width = true,
                "no-full" => full_width = false,
                "quiet" => quiet = true,
                _ => {}
            }
            match name {
                "field" => {
                    action = "field";
                    field = value;
                }
                "fields"
                | "list-boots"
                | "list-invocations"
                | "list-namespaces"
                | "disk-usage"
                | "header"
                | "list-catalog"
                | "dump-catalog"
                | "update-catalog"
                | "setup-keys"
                | "new-id128"
                | "verify"
                | "verify-only"
                | "sync"
                | "relinquish-var"
                | "smart-relinquish-var"
                | "flush" => action = name,
                "rotate" => {
                    action = if action == "vacuum" {
                        "rotate-and-vacuum"
                    } else {
                        "rotate"
                    }
                }
                "vacuum-size" | "vacuum-files" | "vacuum-time" => {
                    action = if action == "rotate" {
                        "rotate-and-vacuum"
                    } else {
                        "vacuum"
                    };
                    tokens.push(option_token(name, value.as_deref().unwrap()));
                }
                "verify-key" => {
                    action = "verify";
                    tokens.retain(|t| t != "--merge");
                    tokens.push(option_token(name, value.as_deref().unwrap()));
                }
                "root" | "image" | "namespace" if value.as_deref().is_some_and(OsStr::is_empty) => {
                    let prefix = format!("--{name}=");
                    tokens.retain(|t| !t.as_encoded_bytes().starts_with(prefix.as_bytes()));
                }
                "this-boot" => tokens.push(option_token("boot", OsStr::new("0"))),
                "I" => tokens.push(option_token("invocation", OsStr::new("0"))),
                "output-fields" => projection.push(value.as_ref().unwrap().clone()),
                _ if kind == Kind::None => tokens.push(format!("--{name}").into()),
                _ => tokens.push(option_token(name, value.as_deref().unwrap())),
            }
        }
        i += 1;
    }
    match action {
        "show" | "vacuum" => {}
        "field" => tokens.push(option_token("field", field.as_deref().unwrap())),
        "rotate-and-vacuum" => tokens.push("--rotate".into()),
        other => tokens.push(format!("--{other}").into()),
    }
    if quiet {
        tokens.push("--quiet".into());
    }
    if !full_width {
        tokens.push("--no-full".into());
    }
    tokens.push("--".into());
    tokens.extend(operands);
    let mut args = Args::try_parse_from(tokens).map_err(|e| anyhow!(e.to_string()))?;
    args.action = Some(action);
    args.since_usec = since_usec;
    args.until_usec = until_usec;
    args.full = full_width;
    args.no_full = !full_width;
    args.output_fields = projection;
    if args.no_tail {
        args.lines = Some("all".into());
        args.head = None;
        args.tail = None;
    }
    Ok(Some(args))
}

fn print_introspection(output: &str) -> Result<()> {
    let options: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../../../go/cmd/journalctl/options.json"))?;
    let value = serde_json::json!({"mediaType":"application/vnd.io.systemd.cli-introspection-0",
        "commands":[{"names":["journalctl"],"project":"systemd-journal-sdk", "version":env!("CARGO_PKG_VERSION"),
            "features":[format!("PCRE2 {}", pcre::version())], "abstract":["Query file-backed journals."], "options":options}]});
    let data = if output == "json-pretty" {
        serde_json::to_string_pretty(&value)?
    } else {
        serde_json::to_string(&value)?
    };
    match output {
        "json-sse" => println!("data: {data}\n"),
        "json-seq" => println!("\x1e{data}"),
        _ => println!("{data}"),
    }
    Ok(())
}
