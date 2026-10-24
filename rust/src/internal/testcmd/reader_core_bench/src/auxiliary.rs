use super::*;
use journal::{IndexedSnapshot, IndexedSnapshotOptions, SnapshotControl, SnapshotEntry};

pub(super) fn read(inputs: &[PathBuf], cfg: &ReadConfig<'_>) -> Result<Counts> {
    require_single_file_input(inputs, cfg.surface, "indexed/verification modes")?;
    if cfg.direction != Direction::Forward || cfg.bounds != "snapshot" {
        return Err(anyhow!(
            "indexed/verification modes require snapshot bounds and forward direction"
        ));
    }
    let control = SnapshotControl::default();
    let mut counts = Counts::default();
    if cfg.mode.starts_with("verify-") {
        if cfg.strategy != ExperimentalMmapStrategy::Windowed
            || cfg.window_size != DEFAULT_WINDOW_SIZE
        {
            return Err(anyhow!(
                "verification modes use public SDK defaults; select windowed access and the default window size"
            ));
        }
        for _ in 0..cfg.catalog_repetitions {
            match cfg.mode {
                "verify-index" => journal::verify_index(&inputs[0], &control)?,
                "verify-file" => journal::verify_file(&inputs[0])?,
                other => return Err(anyhow!("invalid verification mode: {other}")),
            }
            counts.add_record_marker(1);
        }
        counts
            .extra
            .insert("completed_verifications".into(), json!(counts.records));
        counts
            .extra
            .insert("reader_options".into(), json!("public SDK defaults"));
        return Ok(counts);
    }
    if cfg.filter_matches.len() != 1 {
        return Err(anyhow!(
            "indexed modes require one --match FIELD=value declaration"
        ));
    }
    let declaration = &cfg.filter_matches[0];
    let separator = declaration
        .iter()
        .position(|&byte| byte == b'=')
        .filter(|&index| index > 0)
        .ok_or_else(|| anyhow!("indexed declaration must be FIELD=value"))?;
    let (name, value) = (&declaration[..separator], &declaration[separator + 1..]);
    let options = IndexedSnapshotOptions {
        reader: reader_options(cfg.bounds, cfg.strategy, cfg.window_size)?,
        capture_fields: vec![name.to_vec()],
        capture_values: vec![(name.to_vec(), value.to_vec())],
    };
    let mut captured_entries = 0;
    let mut captured = journal::CapturedValue::default();
    let digest = cfg.mode.ends_with("-digest");
    for _ in 0..cfg.catalog_repetitions {
        let mut snapshot = IndexedSnapshot::open(&inputs[0], options.clone(), &control)?;
        captured_entries = snapshot.entry_count();
        captured = snapshot.captured_value(name, value)?;
        let mut visit = |entry: &mut SnapshotEntry<'_>| {
            counts.add_record_marker(entry.metadata().realtime);
            entry.visit_payloads(|payload| {
                if digest {
                    counts.add_full_payload(black_box(payload));
                } else {
                    counts.add_payload(black_box(payload));
                }
                Ok(())
            })
        };
        match cfg.mode.strip_suffix("-digest").unwrap_or(cfg.mode) {
            "indexed-open" => counts.add_record_marker(captured_entries),
            "indexed-exact" => snapshot.visit_match(name, value, &control, &mut visit)?,
            "indexed-field" => snapshot.visit_field(
                name,
                &control,
                |candidate| Ok(candidate == value),
                &mut visit,
            )?,
            "indexed-all" => snapshot.visit_entries(&control, &mut visit)?,
            other => return Err(anyhow!("invalid indexed mode: {other}")),
        }
        drop(snapshot);
    }
    counts
        .extra
        .insert("completed_opens".into(), json!(cfg.catalog_repetitions));
    counts
        .extra
        .insert("captured_entries".into(), json!(captured_entries));
    counts
        .extra
        .insert("captured_value_entries".into(), json!(captured.entry_count));
    counts
        .extra
        .insert("captured_value_present".into(), json!(captured.present));
    Ok(counts)
}
