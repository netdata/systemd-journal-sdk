use crate::repository::RepositoryError;
use crate::repository::error::Result;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Status of a journal file
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub enum Status {
    /// Active journal file currently being written to
    Active,
    /// Archived journal file that has been rotated and is no longer being written to
    Archived {
        /// Sequence number ID for ordering entries across files
        #[cfg_attr(feature = "allocative", allocative(skip))]
        seqnum_id: Uuid,
        /// Sequence number of the first entry in this file
        head_seqnum: u64,
        /// Realtime timestamp (microseconds since epoch) of the first entry
        head_realtime: u64,
    },
    /// Disposed (corrupted or incomplete) journal file marked for cleanup
    Disposed {
        /// Timestamp when the file was disposed (microseconds since epoch)
        timestamp: u64,
        /// Sequence number for ordering multiple disposed files
        number: u64,
    },
}

impl Ord for Status {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Disposed files come first, sorted by timestamp then number
            (
                Status::Disposed {
                    timestamp: t1,
                    number: n1,
                },
                Status::Disposed {
                    timestamp: t2,
                    number: n2,
                },
            ) => t1.cmp(t2).then_with(|| n1.cmp(n2)),

            // Disposed always comes before non-disposed
            (Status::Disposed { .. }, _) => Ordering::Less,
            (_, Status::Disposed { .. }) => Ordering::Greater,

            // Archived files sorted by head_realtime (then seqnum for stability)
            (
                Status::Archived {
                    seqnum_id: lhs_seqnum_id,
                    head_seqnum: lhs_head_seqnum,
                    head_realtime: lhs_head_realtime,
                },
                Status::Archived {
                    seqnum_id: rhs_seqnum_id,
                    head_seqnum: rhs_head_seqnum,
                    head_realtime: rhs_head_realtime,
                },
            ) => lhs_head_realtime
                .cmp(rhs_head_realtime)
                .then_with(|| lhs_seqnum_id.cmp(rhs_seqnum_id))
                .then_with(|| lhs_head_seqnum.cmp(rhs_head_seqnum)),

            // Archived comes before Active
            (Status::Archived { .. }, Status::Active) => Ordering::Less,
            (Status::Active, Status::Archived { .. }) => Ordering::Greater,

            // Active files are equal in terms of status ordering
            (Status::Active, Status::Active) => Ordering::Equal,
        }
    }
}

impl PartialOrd for Status {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Status {
    /// Parse the journal file status from the end of the path, returning the status and the remaining path
    pub(super) fn parse(path: &str) -> Option<(Self, &str)> {
        if let Some(stem) = path.strip_suffix(".journal") {
            return Self::parse_journal_stem(stem);
        }
        let stem = path.strip_suffix(".journal~")?;
        Self::parse_disposed_stem(stem)
    }

    fn parse_journal_stem(stem: &str) -> Option<(Self, &str)> {
        if let Some((prefix, suffix)) = stem.rsplit_once('@') {
            return Self::parse_archived_suffix(prefix, suffix);
        }
        Some((Status::Active, stem))
    }

    fn parse_archived_suffix<'a>(prefix: &'a str, suffix: &str) -> Option<(Self, &'a str)> {
        let mut parts = suffix.split('-');
        let seqnum_id = Uuid::try_parse(parts.next()?).ok()?;
        let head_seqnum = u64::from_str_radix(parts.next()?, 16).ok()?;
        let head_realtime = u64::from_str_radix(parts.next()?, 16).ok()?;
        if parts.next().is_some() {
            return None;
        }

        Some((
            Status::Archived {
                seqnum_id,
                head_seqnum,
                head_realtime,
            },
            prefix,
        ))
    }

    fn parse_disposed_stem(stem: &str) -> Option<(Self, &str)> {
        let (prefix, suffix) = stem.rsplit_once('@')?;
        let (timestamp, number) = suffix.rsplit_once('-')?;
        let timestamp = u64::from_str_radix(timestamp, 16).ok()?;
        let number = u64::from_str_radix(number, 16).ok()?;
        Some((Status::Disposed { timestamp, number }, prefix))
    }
}

/// Source of journal entries
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub enum Source {
    /// System-wide journal (system.journal)
    System,
    /// User-specific journal with the given UID
    User(u32),
    /// Journal from a remote host
    Remote(String),
    /// Unknown or non-standard journal type
    Unknown(String),
}

impl Source {
    /// Parse the journal basename from the end of the path, returning the basename and the remaining path
    pub(super) fn parse(path: &str) -> Option<(Self, &str)> {
        // Split on the last '/' to get directory and basename
        let (dir_path, basename) = path.rsplit_once('/')?;

        let journal_type = if basename == "system" {
            Source::System
        } else if let Some(uid_str) = basename.strip_prefix("user-") {
            if let Ok(uid) = uid_str.parse::<u32>() {
                Source::User(uid)
            } else {
                Source::Unknown(basename.to_string())
            }
        } else if let Some(remote_host) = basename.strip_prefix("remote-") {
            Source::Remote(remote_host.to_string())
        } else {
            Source::Unknown(basename.to_string())
        };

        Some((journal_type, dir_path))
    }
}

/// Origin identifies where a journal file comes from
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub struct Origin {
    /// Machine ID from which the journal originates
    #[cfg_attr(feature = "allocative", allocative(skip))]
    pub machine_id: Option<Uuid>,
    /// Optional namespace for isolated journal instances
    pub namespace: Option<String>,
    /// Source type (system, user, remote, or unknown)
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub(crate) struct FileInner {
    pub(crate) path: String,
    pub(crate) origin: Origin,
    pub(crate) status: Status,
    #[serde(skip)]
    pub(crate) native_path: Option<Box<PathBuf>>,
}

impl Hash for FileInner {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Preserve existing cache hashes; the native key already encodes all bytes.
        self.path.hash(state);
        self.origin.hash(state);
        self.status.hash(state);
    }
}

const NATIVE_PATH_KEY: &str = "\0journal-path-unix-v1:";

#[cfg(unix)]
fn native_path_key(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;

    let bytes = path.as_os_str().as_bytes();
    let mut key = String::with_capacity(NATIVE_PATH_KEY.len() + bytes.len() * 2);
    key.push_str(NATIVE_PATH_KEY);
    for byte in bytes {
        key.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        key.push(char::from(b"0123456789abcdef"[(byte & 15) as usize]));
    }
    key
}

fn decode_native_path_key(key: &str) -> std::result::Result<Option<PathBuf>, &'static str> {
    let Some(encoded) = key.strip_prefix(NATIVE_PATH_KEY) else {
        return Ok(None);
    };
    #[cfg(not(unix))]
    {
        let _ = encoded;
        Err("native Unix path keys are unsupported on this platform")
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;

        if encoded.len() % 2 != 0 {
            return Err("invalid native path key length");
        }
        let mut bytes = Vec::with_capacity(encoded.len() / 2);
        for pair in encoded.as_bytes().chunks_exact(2) {
            let high = char::from(pair[0])
                .to_digit(16)
                .ok_or("invalid native path key")?;
            let low = char::from(pair[1])
                .to_digit(16)
                .ok_or("invalid native path key")?;
            bytes.push(((high << 4) | low) as u8);
        }
        if bytes.contains(&0) {
            return Err("NUL in native filesystem path");
        }
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        if !path.is_absolute() {
            return Err("native path key is not absolute");
        }
        if path.to_str().is_some() {
            return Err("native path key must contain non-UTF8 bytes");
        }
        Ok(Some(path))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub struct File {
    pub(super) inner: Arc<FileInner>,
}

impl serde::Serialize for File {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.as_ref().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for File {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut inner = FileInner::deserialize(deserializer)?;
        if let Some(path) = decode_native_path_key(&inner.path).map_err(serde::de::Error::custom)? {
            #[cfg(unix)]
            {
                inner.path = native_path_key(&path);
            }
            inner.native_path = Some(Box::new(path));
        }
        Ok(File {
            inner: Arc::new(inner),
        })
    }
}

impl File {
    /// Legacy text path/key. Native byte paths use an encoded, non-openable key.
    /// Use `as_path` for filesystem operations.
    pub fn path(&self) -> &str {
        &self.inner.path
    }

    /// Returns the original native filesystem path without lossy conversion.
    pub fn as_path(&self) -> &Path {
        self.inner
            .native_path
            .as_deref()
            .map_or_else(|| Path::new(&self.inner.path), |path| path.as_path())
    }

    pub fn origin(&self) -> &Origin {
        &self.inner.origin
    }

    pub fn status(&self) -> &Status {
        &self.inner.status
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        if !path.is_absolute() {
            return None;
        }

        let path_str = path.to_str()?;
        let filename = path.file_name()?.to_str()?;
        let filename_path = format!("/{filename}");
        let (status, path_after_status) = Status::parse(&filename_path)?;
        let (source, _) = Source::parse(path_after_status)?;

        let (machine_id, namespace) = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|dirname| dirname.to_str())
            .map(parse_machine_id_namespace)
            .unwrap_or((None, None));

        let origin = Origin {
            machine_id,
            namespace,
            source,
        };

        let inner = Arc::new(FileInner {
            path: path_str.to_string(),
            origin,
            status,
            native_path: None,
        });

        Some(File { inner })
    }

    pub fn from_raw_path(path: &Path) -> Option<Self> {
        let raw_path = path;
        if !raw_path.is_absolute() {
            return None;
        }
        let (path, native_path) = match path.to_str() {
            Some(text) => (text.to_string(), None),
            None => {
                #[cfg(unix)]
                {
                    (native_path_key(path), Some(Box::new(path.to_path_buf())))
                }
                #[cfg(not(unix))]
                {
                    return None;
                }
            }
        };

        let inner = Arc::new(FileInner {
            path,
            origin: Origin {
                machine_id: None,
                namespace: None,
                source: Source::Unknown(
                    raw_path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("journal")
                        .to_string(),
                ),
            },
            status: Status::Active,
            native_path,
        });

        Some(File { inner })
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(path: &str) -> Option<Self> {
        // We only accept absolute paths
        if !path.starts_with("/") {
            return None;
        }

        // Parse from right to left
        let (status, path_after_status) = Status::parse(path)?;
        let (source, path_after_source) = Source::parse(path_after_status)?;

        // Try to parse machine ID and namespace from the directory name
        let (machine_id, namespace) = if !path_after_source.is_empty() {
            // Get the last directory component
            let dirname = if let Some((_parent, dir)) = path_after_source.rsplit_once('/') {
                dir
            } else {
                path_after_source
            };

            if let Some((id_str, ns)) = dirname.split_once('.') {
                // Has namespace
                let machine_id = Uuid::try_parse(id_str).ok()?;
                (Some(machine_id), Some(ns.to_string()))
            } else {
                // No namespace, just machine ID
                let machine_id = Uuid::try_parse(dirname).ok();
                (machine_id, None)
            }
        } else {
            (None, None)
        };

        let origin = Origin {
            machine_id,
            namespace,
            source,
        };

        let inner = Arc::new(FileInner {
            path: String::from(path),
            origin,
            status,
            native_path: None,
        });

        Some(File { inner })
    }

    pub fn dir(&self) -> Result<&str> {
        self.as_path()
            .parent()
            .and_then(|p| {
                if self.inner.origin.machine_id.is_some() {
                    p.parent()
                } else {
                    Some(p)
                }
            })
            .and_then(|p| p.to_str())
            .ok_or_else(|| RepositoryError::InvalidUtf8 {
                path: self.as_path().to_path_buf(),
            })
    }

    /// Check if a path looks like a journal file
    pub fn is_journal_file(path: &str) -> bool {
        path.ends_with(".journal") || path.ends_with(".journal~")
    }

    /// Check if this is an active journal file that's currently being written to
    pub fn is_active(&self) -> bool {
        matches!(self.inner.status, Status::Active)
    }

    /// Check if this is an archived journal file
    pub fn is_archived(&self) -> bool {
        matches!(self.inner.status, Status::Archived { .. })
    }

    /// Check if this is a corrupted/disposed journal file
    pub fn is_disposed(&self) -> bool {
        matches!(self.inner.status, Status::Disposed { .. })
    }

    /// Check if this contains logs from users
    pub fn is_user(&self) -> bool {
        matches!(self.inner.origin.source, Source::User(_))
    }

    /// Check if this contains logs from system
    pub fn is_system(&self) -> bool {
        matches!(self.inner.origin.source, Source::System)
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.inner.origin.source, Source::Remote(_))
    }

    /// Get the user ID if this is a user journal
    pub fn user_id(&self) -> Option<u32> {
        match &self.inner.origin.source {
            Source::User(uid) => Some(*uid),
            _ => None,
        }
    }

    /// Get the remote host if this is a remote journal
    pub fn remote_host(&self) -> Option<&str> {
        match &self.inner.origin.source {
            Source::Remote(host) => Some(host.as_str()),
            _ => None,
        }
    }

    /// Get the namespace if this journal belongs to a namespace
    pub fn namespace(&self) -> Option<&str> {
        self.inner.origin.namespace.as_deref()
    }
}

fn parse_machine_id_namespace(dirname: &str) -> (Option<Uuid>, Option<String>) {
    if let Some((id_str, ns)) = dirname.split_once('.') {
        let Some(machine_id) = Uuid::try_parse(id_str).ok() else {
            return (None, None);
        };
        (Some(machine_id), Some(ns.to_string()))
    } else {
        (Uuid::try_parse(dirname).ok(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::{File, Source, Status};
    use std::path::PathBuf;

    #[test]
    fn from_path_parses_native_absolute_paths() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir
            .path()
            .join("00112233445566778899aabbccddeeff")
            .join("system.journal");
        let file = File::from_path(&path).expect("native absolute path parses");

        assert_eq!(file.path(), path.to_str().expect("utf8 path"));
        assert_eq!(
            file.origin()
                .machine_id
                .expect("machine id")
                .simple()
                .to_string(),
            "00112233445566778899aabbccddeeff"
        );
        assert_eq!(file.origin().source, Source::System);
        assert_eq!(file.status(), &Status::Active);
    }

    #[test]
    fn from_path_rejects_relative_paths() {
        assert!(File::from_path(&PathBuf::from("system.journal")).is_none());
    }

    #[test]
    fn from_raw_path_accepts_native_absolute_paths() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("raw-byte-names.journal");
        let file = File::from_raw_path(&path).expect("raw native absolute path parses");

        assert_eq!(file.path(), path.to_str().expect("utf8 path"));
        assert_eq!(
            file.origin().source,
            Source::Unknown("raw-byte-names".to_string())
        );
        assert_eq!(file.status(), &Status::Active);
    }
}

impl Ord for File {
    fn cmp(&self, other: &Self) -> Ordering {
        // First compare by status, then by path for stability
        self.inner
            .status
            .cmp(&other.inner.status)
            .then_with(|| self.inner.path.cmp(&other.inner.path))
    }
}

impl PartialOrd for File {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Scan a directory recursively for journal files
pub fn scan_journal_files(path: &str) -> Result<Vec<File>> {
    let mut files = Vec::new();

    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(file) = File::from_path(path) {
                files.push(file);
            }
        }
    }

    Ok(files)
}
