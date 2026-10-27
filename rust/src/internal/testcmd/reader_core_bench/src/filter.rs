use super::{
    Counts, ReadConfig, borrowed_input_paths, open_facade_reader, reader_options, utf8_input_paths,
};
use anyhow::{Result, anyhow};
use journal::{Direction, DirectoryReader, FileReader, SdJournal};
use std::hint::black_box;
use std::path::PathBuf;

pub(super) fn decode_matches(text: &[String], hex: &[String]) -> Result<Vec<Vec<u8>>> {
    let mut matches: Vec<Vec<u8>> = text.iter().map(|value| value.as_bytes().to_vec()).collect();
    for value in hex {
        if !value.is_ascii() || value.len() % 2 != 0 {
            return Err(anyhow!("invalid --match-hex"));
        }
        let bytes = (0..value.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&value[at..at + 2], 16))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        matches.push(bytes);
    }
    Ok(matches)
}

trait FilterReader {
    fn add(&mut self, payload: &[u8]);
    fn clear(&mut self);
    fn position(&mut self, cfg: &ReadConfig<'_>);
    fn step(&mut self, direction: Direction) -> Result<bool>;
    fn realtime(&self) -> Result<u64>;
    fn payloads(&mut self, visitor: &mut dyn FnMut(&[u8]) -> journal::Result<()>) -> Result<()>;
}

impl FilterReader for FileReader {
    fn add(&mut self, payload: &[u8]) {
        self.add_match(payload);
    }
    fn clear(&mut self) {
        self.flush_matches();
    }
    fn position(&mut self, cfg: &ReadConfig<'_>) {
        if let Some(time) = cfg.filter_realtime {
            self.seek_realtime(time);
        } else if cfg.direction == Direction::Forward {
            self.seek_head();
        } else {
            self.seek_tail();
        }
    }
    fn step(&mut self, direction: Direction) -> Result<bool> {
        Ok(if direction == Direction::Forward {
            self.next()?
        } else {
            self.previous()?
        })
    }
    fn realtime(&self) -> Result<u64> {
        Ok(self.get_realtime_usec()?)
    }
    fn payloads(&mut self, visitor: &mut dyn FnMut(&[u8]) -> journal::Result<()>) -> Result<()> {
        Ok(self.visit_entry_payloads(visitor)?)
    }
}

impl FilterReader for DirectoryReader {
    fn add(&mut self, payload: &[u8]) {
        self.add_match(payload);
    }
    fn clear(&mut self) {
        self.flush_matches();
    }
    fn position(&mut self, cfg: &ReadConfig<'_>) {
        if let Some(time) = cfg.filter_realtime {
            self.seek_realtime(time);
        } else if cfg.direction == Direction::Forward {
            self.seek_head();
        } else {
            self.seek_tail();
        }
    }
    fn step(&mut self, direction: Direction) -> Result<bool> {
        Ok(if direction == Direction::Forward {
            self.next()?
        } else {
            self.previous()?
        })
    }
    fn realtime(&self) -> Result<u64> {
        Ok(self.get_realtime_usec()?)
    }
    fn payloads(&mut self, visitor: &mut dyn FnMut(&[u8]) -> journal::Result<()>) -> Result<()> {
        Ok(self.visit_entry_payloads(visitor)?)
    }
}

impl FilterReader for SdJournal {
    fn add(&mut self, payload: &[u8]) {
        self.add_match(payload);
    }
    fn clear(&mut self) {
        self.flush_matches();
    }
    fn position(&mut self, cfg: &ReadConfig<'_>) {
        if let Some(time) = cfg.filter_realtime {
            self.seek_realtime_usec(time);
        } else if cfg.direction == Direction::Forward {
            self.seek_head();
        } else {
            self.seek_tail();
        }
    }
    fn step(&mut self, direction: Direction) -> Result<bool> {
        Ok(if direction == Direction::Forward {
            self.next()? != 0
        } else {
            self.previous()? != 0
        })
    }
    fn realtime(&self) -> Result<u64> {
        Ok(self.get_realtime_usec()?)
    }
    fn payloads(&mut self, visitor: &mut dyn FnMut(&[u8]) -> journal::Result<()>) -> Result<()> {
        self.restart_data()?;
        while let Some(payload) = self.enumerate_available_data()? {
            visitor(payload)?;
        }
        Ok(())
    }
}

fn scan<R: FilterReader>(mut reader: R, cfg: &ReadConfig<'_>) -> Result<Counts> {
    let install = |reader: &mut R| {
        reader.clear();
        for value in cfg.filter_matches {
            reader.add(value);
        }
    };
    install(&mut reader);
    let mut counts = Counts::default();
    for repetition in 0..cfg.filter_repetitions {
        if repetition != 0 && cfg.filter_rebuild {
            install(&mut reader);
        }
        reader.position(cfg);
        let mut count = 0;
        while (cfg.filter_limit == 0 || count < cfg.filter_limit) && reader.step(cfg.direction)? {
            counts.add_record_marker(reader.realtime()?);
            if cfg.mode.ends_with("-digest") {
                reader.payloads(&mut |payload| {
                    counts.add_full_payload(payload);
                    Ok(())
                })?;
            }
            count += 1;
        }
    }
    black_box(counts.checksum);
    Ok(counts)
}

pub(super) fn read(inputs: &[PathBuf], cfg: &ReadConfig<'_>) -> Result<Counts> {
    let options = reader_options(cfg.bounds, cfg.strategy, cfg.window_size)?;
    if cfg.mode.starts_with("facade-") {
        let paths = utf8_input_paths(inputs)?;
        return scan(
            open_facade_reader(&borrowed_input_paths(&paths), cfg.surface, options)?,
            cfg,
        );
    }
    match cfg.surface {
        "file" if inputs.len() == 1 => {
            scan(FileReader::open_with_options(&inputs[0], options)?, cfg)
        }
        "directory" if inputs.len() == 1 => scan(
            DirectoryReader::open_with_options(&inputs[0], options)?,
            cfg,
        ),
        "open-files" => scan(
            DirectoryReader::open_files_with_options(inputs.iter(), options)?,
            cfg,
        ),
        _ => Err(anyhow!("invalid filter benchmark input/surface")),
    }
}
