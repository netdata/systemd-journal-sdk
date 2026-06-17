use super::*;

pub(super) fn log_writer_field_name_policy(policy: FieldNamePolicy) -> FieldNamePolicy {
    if policy == FieldNamePolicy::Raw {
        FieldNamePolicy::Raw
    } else {
        FieldNamePolicy::Journald
    }
}

pub(super) fn filter_raw_items_for_journal_app<'a>(items: &'a [&'a [u8]]) -> Result<Vec<&'a [u8]>> {
    let mut filtered = Vec::with_capacity(items.len());
    for item in items.iter().copied() {
        let Some(pos) = item.iter().position(|&b| b == b'=') else {
            return Err(JournalError::InvalidField.into());
        };
        if pos == 0 {
            return Err(JournalError::InvalidField.into());
        }
        if is_journal_app_field_name(&item[..pos]) {
            filtered.push(item);
        }
    }
    Ok(filtered)
}

pub(super) fn filter_structured_fields_for_journal_app<'a>(
    fields: &'a [StructuredField<'a>],
) -> Vec<StructuredField<'a>> {
    fields
        .iter()
        .copied()
        .filter(|field| is_journal_app_field_name(field.name))
        .collect()
}

pub(super) fn is_journal_app_field_name(field_name: &[u8]) -> bool {
    if field_name.is_empty() || field_name.len() > 64 {
        return false;
    }
    if field_name[0] == b'_' || field_name[0].is_ascii_digit() {
        return false;
    }
    field_name
        .iter()
        .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

#[cfg(feature = "serde-api")]
// Behavior mirrors Meilisearch's MIT-licensed flatten-serde-json v1.22.1
// flattener; provenance is recorded in the repository provenance file.
pub(super) fn flatten_json_map(
    json: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    fn insert_value(
        out: &mut serde_json::Map<String, serde_json::Value>,
        key: &str,
        value: serde_json::Value,
        came_from_array: bool,
    ) {
        debug_assert!(!value.is_object());
        debug_assert!(!value.is_array());

        if let Some(existing) = out.get_mut(key) {
            if let Some(array) = existing.as_array_mut() {
                array.push(value);
            } else {
                let previous = std::mem::take(existing);
                *existing = serde_json::Value::Array(vec![previous, value]);
            }
        } else if came_from_array {
            out.insert(key.to_string(), serde_json::Value::Array(vec![value]));
        } else {
            out.insert(key.to_string(), value);
        }
    }

    fn insert_array(
        out: &mut serde_json::Map<String, serde_json::Value>,
        key: &str,
        array: &[serde_json::Value],
        originals: &mut Vec<(String, serde_json::Value)>,
    ) {
        for value in array {
            match value {
                serde_json::Value::Object(object) => {
                    insert_object(out, Some(key), object, originals)
                }
                serde_json::Value::Array(array) => insert_array(out, key, array, originals),
                value => insert_value(out, key, value.clone(), true),
            }
        }
    }

    fn insert_object(
        out: &mut serde_json::Map<String, serde_json::Value>,
        base_key: Option<&str>,
        object: &serde_json::Map<String, serde_json::Value>,
        originals: &mut Vec<(String, serde_json::Value)>,
    ) {
        for (key, value) in object {
            let next_key = base_key.map_or_else(|| key.clone(), |base| format!("{base}.{key}"));
            originals.push((next_key.clone(), value.clone()));
            match value {
                serde_json::Value::Object(object) => {
                    insert_object(out, Some(&next_key), object, originals)
                }
                serde_json::Value::Array(array) => insert_array(out, &next_key, array, originals),
                value => insert_value(out, &next_key, value.clone(), false),
            }
        }
    }

    let mut out = serde_json::Map::new();
    let mut originals = Vec::new();
    insert_object(&mut out, None, json, &mut originals);
    for (key, value) in originals {
        out.entry(key).or_insert(value);
    }
    out
}

pub(super) fn create_chain(
    path: &Path,
    source: journal_registry::Source,
    machine_id: uuid::Uuid,
) -> Result<OwnedChain> {
    if path.exists() && !path.is_dir() {
        return Err(WriterError::NotADirectory(path.display().to_string()));
    }

    if path.to_str().is_none() {
        return Err(WriterError::InvalidPath(
            "path contains invalid UTF-8".to_string(),
        ));
    }

    let path = PathBuf::from(path).join(machine_id.as_simple().to_string());
    if path.to_str().is_none() {
        return Err(WriterError::InvalidPath(
            "path with machine ID contains invalid UTF-8".to_string(),
        ));
    }

    std::fs::create_dir_all(&path)?;

    path.canonicalize()
        .map_err(|e| WriterError::NotADirectory(format!("failed to canonicalize path: {}", e)))?;
    if path.to_str().is_none() {
        return Err(WriterError::InvalidPath(
            "canonicalized path contains invalid UTF-8".to_string(),
        ));
    }

    OwnedChain::new(path, machine_id, source)
}

pub(super) fn resolve_machine_id(config: &Config) -> Result<uuid::Uuid> {
    #[allow(deprecated)]
    match config.identity_mode {
        LogIdentityMode::Strict => match config.origin.machine_id {
            Some(machine_id) if !machine_id.is_nil() => Ok(machine_id),
            _ => Err(WriterError::MachineId("machine id is required".to_string())),
        },
        LogIdentityMode::Auto => Err(WriterError::InvalidConfig(
            "LogIdentityMode::Auto is no longer supported; supply explicit machine id and boot id"
                .to_string(),
        )),
    }
}

pub(super) fn resolve_boot_id(config: &Config) -> Result<uuid::Uuid> {
    #[allow(deprecated)]
    match config.identity_mode {
        LogIdentityMode::Strict => match config.boot_id {
            Some(boot_id) if !boot_id.is_nil() => Ok(boot_id),
            _ => Err(WriterError::MachineId("boot id is required".to_string())),
        },
        LogIdentityMode::Auto => Err(WriterError::InvalidConfig(
            "LogIdentityMode::Auto is no longer supported; supply explicit machine id and boot id"
                .to_string(),
        )),
    }
}

pub(super) fn validate_config(config: &Config) -> Result<()> {
    #[allow(deprecated)]
    if config.identity_mode == LogIdentityMode::Auto {
        return Err(WriterError::InvalidConfig(
            "LogIdentityMode::Auto is no longer supported; supply explicit machine id and boot id"
                .to_string(),
        ));
    }
    if config.rotation_policy.size_of_journal_file == Some(0) {
        return Err(WriterError::InvalidConfig(
            "rotation max file size must be greater than 0".to_string(),
        ));
    }
    if config.rotation_policy.number_of_entries == Some(0) {
        return Err(WriterError::InvalidConfig(
            "rotation max entries must be greater than 0".to_string(),
        ));
    }
    if config
        .rotation_policy
        .duration_of_journal_file
        .is_some_and(|duration| duration.is_zero())
    {
        return Err(WriterError::InvalidConfig(
            "rotation max duration must be greater than 0".to_string(),
        ));
    }
    if config.retention_policy.number_of_journal_files == Some(0) {
        return Err(WriterError::InvalidConfig(
            "retention max files must be greater than 0".to_string(),
        ));
    }
    if config.retention_policy.size_of_journal_files == Some(0) {
        return Err(WriterError::InvalidConfig(
            "retention max bytes must be greater than 0".to_string(),
        ));
    }
    if config
        .retention_policy
        .duration_of_journal_files
        .is_some_and(|duration| duration.is_zero())
    {
        return Err(WriterError::InvalidConfig(
            "retention max age must be greater than 0".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn align_to(value: u64, alignment: u64) -> u64 {
    value.saturating_add(alignment.saturating_sub(1)) & !(alignment.saturating_sub(1))
}

pub(super) fn normalize_derived_max_file_size(size: u64, compact: bool) -> u64 {
    let mut size = align_to(size.max(1), PAGE_SIZE);
    if compact && size > JOURNAL_COMPACT_SIZE_MAX {
        size = JOURNAL_COMPACT_SIZE_MAX;
    }
    size.max(JOURNAL_FILE_SIZE_MIN)
}

pub(super) fn derive_rotation_policy(config: &Config) -> RotationPolicy {
    let mut rotation = config.rotation_policy;
    if rotation.size_of_journal_file.is_none()
        && let Some(retention_size) = config.retention_policy.size_of_journal_files
    {
        rotation.size_of_journal_file = Some(normalize_derived_max_file_size(
            retention_size / DERIVED_ROTATION_FRACTION,
            config.compact,
        ));
    }
    if rotation.duration_of_journal_file.is_none()
        && let Some(retention_duration) = config.retention_policy.duration_of_journal_files
    {
        let fraction = u128::from(DERIVED_ROTATION_FRACTION);
        let micros = retention_duration
            .as_micros()
            .saturating_add(fraction.saturating_sub(1))
            / fraction;
        let micros = micros.max(1).min(u128::from(u64::MAX)) as u64;
        rotation.duration_of_journal_file = Some(std::time::Duration::from_micros(micros));
    }
    rotation
}
