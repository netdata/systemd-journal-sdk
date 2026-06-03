use super::Entry;

pub fn export_entry_bytes(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::new();
    write_export_field(&mut out, "__CURSOR", entry.cursor.as_bytes());
    write_export_field(
        &mut out,
        "__REALTIME_TIMESTAMP",
        entry.realtime.to_string().as_bytes(),
    );
    write_export_field(
        &mut out,
        "__MONOTONIC_TIMESTAMP",
        entry.monotonic.to_string().as_bytes(),
    );
    write_export_field(&mut out, "_BOOT_ID", hex::encode(entry.boot_id).as_bytes());

    let mut keys: Vec<_> = entry.field_values.keys().collect();
    keys.sort();
    for key in keys {
        if key == "_BOOT_ID" {
            continue;
        }
        if let Some(values) = entry.field_values.get(key) {
            for value in values {
                write_export_field(&mut out, key, value);
            }
        }
    }
    let mut byte_name_fields: Vec<_> = entry
        .raw_fields()
        .filter(|field| std::str::from_utf8(field.name).is_err() && field.name != b"_BOOT_ID")
        .collect();
    byte_name_fields.sort_by(|left, right| {
        left.name
            .cmp(right.name)
            .then_with(|| left.value.cmp(right.value))
    });
    for field in byte_name_fields {
        write_export_field_bytes(&mut out, field.name, field.value);
    }
    out.push(b'\n');
    out
}

pub fn export_entry(entry: &Entry) -> String {
    String::from_utf8_lossy(&export_entry_bytes(entry)).into_owned()
}

fn write_export_field(out: &mut Vec<u8>, name: &str, value: &[u8]) {
    write_export_field_bytes(out, name.as_bytes(), value);
}

fn write_export_field_bytes(out: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    if value
        .iter()
        .all(|byte| *byte == b'\t' || (0x20..0x7f).contains(byte))
    {
        out.extend_from_slice(name);
        out.push(b'=');
        out.extend_from_slice(value);
        out.push(b'\n');
    } else {
        out.extend_from_slice(name);
        out.push(b'\n');
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value);
        out.push(b'\n');
    }
}

pub fn json_entry(entry: &Entry) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "__CURSOR".to_string(),
        serde_json::Value::String(entry.cursor.clone()),
    );
    map.insert(
        "__REALTIME_TIMESTAMP".to_string(),
        serde_json::Value::String(entry.realtime.to_string()),
    );
    map.insert(
        "__MONOTONIC_TIMESTAMP".to_string(),
        serde_json::Value::String(entry.monotonic.to_string()),
    );
    map.insert(
        "_BOOT_ID".to_string(),
        serde_json::Value::String(hex::encode(entry.boot_id)),
    );

    let mut keys: Vec<_> = entry.field_values.keys().collect();
    keys.sort();
    for key in keys {
        if key == "_BOOT_ID" {
            continue;
        }
        let values = &entry.field_values[key];
        let json_values: Vec<_> = values
            .iter()
            .map(|value| json_value_for_bytes(value))
            .collect();
        let value = if json_values.len() == 1 {
            json_values.into_iter().next().unwrap()
        } else {
            serde_json::Value::Array(json_values)
        };
        map.insert(key.clone(), value);
    }

    serde_json::Value::Object(map)
}

fn json_value_for_bytes(value: &[u8]) -> serde_json::Value {
    if json_bytes_printable(value) {
        serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
    } else {
        serde_json::Value::Array(
            value
                .iter()
                .map(|byte| serde_json::Value::Number((*byte).into()))
                .collect(),
        )
    }
}

fn json_bytes_printable(value: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(value) else {
        return false;
    };
    for ch in text.chars() {
        let cp = ch as u32;
        if cp < 0x20 && ch != '\t' && ch != '\n' {
            return false;
        }
        if (0x7f..=0x9f).contains(&cp) {
            return false;
        }
    }
    true
}

pub fn format_entry_text(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(message) = entry.get("MESSAGE") {
        out.extend_from_slice(message);
    }
    out.push(b'\n');
    out
}
