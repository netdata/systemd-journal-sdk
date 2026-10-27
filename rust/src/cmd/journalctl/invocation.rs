use super::*;
use journal::{SdJournalGetData, SdJournalGetRealtimeUsec};

fn single_unit(journal: &mut SdJournal, args: &Args, option: &str) -> Result<UnitSpecs> {
    let (system, user) = raw_unit_specs(args);
    match system.len() + user.len() {
        0 => {
            return Err(anyhow!(
                "Using {option} requires a unit. Please specify a unit name with -u/--unit=/--user-unit=."
            ));
        }
        1 => {}
        _ => {
            return Err(anyhow!(
                "Using {option} with multiple units is not supported."
            ));
        }
    }
    let (system, user) = effective_unit_specs(args)?;
    let (specs, fields) = if !user.is_empty() {
        (user, &USER_UNIT_FIELDS_FULL[..3])
    } else {
        (system, &SYSTEM_UNIT_FIELDS_FULL[..3])
    };
    let units = expand_unit_specs(journal, specs, fields)?;
    if units.is_empty() {
        return Err(anyhow!(
            "No matching unit found for '{}' in journal.",
            String::from_utf8_lossy(&specs[0])
        ));
    }
    if units.len() > 1 {
        return Err(anyhow!(
            "Multiple matching units found for '{}' in journal.",
            String::from_utf8_lossy(&specs[0])
        ));
    }
    Ok(if !user.is_empty() {
        (Vec::new(), units)
    } else {
        (units, Vec::new())
    })
}

fn boot_id(journal: &mut SdJournal, args: &Args) -> Result<String> {
    match effective_boot_descriptor(args) {
        Some(boot) if boot.trim() != "all" => resolve_boot_id(journal, boot.trim()),
        _ => Ok(String::new()),
    }
}

fn add_boot(journal: &mut SdJournal, boot: &str) -> Result<()> {
    if !boot.is_empty() {
        add_field_matches(journal, "_BOOT_ID", [boot.to_string()])?;
    }
    Ok(())
}

fn parse_id(value: &[u8]) -> Option<[u8; 16]> {
    if value.len() > 37 {
        return None;
    }
    let value = value.split(|c| *c == 0).next()?;
    let mut clean = [0; 32];
    let value = match value.len() {
        32 => value,
        36 => {
            if [8, 13, 18, 23].iter().any(|&i| value[i] != b'-') {
                return None;
            }
            let mut i = 0;
            for (pos, &c) in value.iter().enumerate() {
                if [8, 13, 18, 23].contains(&pos) {
                    continue;
                }
                clean[i] = c;
                i += 1;
            }
            &clean
        }
        _ => return None,
    };
    let mut id = [0; 16];
    hex::decode_to_slice(value, &mut id).ok()?;
    (id != [0; 16]).then_some(id)
}

fn eligibility(journal: &mut SdJournal) -> Result<Vec<Vec<u8>>> {
    let mut matches = Vec::new();
    for field in INVOCATION_ID_FIELDS {
        SdJournalVisitUniqueValues(journal, field, |value| {
            if parse_id(value).is_some() {
                let mut payload = format!("{field}=").into_bytes();
                payload.extend_from_slice(value);
                matches.push(payload);
            }
            Ok(())
        })?;
    }
    Ok(matches)
}

fn primary_id(journal: &mut SdJournal) -> Result<Option<String>> {
    for field in INVOCATION_ID_FIELDS {
        let payload = match SdJournalGetData(journal, field) {
            Ok(payload) => payload,
            Err(FacadeError::NoEntry) => continue,
            Err(err) => return Err(err.into()),
        };
        if let Some(id) = payload.get(field.len() + 1..).and_then(parse_id) {
            return Ok(Some(hex::encode(id)));
        }
    }
    Ok(None)
}

// Keep the discovery predicate fixed while a second indexed reader supplies
// global bounds. Metadata getters avoid expanding unrelated DATA payloads.
struct Walker {
    scan: SdJournal,
    bounds: Option<SdJournal>,
    boot: String,
    older: bool,
    done: bool,
    anchor: Option<String>,
    previous: Option<String>,
    units: UnitSpecs,
    eligibility: Vec<Vec<u8>>,
    unrestricted: bool,
}

impl Walker {
    fn new(
        input: &CliInput,
        args: &Args,
        option: Option<&str>,
        older: bool,
        show: bool,
    ) -> Result<Self> {
        let mut scan = input.open_journal()?;
        // Show validates boot before unit; invocation tables acquire unit first.
        let mut boot = if show {
            boot_id(&mut scan, args)?
        } else {
            String::new()
        };
        let (system, user) = match option {
            Some(option) => single_unit(&mut scan, args, option)?,
            None => (Vec::new(), Vec::new()),
        };
        if !show {
            boot = boot_id(&mut scan, args)?;
        }
        let eligibility = if option.is_some() {
            eligibility(&mut scan)?
        } else {
            Vec::new()
        };
        let done = option.is_some() && eligibility.is_empty();
        let mut walker = Self {
            scan,
            bounds: if done {
                None
            } else {
                Some(input.open_journal()?)
            },
            boot,
            older,
            done,
            anchor: None,
            previous: None,
            units: (system, user),
            eligibility,
            unrestricted: false,
        };
        if done {
            return Ok(walker);
        }
        walker.reset_scan(false)?;
        if older {
            SdJournalSeekTail(&mut walker.scan)?;
        } else {
            SdJournalSeekHead(&mut walker.scan)?;
        }
        Ok(walker)
    }

    fn add_unit_scope(journal: &mut SdJournal, units: &UnitSpecs) -> Result<()> {
        for unit in &units.0 {
            add_system_unit_match_groups(journal, unit, false)?;
        }
        let uid = current_uid_string();
        for unit in &units.1 {
            add_user_unit_match_groups(journal, unit, uid.as_deref(), false)?;
        }
        if !units.0.is_empty() || !units.1.is_empty() {
            SdJournalAddConjunction(journal)?;
        }
        Ok(())
    }

    fn reset_scan(&mut self, unrestricted: bool) -> Result<()> {
        SdJournalFlushMatches(&mut self.scan)?;
        if !unrestricted {
            add_boot(&mut self.scan, &self.boot)?;
            Self::add_unit_scope(&mut self.scan, &self.units)?;
        }
        for (i, payload) in self.eligibility.iter().enumerate() {
            if i > 0 {
                SdJournalAddDisjunction(&mut self.scan)?;
            }
            SdJournalAddMatch(&mut self.scan, payload)?;
        }
        if !self.eligibility.is_empty() {
            SdJournalAddConjunction(&mut self.scan)?;
        }
        self.unrestricted = unrestricted;
        Ok(())
    }

    fn bound(&mut self, id: &str) -> Result<Option<(InvocationEntry, String)>> {
        let Some(j) = self.bounds.as_mut() else {
            return Ok(None);
        };
        SdJournalFlushMatches(j)?;
        add_boot(j, &self.boot)?;
        if id.is_empty() {
            Self::add_unit_scope(j, &self.units)?;
        } else {
            add_invocation_matches(j, id)?;
        }
        SdJournalSeekHead(j)?;
        if SdJournalNext(j)? == 0 {
            return Ok(None);
        }
        let first_usec = SdJournalGetRealtimeUsec(j)?;
        let mut anchor = if self.older {
            SdJournalGetCursor(j)?
        } else {
            String::new()
        };
        SdJournalSeekTail(j)?;
        if SdJournalPrevious(j)? == 0 {
            return Ok(None);
        }
        let last_usec = SdJournalGetRealtimeUsec(j)?;
        if !self.older {
            anchor = SdJournalGetCursor(j)?;
        }
        Ok(Some((
            InvocationEntry {
                id: id.to_string(),
                first_usec,
                last_usec,
            },
            anchor,
        )))
    }

    fn step(&mut self) -> Result<bool> {
        if let Some(anchor) = self.anchor.take() {
            SdJournalSeekCursorDirection(
                &mut self.scan,
                &anchor,
                if self.older {
                    journal::Direction::Backward
                } else {
                    journal::Direction::Forward
                },
            )?;
            match SdJournalTestCursor(&self.scan, &anchor) {
                Ok(false) => return Ok(true),
                Ok(true) => {}
                Err(FacadeError::NoEntry | FacadeError::EndOfEntries) => return Ok(false),
                Err(err) => return Err(err.into()),
            }
        }
        Ok(if self.older {
            SdJournalPrevious(&mut self.scan)?
        } else {
            SdJournalNext(&mut self.scan)?
        } != 0)
    }

    fn next(&mut self) -> Result<Option<InvocationEntry>> {
        if self.done {
            return Ok(None);
        }
        if self.unrestricted {
            self.reset_scan(false)?;
        }
        let mut broken = HashSet::new();
        loop {
            if !self.step()? {
                self.done = true;
                return Ok(None);
            }
            let Some(id) = primary_id(&mut self.scan)? else {
                continue;
            };
            if self.previous.as_deref() == Some(&id) || broken.contains(&id) {
                continue;
            }
            let Some((info, anchor)) = self.bound(&id)? else {
                broken.insert(id);
                let previous = self.previous.clone().unwrap_or_default();
                let Some((_, anchor)) = self.bound(&previous)? else {
                    return Err(anyhow!(
                        "Cannot seek to the previous invocation: No data available"
                    ));
                };
                self.anchor = Some(anchor);
                // Native discovery resumes without unit/boot matches after a broken ID.
                self.reset_scan(true)?;
                continue;
            };
            self.anchor = Some(anchor);
            self.previous = Some(id);
            return Ok(Some(info));
        }
    }
}

pub(super) fn collect_invocations_from_input(
    input: &CliInput,
    args: &Args,
    unit_option: Option<&str>,
) -> Result<Vec<InvocationEntry>> {
    let (older, max) = match parse_lines_limit(args.lines.as_deref())? {
        Some(LinesLimit::Head(n)) => (false, n),
        Some(LinesLimit::Tail(n)) => (true, n),
        _ => (false, usize::MAX),
    };
    if max == 0 {
        let mut journal = input.open_journal()?;
        if let Some(option) = unit_option {
            single_unit(&mut journal, args, option)?;
        }
        boot_id(&mut journal, args)?;
        return Ok(Vec::new());
    }
    let mut walker = Walker::new(input, args, unit_option, older, false)?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while out.len() < max {
        let Some(info) = walker.next()? else {
            break;
        };
        if !seen.insert(info.id.clone()) {
            break;
        }
        out.push(info);
    }
    if older {
        out.reverse();
    }
    Ok(out)
}

pub(super) fn resolve_invocation_filter(input: &CliInput, args: &Args) -> Result<Option<String>> {
    if args.invocation.is_none() && !args.invocation_latest {
        return Ok(None);
    }
    args.resolved_invocation
        .get_or_init(|| find_filter(input, args).map_err(|e| e.to_string()))
        .as_ref()
        .cloned()
        .map_err(|e| anyhow!("{e}"))
}

fn find_filter(input: &CliInput, args: &Args) -> Result<Option<String>> {
    let Some(descriptor) = effective_invocation_descriptor(args) else {
        return Ok(None);
    };
    let Some((id, offset)) = parse_invocation_descriptor(descriptor)? else {
        return Ok(None);
    };
    let option = (id.is_empty() || offset != 0).then_some("-I/--invocation= with an offset");
    let mut walker = Walker::new(input, args, option, offset <= 0, true)?;
    let not_found = || {
        anyhow!(
            "No journal entry found for the invocation ({}{offset:+}).",
            id
        )
    };
    let mut off = if offset <= 0 { 0 } else { 1 };
    if !id.is_empty() {
        let Some((_, anchor)) = walker.bound(&id)? else {
            return Err(not_found());
        };
        if offset == 0 {
            return Ok(Some(id));
        }
        walker.anchor = Some(anchor);
        walker.previous = Some(id.clone());
        if offset < 0 {
            off = -1;
        }
    }
    loop {
        let Some(info) = walker.next()? else {
            return Err(not_found());
        };
        if off == offset {
            return Ok(Some(info.id));
        }
        off += if walker.older { -1 } else { 1 };
    }
}
