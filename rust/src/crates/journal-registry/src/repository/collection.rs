use crate::repository::error::Result;
use crate::repository::{File, Origin, Status};
use journal_common::Seconds;
use journal_common::collections::{HashMap, VecDeque};
use tracing::error;

/// An ordered collection of journal files from the same origin
///
/// Files are kept sorted by status and time:
/// - Disposed files (corrupted) come first
/// - Archived files follow in chronological order (by head_realtime)
/// - Active file (if any) comes last
///
/// This ordering is maintained automatically by [`insert_file()`](Self::insert_file)
/// and is critical for correct time-range queries.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub struct Chain {
    /// Ordered collection of files maintaining the sorting invariant
    pub(crate) files: VecDeque<File>,
}

impl Chain {
    /// Insert a file maintaining sorted order (disposed → archived → active)
    pub fn insert_file(&mut self, file: File) {
        let pos = self.files.partition_point(|f| *f < file);

        if pos < self.files.len() && self.files[pos] == file {
            return;
        }

        self.files.insert(pos, file.clone());
    }

    /// Remove a file from the chain
    pub fn remove_file(&mut self, file: &File) {
        // Use partition_point to find where the file would be
        let pos = self.files.partition_point(|f| f < file);

        // Check if the file at this position matches the one we want to remove
        if pos < self.files.len() && self.files[pos] == *file {
            self.files.remove(pos);
        }
    }

    pub fn pop_front(&mut self) -> Option<File> {
        self.files.pop_front()
    }

    pub fn back(&self) -> Option<&File> {
        self.files.back()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Remove files older than cutoff time (microseconds since epoch)
    ///
    /// Active files are never drained.
    pub fn drain(&mut self, cutoff_time: u64) -> impl Iterator<Item = File> + '_ {
        let mut drained = Vec::new();
        let mut retained = VecDeque::new();

        while let Some(file) = self.files.pop_front() {
            let should_drain = match file.status() {
                Status::Active => false,
                Status::Archived { head_realtime, .. } => *head_realtime <= cutoff_time,
                Status::Disposed { timestamp, .. } => *timestamp <= cutoff_time,
            };

            if should_drain {
                drained.push(file);
            } else {
                retained.push_back(file);
            }
        }

        self.files = retained;
        drained.into_iter()
    }

    /// Find files that overlap with the time range [start, end)
    ///
    /// Extends the provided collection with matching files.
    pub fn find_files_in_range<C>(&self, start: Seconds, end: Seconds, files: &mut C)
    where
        C: Extend<File>,
    {
        if self.files.is_empty() || start >= end {
            return;
        }

        let start = seconds_to_microseconds(start);
        let end = seconds_to_microseconds(end);
        let pos = self.first_range_candidate_position(start);
        let mut prev_head_realtime = self.archived_head_at(pos);
        let mut iter = self.files.iter().skip(pos).peekable();

        while let Some(file) = iter.next() {
            match file.status() {
                Status::Archived { head_realtime, .. } => {
                    if *head_realtime >= end {
                        break;
                    }

                    let tail_realtime = next_file_tail_realtime(iter.peek().copied());
                    if range_overlaps(*head_realtime, tail_realtime, start, end) {
                        files.extend(std::iter::once(file.clone()));
                    }
                    prev_head_realtime = Some(*head_realtime);
                }
                Status::Active => {
                    let head_realtime = prev_head_realtime.unwrap_or(u64::MIN);
                    if range_overlaps(head_realtime, u64::MAX, start, end) {
                        files.extend(std::iter::once(file.clone()));
                    }
                    break;
                }
                Status::Disposed { .. } => {
                    continue;
                }
            }
        }
    }

    fn first_range_candidate_position(&self, start: u64) -> usize {
        self.files
            .partition_point(|f| match f.status() {
                Status::Active => false,
                Status::Archived { head_realtime, .. } => *head_realtime < start,
                Status::Disposed { .. } => true,
            })
            .saturating_sub(1)
    }

    fn archived_head_at(&self, pos: usize) -> Option<u64> {
        match self.files.get(pos).map(|f| f.status()) {
            Some(Status::Archived { head_realtime, .. }) => Some(*head_realtime),
            _ => None,
        }
    }
}

fn seconds_to_microseconds(seconds: Seconds) -> u64 {
    const USEC_PER_SEC: u64 = std::time::Duration::from_secs(1).as_micros() as u64;
    seconds.0 as u64 * USEC_PER_SEC
}

fn next_file_tail_realtime(next_file: Option<&File>) -> u64 {
    let Some(next_file) = next_file else {
        return u64::MAX;
    };
    match next_file.status() {
        Status::Archived { head_realtime, .. } => *head_realtime,
        Status::Active => u64::MAX,
        Status::Disposed { .. } => {
            error!(
                "Disposed file found after archived file, violating chain ordering: {:?}",
                next_file.path()
            );
            u64::MAX
        }
    }
}

fn range_overlaps(head_realtime: u64, tail_realtime: u64, start: u64, end: u64) -> bool {
    head_realtime < end && tail_realtime > start
}

#[derive(Default, Debug)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub(super) struct Directory {
    pub(super) chains: HashMap<Origin, Chain>,
}

/// A repository that organizes journal files by directory and origin
///
/// The repository maintains a three-level hierarchy:
/// ```text
/// Repository
///   └─ Directory (/var/log/journal)
///       └─ Origin (System, User(1000), etc.)
///           └─ Chain (ordered list of files)
/// ```
///
/// This structure allows efficient querying and management of journal files
/// from multiple directories and origins.
#[derive(Default)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub struct Repository {
    /// Maps journal directory paths to their contents
    pub(super) directories: HashMap<String, Directory>,
}

impl Repository {
    /// Insert a file into the appropriate directory/origin/chain
    pub fn insert(&mut self, file: File) -> Result<()> {
        let dir = file.dir()?.to_string();

        if let Some(directory) = self.directories.get_mut(&dir) {
            if let Some(chain) = directory.chains.get_mut(file.origin()) {
                chain.insert_file(file);
            } else {
                let origin = file.origin().clone();
                let mut chain = Chain::default();
                chain.insert_file(file);
                directory.chains.insert(origin, chain);
            }
        } else {
            let origin = file.origin().clone();
            let mut chain = Chain::default();
            chain.insert_file(file);

            let mut directory = Directory::default();
            directory.chains.insert(origin, chain);

            self.directories.insert(dir, directory);
        }
        Ok(())
    }

    /// Remove a file and clean up empty chains/directories
    pub fn remove(&mut self, file: &File) -> Result<()> {
        let dir = file.dir()?;
        let mut remove_directory = false;

        if let Some(directory) = self.directories.get_mut(dir) {
            let mut remove_chain = false;

            if let Some(chain) = directory.chains.get_mut(file.origin()) {
                chain.remove_file(file);
                remove_chain = chain.is_empty();
            };

            if remove_chain {
                directory.chains.remove(file.origin());
            }

            remove_directory = directory.chains.is_empty();
        };

        if remove_directory {
            self.directories.remove(dir);
        }
        Ok(())
    }

    /// Remove all files from a directory
    pub fn remove_directory(&mut self, path: &str) {
        self.directories.remove(path);
    }

    /// Collect all files in the given time range
    pub fn find_files_in_range<C>(&self, start: Seconds, end: Seconds) -> C
    where
        C: FromIterator<File> + Extend<File> + Default,
    {
        let mut files = C::default();

        for directory in self.directories.values() {
            for chain in directory.chains.values() {
                chain.find_files_in_range(start, end, &mut files);
            }
        }

        files
    }
}
