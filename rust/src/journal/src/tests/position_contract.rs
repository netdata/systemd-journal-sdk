//! Live position contract shared with the Go SDK (SOW-0045): next/previous
//! after one live append match stock libsystemd for file, directory and facade
//! readers, with and without an installed match.

use super::*;

trait PositionCursor {
    fn step(&mut self) -> bool;
    fn step_back(&mut self) -> bool;
    fn head(&mut self);
    fn tail(&mut self);
    fn realtime(&self) -> u64;
    fn add_match(&mut self, data: &[u8]);
}

impl PositionCursor for FileReader {
    fn step(&mut self) -> bool {
        self.next().expect("next")
    }
    fn step_back(&mut self) -> bool {
        self.previous().expect("previous")
    }
    fn head(&mut self) {
        self.seek_head()
    }
    fn tail(&mut self) {
        self.seek_tail()
    }
    fn realtime(&self) -> u64 {
        self.get_realtime_usec().expect("realtime")
    }
    fn add_match(&mut self, data: &[u8]) {
        FileReader::add_match(self, data)
    }
}

impl PositionCursor for DirectoryReader {
    fn step(&mut self) -> bool {
        self.next().expect("next")
    }
    fn step_back(&mut self) -> bool {
        self.previous().expect("previous")
    }
    fn head(&mut self) {
        self.seek_head()
    }
    fn tail(&mut self) {
        self.seek_tail()
    }
    fn realtime(&self) -> u64 {
        self.get_realtime_usec().expect("realtime")
    }
    fn add_match(&mut self, data: &[u8]) {
        DirectoryReader::add_match(self, data)
    }
}

impl PositionCursor for SdJournal {
    fn step(&mut self) -> bool {
        self.next().expect("next") > 0
    }
    fn step_back(&mut self) -> bool {
        self.previous().expect("previous") > 0
    }
    fn head(&mut self) {
        self.seek_head()
    }
    fn tail(&mut self) {
        self.seek_tail()
    }
    fn realtime(&self) -> u64 {
        self.get_realtime_usec().expect("realtime")
    }
    fn add_match(&mut self, data: &[u8]) {
        SdJournal::add_match(self, data)
    }
}

struct LiveJournal {
    file: JournalFile<MmapMut>,
    writer: JournalWriter,
}

impl LiveJournal {
    fn create(path: &Path, compact: bool, seqnum_seed: u8) -> Self {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        let repo = RepoFile::from_path(path).expect("journal path");
        let mut file = JournalFile::<MmapMut>::create(
            &repo,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(seqnum_seed))
                .with_compact(compact),
        )
        .expect("create journal");
        let writer = JournalWriter::new(&mut file, 1, test_uuid(4)).expect("writer");
        Self { file, writer }
    }

    fn append(&mut self, usec: u64, extra: Option<&[u8]>) {
        let message = format!("MESSAGE=row{usec}");
        let mut payloads = vec![message.as_bytes(), b"K=v".as_slice()];
        payloads.extend(extra);
        self.writer
            .add_entry(&mut self.file, &payloads, usec, usec)
            .expect("append");
        self.file.sync().expect("sync");
    }
}

fn result(cursor: &mut dyn PositionCursor, moved: bool) -> u64 {
    if moved { cursor.realtime() } else { 0 }
}

type Setup = fn(&mut dyn PositionCursor);

const STATES: &[(&str, u64, Setup, u64, u64)] = &[
    ("empty-open", 0, |_| {}, 1, 0),
    ("seek-head", 1, |c| c.head(), 1, 0),
    (
        "on-last-row",
        1,
        |c| {
            c.step();
        },
        2,
        0,
    ),
    (
        "forward-eof",
        1,
        |c| {
            c.step();
            c.step();
        },
        2,
        0,
    ),
    ("explicit-tail", 1, |c| c.tail(), 0, 2),
    (
        "backward-eof",
        1,
        |c| {
            c.step();
            c.step_back();
        },
        2,
        0,
    ),
];

#[test]
fn position_contract_matches_libsystemd() {
    type Open = fn(&Path) -> Box<dyn PositionCursor>;
    let openers: [(&str, Open); 3] = [
        ("file", |path| {
            Box::new(FileReader::open(path).expect("open file"))
        }),
        ("directory", |path| {
            Box::new(DirectoryReader::open(path.parent().unwrap()).expect("open directory"))
        }),
        ("facade", |path| {
            Box::new(SdJournalOpenFilePath(path, 0).expect("open facade"))
        }),
    ];
    for (reader, open) in openers {
        for compact in [false, true] {
            for filtered in [false, true] {
                for &(state, initial, setup, next, previous) in STATES {
                    for backward in [false, true] {
                        let dir = tempfile::tempdir().expect("temp dir");
                        let path = dir.path().join("journal/system.journal");
                        let mut live = LiveJournal::create(&path, compact, 3);
                        for usec in 1..=initial {
                            live.append(usec, None);
                        }
                        let mut cursor = open(&path);
                        if filtered {
                            cursor.add_match(b"K=v");
                        }
                        setup(cursor.as_mut());
                        live.append(initial + 1, None);
                        let (got, want) = if backward {
                            let moved = cursor.step_back();
                            (result(cursor.as_mut(), moved), previous)
                        } else {
                            let moved = cursor.step();
                            (result(cursor.as_mut(), moved), next)
                        };
                        assert_eq!(
                            got, want,
                            "{reader} compact={compact} filtered={filtered} {state} backward={backward}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn absent_match_observes_append() {
    for compact in [false, true] {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("journal/system.journal");
        let mut live = LiveJournal::create(&path, compact, 3);
        live.append(1, None);
        let mut reader = FileReader::open(&path).expect("open");
        reader.add_match(b"TARGET=new");
        assert!(!reader.next().expect("absent"));
        live.append(2, Some(b"TARGET=new"));
        assert!(reader.next().expect("appended"), "compact={compact}");
        assert_eq!(reader.get_realtime_usec().expect("realtime"), 2);
    }
}

#[test]
fn cursor_seek_observes_append() {
    for filtered in [false, true] {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("journal/system.journal");
        let mut live = LiveJournal::create(&path, false, 3);
        live.append(1, None);
        let mut first = FileReader::open(&path).expect("open");
        assert!(first.next().expect("first row"));
        let cursor = first.get_cursor().expect("cursor");
        let mut reader = FileReader::open(&path).expect("open");
        if filtered {
            reader.add_match(b"TARGET=new");
            assert!(!reader.next().expect("absent"));
        } else {
            assert!(reader.next().expect("first row") && !reader.next().expect("eof"));
        }
        live.append(2, Some(b"TARGET=new"));
        reader
            .seek_cursor_direction(&cursor, Direction::Forward)
            .expect("seek");
        assert_eq!(
            reader.get_realtime_usec().expect("appended row"),
            if filtered { 2 } else { 1 },
            "filtered={filtered}"
        );
        if !filtered {
            assert!(reader.next().expect("next") && reader.get_realtime_usec().unwrap() == 2);
        }
    }
}

#[test]
fn getters_keep_last_entry_after_failed_move() {
    for filtered in [false, true] {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("journal/system.journal");
        let mut live = LiveJournal::create(&path, false, 3);
        live.append(1, None);
        live.append(2, None);
        let mut reader = FileReader::open(&path).expect("open");
        if filtered {
            reader.add_match(b"K=v");
        }
        assert!(reader.next().unwrap() && reader.next().unwrap());
        assert!(!reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().expect("after EOF"), 2);
        reader.seek_tail();
        assert!(reader.get_realtime_usec().is_err(), "tail has no entry");
    }
}

#[test]
fn merged_directory_boundaries_and_eof() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut first = LiveJournal::create(&dir.path().join("journal/a.journal"), false, 3);
    let mut second = LiveJournal::create(&dir.path().join("journal/b.journal"), false, 5);
    first.append(1, None);
    second.append(2, None);
    first.append(3, None);
    let mut reader = DirectoryReader::open(dir.path().join("journal")).expect("open");
    let c: &mut dyn PositionCursor = &mut reader;
    let moved = c.step_back();
    assert_eq!(result(c, moved), 0, "previous at open");
    for want in [1, 2, 3] {
        let moved = c.step();
        assert_eq!(result(c, moved), want, "forward");
    }
    assert!(!c.step(), "forward EOF");
    assert_eq!(c.realtime(), 3, "getter after forward EOF keeps row 3");
    let moved = c.step_back();
    assert_eq!(result(c, moved), 2, "reverse after EOF");
    let moved = c.step();
    assert_eq!(result(c, moved), 3, "forward again");
    assert!(!c.step(), "forward EOF again");
    second.append(4, None);
    let moved = c.step();
    assert_eq!(result(c, moved), 4, "next after append");
    let moved = c.step_back();
    assert_eq!(result(c, moved), 3, "reverse");
    c.tail();
    assert!(!c.step(), "next at tail");
    let moved = c.step_back();
    assert_eq!(result(c, moved), 4, "previous at tail");
    for want in [3, 2, 1] {
        let moved = c.step_back();
        assert_eq!(result(c, moved), want, "backward");
    }
    assert!(!c.step_back(), "backward EOF");
    assert_eq!(c.realtime(), 1, "getter after backward EOF keeps row 1");
    c.head();
    assert!(!c.step_back(), "previous at head");
    let moved = c.step();
    assert_eq!(result(c, moved), 1, "next at head");
}

#[test]
fn directory_seek_realtime_clears_current_entry() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("journal/system.journal");
    let mut live = LiveJournal::create(&path, false, 3);
    live.append(1, None);
    live.append(2, None);
    let mut reader = DirectoryReader::open(path.parent().unwrap()).expect("open");
    assert!(reader.next().unwrap());
    reader.seek_realtime(2);
    assert!(reader.get_realtime_usec().is_err());
    assert!(reader.next().unwrap());
    assert_eq!(reader.get_realtime_usec().unwrap(), 2);
}

#[test]
fn directory_getters_keep_last_entry_after_failed_move() {
    for filtered in [false, true] {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("journal/system.journal");
        let mut live = LiveJournal::create(&path, false, 3);
        live.append(1, None);
        live.append(2, None);
        let mut reader = DirectoryReader::open(path.parent().unwrap()).expect("open");
        if filtered {
            reader.add_match(b"K=v");
        }
        assert!(reader.next().unwrap() && reader.next().unwrap());
        assert!(!reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().expect("after EOF"), 2);
        reader.get_cursor().expect("cursor after EOF");
        assert!(reader.previous().unwrap(), "reverse");
        assert_eq!(reader.get_realtime_usec().unwrap(), 1);
        assert!(!reader.previous().unwrap());
        assert_eq!(reader.get_realtime_usec().expect("after start"), 1);
        reader.seek_tail();
        assert!(reader.get_realtime_usec().is_err(), "tail has no entry");
    }
}
