//! Cache types for journal file indexes

use crate::facets::Facets;
use foyer::HybridCache;
use journal_index::{FieldName, FileIndex};
use journal_registry::File;
use serde::{Deserialize, Serialize};

/// Cache version number. Increment this when the FileIndex schema, FileIndexKey
/// schema, or indexer semantics change to automatically invalidate old cache
/// entries.
///
/// v3: Index semantics changed from the v2 ND_REMAPPING-specific behavior, and
///     FileIndexKey gained an explicit consumer namespace. Old v2 cache entries
///     are not reused across SDK versions with different index rules.
/// v4: Native array traversal rejects missing required links and counted
///     revisits; older cached indexes may contain incomplete or duplicate rows.
const CACHE_VERSION: u32 = 4;

/// Cache key for file indexes that includes the file, facets, source timestamp
/// field, and cache version. Different facet configurations or timestamp fields
/// produce different indexes, so all are needed to uniquely identify a cached
/// index. The version ensures that schema changes automatically invalidate old
/// cache entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileIndexKey {
    version: u32,
    #[serde(default)]
    namespace: String,
    pub file: File,
    pub(crate) facets: Facets,
    pub(crate) source_timestamp_field: Option<FieldName>,
}

impl FileIndexKey {
    pub fn new(file: &File, facets: &Facets, source_timestamp_field: Option<FieldName>) -> Self {
        Self::new_with_namespace(file, facets, source_timestamp_field, "")
    }

    /// Creates a cache key inside a consumer-controlled namespace.
    ///
    /// Consumers can change the namespace to force a clean index rebuild for
    /// their own semantic migrations without changing the SDK-owned cache
    /// version or moving the cache directory.
    pub fn new_with_namespace(
        file: &File,
        facets: &Facets,
        source_timestamp_field: Option<FieldName>,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            version: CACHE_VERSION,
            namespace: namespace.into(),
            file: file.clone(),
            facets: facets.clone(),
            source_timestamp_field,
        }
    }
}

/// Type alias for file index cache using Foyer's HybridCache.
pub type FileIndexCache = HybridCache<FileIndexKey, FileIndex>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn cache_version_bump_prevents_old_key_reuse() {
        let file = test_file();
        let facets = Facets::new(&[]);
        let current = FileIndexKey::new(&file, &facets, None);
        let old = FileIndexKey {
            version: CACHE_VERSION - 1,
            namespace: String::new(),
            file,
            facets,
            source_timestamp_field: None,
        };

        let mut cache = HashMap::new();
        cache.insert(old, "old-index");

        assert_eq!(cache.get(&current), None);
    }

    #[test]
    fn cache_namespace_keys_do_not_collide() {
        let file = test_file();
        let facets = Facets::new(&[]);

        let default = FileIndexKey::new(&file, &facets, None);
        let explicit_default = FileIndexKey::new_with_namespace(&file, &facets, None, "");
        let named = FileIndexKey::new_with_namespace(&file, &facets, None, "consumer-v2");

        assert_eq!(default, explicit_default);
        assert_ne!(default, named);
    }

    #[test]
    fn native_path_support_preserves_ordinary_file_cache_identity() {
        use std::hash::{Hash, Hasher};

        #[derive(Hash, Serialize)]
        struct LegacyFile<'a> {
            path: &'a str,
            origin: &'a journal_registry::Origin,
            status: &'a journal_registry::Status,
        }
        let file = test_file();
        let legacy = LegacyFile {
            path: file.path(),
            origin: file.origin(),
            status: file.status(),
        };
        assert_eq!(
            serde_json::to_vec(&file).unwrap(),
            serde_json::to_vec(&legacy).unwrap()
        );
        let mut actual = std::collections::hash_map::DefaultHasher::new();
        let mut previous = std::collections::hash_map::DefaultHasher::new();
        file.hash(&mut actual);
        legacy.hash(&mut previous);
        assert_eq!(actual.finish(), previous.finish());
        let decoded: File = serde_json::from_slice(&serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(decoded, file);
        assert_eq!(decoded.as_path(), Path::new(legacy.path));
    }

    #[cfg(unix)]
    #[test]
    fn native_path_cache_keys_round_trip_without_replacement_aliases() {
        use std::os::unix::ffi::OsStringExt;

        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(
            b"/example/raw-\xff.journal".to_vec(),
        ));
        let file = File::from_raw_path(&path).unwrap();
        let decoy = File::from_raw_path(Path::new(path.to_string_lossy().as_ref())).unwrap();
        let key = FileIndexKey::new(&file, &Facets::new(&[]), None);
        let replacement = FileIndexKey::new(&decoy, &Facets::new(&[]), None);
        assert_ne!(key, replacement);
        let encoded = serde_json::to_vec(&key).unwrap();
        let decoded: FileIndexKey = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, key);
        assert_eq!(decoded.file.as_path(), path);
        // An old text-only consumer must fail to open, rather than select the decoy.
        assert!(decoded.file.path().as_bytes().contains(&0));
        assert_eq!(
            std::fs::File::open(decoded.file.path()).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
        let mut cache = HashMap::new();
        cache.insert(key, "native");
        assert_eq!(cache.get(&decoded), Some(&"native"));
        assert_eq!(cache.get(&replacement), None);
        let mut malformed = serde_json::to_value(&decoded).unwrap();
        malformed["file"]["path"] = serde_json::json!("\0journal-path-unix-v1:2f00");
        assert!(serde_json::from_value::<FileIndexKey>(malformed).is_err());
    }

    #[test]
    fn old_serialized_cache_key_without_namespace_decodes_as_old_key() {
        let file = test_file();
        let facets = Facets::new(&[]);
        let old = FileIndexKey {
            version: CACHE_VERSION - 1,
            namespace: "old-cache".to_string(),
            file: file.clone(),
            facets: facets.clone(),
            source_timestamp_field: None,
        };
        let mut old_shape = serde_json::to_value(&old).expect("serialize old key");
        old_shape
            .as_object_mut()
            .expect("key serializes as object")
            .remove("namespace");

        let decoded: FileIndexKey =
            serde_json::from_value(old_shape).expect("decode old key without namespace");

        assert_eq!(decoded.namespace, "");
        assert_eq!(decoded.version, CACHE_VERSION - 1);
        assert_ne!(decoded, FileIndexKey::new(&file, &facets, None));
    }

    fn test_file() -> File {
        File::from_path(Path::new(
            "/var/log/journal/00112233445566778899aabbccddeeff/system.journal",
        ))
        .expect("valid journal file path")
    }
}
