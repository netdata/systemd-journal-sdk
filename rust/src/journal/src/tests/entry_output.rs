use std::collections::HashMap;

use super::repo_root;
use crate::{
    Entry, EntryEncoder, EntryOutputOptions, FileReader, export_entry_bytes, is_printable_text,
    json_entry,
};
use serde_json::Value;

// The goldens under tests/output were produced by systemd v262 journalctl from
// entry-output.journal (see tests/output/README.md).
const FIELDS: [&str; 7] = [
    "MESSAGE", "MULTI", "BIN", "__CURSOR", "_BOOT_ID", "AFTER", "LAST",
];

fn output_entries() -> Vec<Entry> {
    let path = repo_root().join("tests/output/entry-output.journal.zst");
    let mut reader = FileReader::open(&path).expect("open entry output fixture");
    let mut entries = Vec::new();
    while reader.next().expect("next entry") {
        entries.push(reader.get_entry().expect("get entry"));
    }
    entries
}

fn golden(name: &str) -> Vec<u8> {
    std::fs::read(repo_root().join("tests/output").join(name)).expect("read golden")
}

fn decode_json_stream(data: &[u8]) -> Vec<Value> {
    serde_json::Deserializer::from_slice(data)
        .into_iter::<Value>()
        .map(|value| value.expect("decode JSON"))
        .collect()
}

fn field_options() -> EntryOutputOptions {
    EntryOutputOptions {
        fields: Some(FIELDS.iter().map(|name| name.to_string()).collect()),
        show_all: false,
    }
}

#[test]
fn export_matches_systemd() {
    let entries = output_entries();
    for (name, options) in [
        ("export.golden", EntryOutputOptions::default()),
        ("export-fields.golden", field_options()),
    ] {
        let mut encoder = EntryEncoder::new(options);
        let mut out = Vec::new();
        for entry in &entries {
            encoder.append_export(&mut out, entry);
        }
        assert!(out == golden(name), "{name}: export differs from systemd");
    }
}

#[test]
fn json_matches_systemd() {
    let entries = output_entries();
    let show_all = EntryOutputOptions {
        fields: None,
        show_all: true,
    };
    for (name, options, pretty) in [
        ("json.golden", EntryOutputOptions::default(), false),
        ("json-all.golden", show_all, false),
        ("json-fields.golden", field_options(), false),
        ("json-pretty.golden", EntryOutputOptions::default(), true),
    ] {
        let mut encoder = EntryEncoder::new(options);
        let mut out = Vec::new();
        for entry in &entries {
            if pretty {
                encoder.append_json_pretty(&mut out, entry);
            } else {
                encoder.append_json(&mut out, entry);
            }
        }
        // systemd orders keys by hash table iteration, so compare values.
        assert_eq!(
            decode_json_stream(&out),
            decode_json_stream(&golden(name)),
            "{name}: JSON differs from systemd"
        );
        if !pretty {
            assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), entries.len());
        }
    }
}

fn synthetic_entry(payloads: &[&[u8]]) -> Entry {
    Entry {
        fields: HashMap::new(),
        field_values: HashMap::new(),
        payloads: payloads.iter().map(|payload| payload.to_vec()).collect(),
        seqnum: 7,
        realtime: 1234,
        monotonic: 5678,
        boot_id: [0x22; 16],
        cursor: "s=33333333333333333333333333333333;i=7;b=22222222222222222222222222222222;m=162e;t=4d2;x=1"
            .to_string(),
    }
}

fn sample_payloads() -> [&'static [u8]; 4] {
    [
        b"MESSAGE=say \"hi\"\t\\ <b>\n",
        b"BIN=\x00\x1f",
        b"MULTI=one",
        b"MULTI=two",
    ]
}

#[test]
fn json_bytes_match_systemd_layout() {
    let entry = synthetic_entry(&sample_payloads());
    let mut encoder = EntryEncoder::new(EntryOutputOptions::default());
    let mut out = Vec::new();
    encoder.append_json(&mut out, &entry);
    let want = concat!(
        r#"{"BIN":[0,31],"MESSAGE":"say \"hi\"\t\\ <b>\n","MULTI":["one","two"],"#,
        r#""_BOOT_ID":"22222222222222222222222222222222","#,
        r#""__CURSOR":"s=33333333333333333333333333333333;i=7;b=22222222222222222222222222222222;m=162e;t=4d2;x=1","#,
        r#""__MONOTONIC_TIMESTAMP":"5678","__REALTIME_TIMESTAMP":"1234","__SEQNUM":"7","#,
        r#""__SEQNUM_ID":"33333333333333333333333333333333"}"#,
        "\n"
    );
    assert_eq!(String::from_utf8(out).unwrap(), want);

    let mut out = Vec::new();
    encoder.append_json_pretty(&mut out, &entry);
    let want = concat!(
        "{\n",
        "\t\"BIN\" : [\n\t\t0,\n\t\t31\n\t],\n",
        "\t\"MESSAGE\" : \"say \\\"hi\\\"\\t\\\\ <b>\\n\",\n",
        "\t\"MULTI\" : [\n\t\t\"one\",\n\t\t\"two\"\n\t],\n",
        "\t\"_BOOT_ID\" : \"22222222222222222222222222222222\",\n",
        "\t\"__CURSOR\" : \"s=33333333333333333333333333333333;i=7;b=22222222222222222222222222222222;m=162e;t=4d2;x=1\",\n",
        "\t\"__MONOTONIC_TIMESTAMP\" : \"5678\",\n",
        "\t\"__REALTIME_TIMESTAMP\" : \"1234\",\n",
        "\t\"__SEQNUM\" : \"7\",\n",
        "\t\"__SEQNUM_ID\" : \"33333333333333333333333333333333\"\n",
        "}\n"
    );
    assert_eq!(String::from_utf8(out).unwrap(), want);
}

#[test]
fn export_stops_at_payload_without_separator() {
    let entry = synthetic_entry(&[b"FIRST=1", b"NOSEPARATOR", b"AFTER=2"]);
    let mut out = Vec::new();
    EntryEncoder::new(EntryOutputOptions::default()).append_export(&mut out, &entry);
    assert!(out.ends_with(b"_BOOT_ID=22222222222222222222222222222222\nFIRST=1\n\n"));
    // JSON skips the bad payload and continues.
    let mut json = Vec::new();
    EntryEncoder::new(EntryOutputOptions::default()).append_json(&mut json, &entry);
    assert!(String::from_utf8(json).unwrap().contains(r#""AFTER":"2""#));
}

#[test]
fn map_fallback_and_missing_cursor() {
    let mut entry = synthetic_entry(&[]);
    entry.seqnum = 0;
    entry.realtime = 0;
    entry.monotonic = 0;
    entry.boot_id = [0; 16];
    entry.cursor.clear();
    entry.field_values.insert("B".into(), vec![b"2".to_vec()]);
    entry.field_values.insert("A".into(), vec![b"1".to_vec()]);
    entry.fields.insert("C".into(), b"3".to_vec());
    let mut out = Vec::new();
    EntryEncoder::new(EntryOutputOptions::default()).append_export(&mut out, &entry);
    let want = concat!(
        "__CURSOR=\n__REALTIME_TIMESTAMP=0\n__MONOTONIC_TIMESTAMP=0\n__SEQNUM=0\n",
        "__SEQNUM_ID=00000000000000000000000000000000\n_BOOT_ID=00000000000000000000000000000000\n",
        "A=1\nB=2\nC=3\n\n"
    );
    assert_eq!(String::from_utf8(out).unwrap(), want);
}

// export_entry_bytes keeps the SOW-0043 contract: fields with invalid or
// non-UTF-8 names stay in the output instead of ending the entry.
#[test]
fn export_entry_bytes_keeps_invalid_field_names() {
    let entry = synthetic_entry(&[
        b"FIRST=1",
        b"lower=x",
        b"\xffRAW=invalid utf8",
        b"NOSEPARATOR",
        b"AFTER=2",
    ]);
    let want: &[u8] = b"_BOOT_ID=22222222222222222222222222222222\nFIRST=1\nlower=x\n\xffRAW\n\x0c\x00\x00\x00\x00\x00\x00\x00invalid utf8\nAFTER=2\n\n";
    assert!(export_entry_bytes(&entry).ends_with(want));
    let mut strict = Vec::new();
    EntryEncoder::new(EntryOutputOptions::default()).append_export(&mut strict, &entry);
    assert!(strict.ends_with(b"FIRST=1\n\n"));
}

#[test]
fn json_entry_keeps_full_values() {
    let big = [b"BIG=".as_slice(), &[b'x'; 5000]].concat();
    let mut payloads = sample_payloads().to_vec();
    payloads.push(&big);
    let Value::Object(object) = json_entry(&synthetic_entry(&payloads)) else {
        panic!("entry JSON should be an object");
    };
    assert_eq!(object["BIG"].as_str().map(str::len), Some(5000));
    assert_eq!(object["BIN"], serde_json::json!([0, 31]));
    assert_eq!(object["MULTI"], serde_json::json!(["one", "two"]));
}

#[test]
fn printable_text_matches_systemd() {
    let cases: [(&[u8], bool, bool); 19] = [
        (b"plain text", true, true),
        (b"tab\there", true, true),
        (b"line\nbreak", true, false),
        (b"", true, true),
        ("h\u{e9}llo \u{2603} \u{1F600}".as_bytes(), true, true),
        ("\u{fffd}".as_bytes(), true, true),
        (b"\x00", false, false),
        (b"\r", false, false),
        (b"\x7f", false, false),
        ("\u{85}".as_bytes(), false, false),
        ("\u{fdd0}".as_bytes(), false, false),
        ("\u{fdef}".as_bytes(), false, false),
        (b"\xef\xbf\xbe", false, false),
        (b"\xef\xbf\xbf", false, false),
        (b"\xf0\x9f\xbf\xbe", false, false),
        (b"\xed\xa0\x80", false, false),
        (b"\xc0\x80", false, false),
        (b"\xff", false, false),
        (b"\xe2\x98", false, false),
    ];
    for (value, with_newline, without_newline) in cases {
        assert_eq!(
            is_printable_text(value, true),
            with_newline,
            "{value:?} with newline"
        );
        assert_eq!(
            is_printable_text(value, false),
            without_newline,
            "{value:?} without newline"
        );
    }
}
