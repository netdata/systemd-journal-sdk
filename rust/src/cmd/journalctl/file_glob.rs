use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
pub(super) struct ThreadLocale {
    selected: libc::locale_t,
    previous: libc::locale_t,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
impl ThreadLocale {
    pub(super) fn new() -> Result<Self> {
        let mut selected =
            unsafe { libc::newlocale(libc::LC_ALL_MASK, c"".as_ptr(), std::ptr::null_mut()) };
        if selected.is_null() {
            selected =
                unsafe { libc::newlocale(libc::LC_ALL_MASK, c"C".as_ptr(), std::ptr::null_mut()) };
        }
        if selected.is_null() {
            return Err(anyhow!("failed to initialize pattern locale"));
        }
        let previous = unsafe { libc::uselocale(selected) };
        if previous.is_null() {
            unsafe { libc::freelocale(selected) };
            return Err(anyhow!("failed to select pattern locale"));
        }
        Ok(Self { selected, previous })
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
impl Drop for ThreadLocale {
    fn drop(&mut self) {
        // Restore before releasing the locale still selected by this thread.
        unsafe {
            libc::uselocale(self.previous);
            libc::freelocale(self.selected);
        }
    }
}

pub(super) fn expand(pattern: &Path) -> Result<Vec<PathBuf>> {
    if !pattern
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .any(|b| b"*?[]{}".contains(b) || cfg!(unix) && *b == b'\\')
    {
        return Ok(Vec::new());
    }
    expand_platform(pattern)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
fn expand_platform(pattern: &Path) -> Result<Vec<PathBuf>> {
    use std::ffi::{CStr, CString, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    struct Matches(libc::glob_t);
    impl Drop for Matches {
        fn drop(&mut self) {
            // glob owns all result strings, including partial error results.
            unsafe { libc::globfree(&mut self.0) };
        }
    }

    let pattern = CString::new(pattern.as_os_str().as_bytes())?;
    let _locale = ThreadLocale::new()?;
    // The zeroed glob result is valid for glob and globfree on failure.
    let mut matches = Matches(unsafe { std::mem::zeroed() });
    #[cfg(target_os = "linux")]
    let brace = libc::GLOB_BRACE;
    // libc omits this flag from its Apple and FreeBSD bindings.
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    let brace = 0x0080;
    let status = unsafe {
        libc::glob(
            pattern.as_ptr(),
            brace | libc::GLOB_NOCHECK | libc::GLOB_NOSORT,
            None,
            &mut matches.0,
        )
    };
    if status != 0 {
        return Err(anyhow!("file pattern expansion failed ({status})"));
    }
    if matches.0.gl_pathc == 0 {
        return Ok(Vec::new());
    }
    let count = usize::try_from(matches.0.gl_pathc)?;
    let paths = unsafe { std::slice::from_raw_parts(matches.0.gl_pathv, count) };
    Ok(paths
        .iter()
        .map(|path| {
            let bytes = unsafe { CStr::from_ptr(*path) }.to_bytes();
            PathBuf::from(OsString::from_vec(bytes.to_vec()))
        })
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
fn expand_platform(pattern: &Path) -> Result<Vec<PathBuf>> {
    let text = pattern
        .to_str()
        .ok_or_else(|| anyhow!("file patterns require Unicode filenames on this platform"))?;
    let mut paths = Vec::new();
    for alternative in brace_alternatives(text) {
        paths.extend(
            expand_unicode(Path::new(&alternative))?
                .into_iter()
                .filter(|p| p.exists()),
        );
    }
    Ok(paths)
}

#[cfg(any(
    test,
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))
))]
fn brace_alternatives(pattern: &str) -> Vec<String> {
    for (start, byte) in pattern.bytes().enumerate() {
        if byte != b'{' {
            continue;
        }
        let (mut depth, mut part) = (1, start + 1);
        let mut alternatives = Vec::new();
        for (end, byte) in pattern.bytes().enumerate().skip(start + 1) {
            match byte {
                b'{' => depth += 1,
                b',' if depth == 1 => {
                    alternatives.push(&pattern[part..end]);
                    part = end + 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth != 0 {
                        continue;
                    }
                    if !alternatives.is_empty() {
                        alternatives.push(&pattern[part..end]);
                        return alternatives
                            .into_iter()
                            .flat_map(|value| {
                                brace_alternatives(&format!(
                                    "{}{value}{}",
                                    &pattern[..start],
                                    &pattern[end + 1..]
                                ))
                            })
                            .collect();
                    }
                    break;
                }
                _ => {}
            }
        }
    }
    vec![pattern.to_owned()]
}

#[cfg(any(
    test,
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))
))]
fn expand_unicode(pattern: &Path) -> Result<Vec<PathBuf>> {
    let mut prefixes = Vec::<PathBuf>::new();
    for component in pattern.components() {
        match component {
            std::path::Component::Prefix(prefix) => {
                prefixes = vec![PathBuf::from(prefix.as_os_str())]
            }
            std::path::Component::RootDir => {
                if prefixes.is_empty() {
                    prefixes.push(PathBuf::from(std::path::MAIN_SEPARATOR.to_string()));
                } else {
                    for prefix in &mut prefixes {
                        prefix.push(std::path::MAIN_SEPARATOR.to_string());
                    }
                }
            }
            std::path::Component::CurDir | std::path::Component::ParentDir => {
                if prefixes.is_empty() {
                    prefixes.push(PathBuf::new());
                }
                for prefix in &mut prefixes {
                    prefix.push(component.as_os_str());
                }
            }
            std::path::Component::Normal(component) => {
                if prefixes.is_empty() {
                    prefixes.push(PathBuf::new());
                }
                let text = component
                    .to_str()
                    .ok_or_else(|| anyhow!("Unicode file pattern required"))?;
                if super::is_glob_pattern(text) {
                    let matcher = regex::Regex::new(&super::glob_pattern_to_regex(text))?;
                    let mut next = Vec::new();
                    for prefix in &prefixes {
                        let dir = if prefix.as_os_str().is_empty() {
                            Path::new(".")
                        } else {
                            prefix
                        };
                        let Ok(entries) = std::fs::read_dir(dir) else {
                            continue;
                        };
                        for entry in entries.flatten() {
                            let name = entry.file_name();
                            let Some(name_text) = name.to_str() else {
                                continue;
                            };
                            if name_text.starts_with('.') && !text.starts_with('.') {
                                continue;
                            }
                            if matcher.is_match(name_text) {
                                next.push(prefix.join(name));
                            }
                        }
                    }
                    prefixes = next;
                } else {
                    for prefix in &mut prefixes {
                        prefix.push(component);
                    }
                }
            }
        }
    }
    prefixes.sort();
    Ok(prefixes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_brace_alternatives() {
        assert_eq!(
            brace_alternatives("{a,{b,c}}-é.journal"),
            ["a-é.journal", "b-é.journal", "c-é.journal"]
        );
        assert_eq!(brace_alternatives("{plain}.journal"), ["{plain}.journal"]);
        assert_eq!(
            brace_alternatives("{{a,b}}.journal"),
            ["{a}.journal", "{b}.journal"]
        );
        assert_eq!(
            brace_alternatives("{a,}?.journal"),
            ["a?.journal", "?.journal"]
        );
    }

    #[test]
    fn unicode_wildcards_require_explicit_dot() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("é.journal"), b"").unwrap();
        std::fs::write(directory.path().join(".hidden.journal"), b"").unwrap();
        assert_eq!(
            expand_unicode(&directory.path().join("?.journal")).unwrap(),
            [directory.path().join("é.journal")]
        );
        assert_eq!(
            expand_unicode(&directory.path().join(".*.journal")).unwrap(),
            [directory.path().join(".hidden.journal")]
        );
    }
}
