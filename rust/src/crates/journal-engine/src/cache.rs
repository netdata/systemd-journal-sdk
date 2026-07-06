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
const CACHE_VERSION: u32 = 3;

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
