use anyhow::{Result, anyhow};
use journal::{DirectoryReader, FileReader, ReaderOptions};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Request {
    path: String,
    operation: String,
    #[serde(default)]
    field: String,
    #[serde(default)]
    match_hex: String,
    #[serde(default)]
    realtime: u64,
    #[serde(default)]
    snapshot: bool,
    #[serde(default)]
    directory: bool,
}

enum Reader {
    File(FileReader),
    Directory(DirectoryReader),
}

macro_rules! call {
    ($reader:expr, $method:ident $(, $argument:expr)*) => {
        match $reader {
            Reader::File(reader) => reader.$method($($argument),*),
            Reader::Directory(reader) => reader.$method($($argument),*),
        }
    };
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

fn decode_hex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return Err(anyhow!("invalid hex input"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char)
                .to_digit(16)
                .ok_or_else(|| anyhow!("invalid hex input"))?;
            let low = (pair[1] as char)
                .to_digit(16)
                .ok_or_else(|| anyhow!("invalid hex input"))?;
            Ok((high * 16 + low) as u8)
        })
        .collect()
}

// Public SDK probe used by shared damaged-file subprocess tests.
pub(super) fn run() -> Result<()> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut result =
        json!({"rows":[],"fields":[],"unique":[],"recovery_statuses":[],"error":false});
    if let Err(error) = inspect(&request, &mut result) {
        result["error"] = Value::Bool(true);
        result["error_message"] = Value::String(error.to_string());
    }
    serde_json::to_writer(std::io::stdout(), &result)?;
    println!();
    Ok(())
}

fn inspect(request: &Request, result: &mut Value) -> Result<()> {
    if request.operation == "verify" {
        return journal::verify_file(&request.path).map_err(Into::into);
    }
    if request.operation.starts_with("facade-") {
        return inspect_facade(request, result);
    }
    let options = if request.snapshot {
        ReaderOptions::snapshot()
    } else {
        ReaderOptions::default()
    };
    let mut reader = if request.directory {
        Reader::Directory(DirectoryReader::open_with_options(&request.path, options)?)
    } else {
        Reader::File(FileReader::open_with_options(&request.path, options)?)
    };
    let inspected = inspect_open(&mut reader, request, result);
    result["recovery_statuses"] = Value::Array(
        call!(&reader, recovery_statuses)
            .iter()
            .map(|file| {
                let s = file.status;
                json!({"file_id":hex(&file.file_id),"status":{
            "reason":s.reason as u8,"first_lost_offset":s.first_lost_offset,
            "declared_entries":s.declared_entries,"recovered_entries":s.recovered_entries}})
            })
            .collect(),
    );
    inspected
}

fn inspect_facade(request: &Request, result: &mut Value) -> Result<()> {
    let options = if request.snapshot {
        ReaderOptions::snapshot()
    } else {
        ReaderOptions::default()
    };
    let mut reader = if request.directory {
        journal::SdJournalOpenDirectoryWithOptions(&request.path, 0, options)?
    } else {
        journal::SdJournalOpenFileWithOptions(&request.path, 0, options)?
    };
    let inspected = (|| -> Result<()> {
        if !request.match_hex.is_empty() {
            journal::SdJournalAddMatch(&mut reader, &decode_hex(&request.match_hex)?)?;
        }
        let operation = request.operation.strip_prefix("facade-").unwrap();
        match operation {
            "fields" => {
                result["fields"] = json!(journal::SdJournalEnumerateFields(&mut reader)?);
                return Ok(());
            }
            "unique" => {
                for (_, value) in journal::SdJournalQueryUnique(&mut reader, &request.field)? {
                    result["unique"]
                        .as_array_mut()
                        .unwrap()
                        .push(Value::String(hex(&value)));
                }
                return Ok(());
            }
            "unique-state" => {
                journal::SdJournalQueryUniqueState(&mut reader, &request.field)?;
                while let Some(value) = journal::SdJournalEnumerateAvailableUnique(&mut reader)? {
                    result["unique"]
                        .as_array_mut()
                        .unwrap()
                        .push(Value::String(hex(&value)));
                }
                return Ok(());
            }
            "flush" => {
                for _ in 0..2 {
                    if journal::SdJournalNext(&mut reader)? != 1 {
                        return Err(anyhow!("missing pre-flush match"));
                    }
                }
                journal::SdJournalFlushMatches(&mut reader)?;
            }
            "forward" => (),
            "reverse" => journal::SdJournalSeekTail(&mut reader)?,
            _ => return Err(anyhow!("unknown facade probe operation")),
        }
        loop {
            let count = if operation == "reverse" {
                journal::SdJournalPrevious(&mut reader)?
            } else {
                journal::SdJournalNext(&mut reader)?
            };
            if count == 0 {
                return Ok(());
            }
            let entry = journal::SdJournalGetEntry(&mut reader)?;
            let mut payloads = entry
                .payloads
                .iter()
                .map(|value| hex(value))
                .collect::<Vec<_>>();
            payloads.sort();
            result["rows"]
                .as_array_mut()
                .unwrap()
                .push(json!({"realtime":entry.realtime,"payloads":payloads}));
        }
    })();
    result["recovery_statuses"] = Value::Array(journal::SdJournalRecoveryStatuses(&reader).iter().map(|file| {
        let s = file.status;
        json!({"file_id":hex(&file.file_id),"status":{"reason":s.reason as u8,"first_lost_offset":s.first_lost_offset,"declared_entries":s.declared_entries,"recovered_entries":s.recovered_entries}})
    }).collect());
    inspected
}

fn inspect_open(reader: &mut Reader, request: &Request, result: &mut Value) -> Result<()> {
    if !request.match_hex.is_empty() {
        call!(&mut *reader, add_match, &decode_hex(&request.match_hex)?);
    }
    match request.operation.as_str() {
        "explorer" | "explorer-filter" | "explorer-facet" | "explorer-histogram" => {
            let Reader::File(file) = reader else {
                return Err(anyhow!("explorer probe needs one file"));
            };
            let mut query = journal::ExplorerQuery {
                filters: vec![journal::ExplorerFilter::new(
                    request.field.as_bytes(),
                    [b"absent".as_slice()],
                )],
                field_mode: journal::ExplorerFieldMode::AllValues,
                use_source_realtime: false,
                ..Default::default()
            };
            if request.operation != "explorer" {
                query.filters.clear();
                query.limit = 0;
                match request.operation.as_str() {
                    "explorer-filter" => {
                        query.limit = 1;
                        query.filters.push(journal::ExplorerFilter::new(
                            request.field.as_bytes(),
                            [b"all".as_slice()],
                        ));
                    }
                    "explorer-facet" => query.facets.push(request.field.as_bytes().to_vec()),
                    _ => query.histogram = Some(request.field.as_bytes().to_vec()),
                }
            }
            file.explore_with_strategy(&query, journal::ExplorerStrategy::Index)?;
            return Ok(());
        }
        "fields" => {
            result["fields"] = json!(call!(reader, enumerate_fields)?);
            return Ok(());
        }
        "unique" => {
            call!(
                reader,
                visit_unique_values,
                &request.field,
                |value: &[u8]| {
                    result["unique"]
                        .as_array_mut()
                        .unwrap()
                        .push(Value::String(hex(value)));
                    Ok(())
                }
            )?;
            return Ok(());
        }
        "unique-state" => {
            call!(&mut *reader, query_unique_state, &request.field)?;
            loop {
                let value = match &mut *reader {
                    Reader::File(file) => file.enumerate_unique_payload(&request.field)?,
                    Reader::Directory(directory) => directory.enumerate_unique_payload()?,
                };
                let Some(value) = value else {
                    return Ok(());
                };
                result["unique"]
                    .as_array_mut()
                    .unwrap()
                    .push(Value::String(hex(&value)));
            }
        }
        "flush" => {
            for _ in 0..2 {
                if !call!(&mut *reader, next)? {
                    return Err(anyhow!("missing pre-flush match"));
                }
            }
            call!(&mut *reader, flush_matches);
        }
        "reverse" => call!(&mut *reader, seek_tail),
        "seek-forward" | "seek-reverse" => call!(&mut *reader, seek_realtime, request.realtime),
        "forward" => (),
        _ => return Err(anyhow!("unsupported probe operation {}", request.operation)),
    }
    loop {
        let found = if request.operation == "reverse" || request.operation == "seek-reverse" {
            call!(&mut *reader, previous)?
        } else {
            call!(&mut *reader, next)?
        };
        if !found {
            return Ok(());
        }
        let realtime = call!(&mut *reader, get_realtime_usec)?;
        let mut payloads = Vec::new();
        call!(&mut *reader, collect_entry_payloads, &mut payloads)?;
        let mut payloads: Vec<_> = payloads.iter().map(|payload| hex(payload)).collect();
        payloads.sort();
        result["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"realtime":realtime,"payloads":payloads}));
    }
}
