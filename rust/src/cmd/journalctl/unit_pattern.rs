use anyhow::Result;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
pub(super) struct Matcher {
    _locale: super::file_glob::ThreadLocale,
    patterns: Vec<std::ffi::CString>,
    buffer: Vec<u8>,
    #[cfg(target_os = "freebsd")]
    compat: Vec<Option<CompatibilityPattern>>,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
impl Matcher {
    pub(super) fn new(patterns: &[Vec<u8>]) -> Result<Self> {
        Ok(Self {
            _locale: super::file_glob::ThreadLocale::new()?,
            patterns: patterns
                .iter()
                .map(|s| std::ffi::CString::new(s.as_slice()))
                .collect::<std::result::Result<_, _>>()?,
            buffer: Vec::new(),
            #[cfg(target_os = "freebsd")]
            compat: if patterns
                .iter()
                .any(|p| p.windows(2).any(|s| s == b"[:" || s == b"[." || s == b"[="))
            {
                patterns
                    .iter()
                    .map(|p| CompatibilityPattern::new(p))
                    .collect::<Result<_>>()?
            } else {
                Vec::new()
            },
        })
    }

    pub(super) fn matches(&mut self, value: &[u8]) -> bool {
        self.buffer.clear();
        self.buffer.extend_from_slice(value);
        self.buffer.push(0);
        #[cfg(target_os = "freebsd")]
        if !self.compat.is_empty() {
            return self.patterns.iter().zip(&self.compat).any(|(p, c)| {
                if let Some(c) = c {
                    c.matches(&self.buffer)
                } else {
                    unsafe {
                        libc::fnmatch(p.as_ptr(), self.buffer.as_ptr().cast(), libc::FNM_NOESCAPE)
                            == 0
                    }
                }
            });
        }
        self.patterns.iter().any(|p| unsafe {
            libc::fnmatch(p.as_ptr(), self.buffer.as_ptr().cast(), libc::FNM_NOESCAPE) == 0
        })
    }
}

#[cfg(any(
    test,
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))
))]
pub(super) struct ByteMatcher {
    patterns: Vec<regex::bytes::Regex>,
}

#[cfg(any(
    test,
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))
))]
impl ByteMatcher {
    pub(super) fn new(patterns: &[Vec<u8>]) -> Result<Self> {
        Ok(Self {
            patterns: patterns
                .iter()
                .filter_map(|p| {
                    regex::bytes::RegexBuilder::new(&super::glob_pattern_to_regex(
                        &String::from_utf8_lossy(p),
                    ))
                    .unicode(false)
                    .build()
                    .ok()
                })
                .collect(),
        })
    }

    pub(super) fn matches(&mut self, value: &[u8]) -> bool {
        self.patterns.iter().any(|p| p.is_match(value))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
pub(super) type Matcher = ByteMatcher;

#[cfg(test)]
mod tests {
    use super::ByteMatcher;

    #[test]
    fn fallback_byte_grammar() {
        for (pattern, value, want) in [
            ("[[.a.]]lpha.*", b"alpha.service".as_slice(), true),
            ("[[]", b"[".as_slice(), true),
            ("[[:alpha:]]lpha.*", b"alpha.service".as_slice(), true),
            ("[!b]lpha.*", b"blpha.service".as_slice(), false),
            ("[]a]*", b"]name".as_slice(), true),
            ("alpha[", b"alpha[".as_slice(), true),
            (r"alpha\x20*", br"alpha\x20name.service".as_slice(), true),
            ("bad-?.service", b"bad-\xff.service".as_slice(), true),
            ("bad-?.service", "bad-é.service".as_bytes(), false),
            ("bad-??.service", "bad-é.service".as_bytes(), true),
        ] {
            let mut matcher = ByteMatcher::new(&[pattern.as_bytes().to_vec()]).unwrap();
            assert_eq!(matcher.matches(value), want, "{pattern}");
        }
    }
}

#[cfg(any(
    target_os = "freebsd",
    all(test, any(target_os = "linux", target_os = "macos"))
))]
struct CompatibilityPattern(std::ptr::NonNull<std::ffi::c_void>);

#[cfg(any(
    target_os = "freebsd",
    all(test, any(target_os = "linux", target_os = "macos"))
))]
unsafe extern "C" {
    fn journalctl_unit_pattern_new(pattern: *const libc::c_char) -> *mut std::ffi::c_void;
    fn journalctl_unit_pattern_needed(pattern: *const libc::c_char) -> libc::c_int;
    fn journalctl_unit_pattern_match(
        pattern: *mut std::ffi::c_void,
        value: *const libc::c_char,
    ) -> libc::c_int;
    fn journalctl_unit_pattern_free(pattern: *mut std::ffi::c_void);
}

#[cfg(any(
    target_os = "freebsd",
    all(test, any(target_os = "linux", target_os = "macos"))
))]
impl CompatibilityPattern {
    fn new(pattern: &[u8]) -> Result<Option<Self>> {
        let pattern = std::ffi::CString::new(pattern)?;
        if unsafe { journalctl_unit_pattern_needed(pattern.as_ptr()) } == 0 {
            return Ok(None);
        }
        let ptr = unsafe { journalctl_unit_pattern_new(pattern.as_ptr()) };
        Ok(Some(Self(std::ptr::NonNull::new(ptr).ok_or_else(
            || anyhow::anyhow!("failed to prepare unit pattern"),
        )?)))
    }
    fn matches(&self, terminated_value: &[u8]) -> bool {
        unsafe {
            journalctl_unit_pattern_match(self.0.as_ptr(), terminated_value.as_ptr().cast()) != 0
        }
    }
}

#[cfg(any(
    target_os = "freebsd",
    all(test, any(target_os = "linux", target_os = "macos"))
))]
impl Drop for CompatibilityPattern {
    fn drop(&mut self) {
        unsafe { journalctl_unit_pattern_free(self.0.as_ptr()) }
    }
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "freebsd")
))]
#[test]
fn compatibility_brackets() {
    struct LocaleGuard(libc::locale_t, libc::locale_t);
    impl Drop for LocaleGuard {
        fn drop(&mut self) {
            unsafe {
                libc::uselocale(self.1);
                libc::freelocale(self.0);
            }
        }
    }
    for locale in [c"C", c"en_US.UTF-8"] {
        let selected =
            unsafe { libc::newlocale(libc::LC_ALL_MASK, locale.as_ptr(), std::ptr::null_mut()) };
        assert!(
            !selected.is_null(),
            "required test locale {locale:?} unavailable"
        );
        let previous = unsafe { libc::uselocale(selected) };
        assert!(!previous.is_null());
        let _guard = LocaleGuard(selected, previous);
        for (pattern, value, want) in [
            ("[[:alpha:]]", "é".as_bytes(), locale != c"C"),
            ("[[=a=]]", "à".as_bytes(), locale != c"C"),
            ("[[=a=]]", b"A", locale != c"C"),
            ("[[=a=]]?", "à".as_bytes(), locale != c"C"),
            ("[[=a=]-z]", b"-", true),
            ("[[:alpha:]]lpha.*", b"alpha.service".as_slice(), true),
            ("[[:alpha:]]lpha.*", b"1lpha.service", false),
            ("[![:alpha:]]*", b"1.service", true),
            ("[![:alpha:]]*", b"alpha.service", false),
            ("[[.a.]]lpha.*", b"alpha.service", true),
            ("[[=a=]]lpha.*", b"alpha.service", true),
            ("[[:alpha:]]*", b"a\xff.service", true),
            ("[![:alpha:]]?", "é".as_bytes(), true),
            ("*[[:alpha:]]*", b"\xffa\xff", true),
            (r"[[:alpha:]\]", b"\\", true),
            ("[[:alpha:]]alpha[", b"aalpha[", true),
            ("[[:alpha:]]$", b"a$", true),
            ("[[:alpha:]]?", b"a\n", true),
            ("[[:alpha:]]", b"a\n", false),
            ("[[:unknown:]]*", b"a", false),
            ("[[:<:]]*", b"a", false),
            ("[[.period.]]*", b".", false),
            ("[[:alpha:]", b"a", false),
            ("[[[:bogus:]]", b"[", true),
            ("[a-[:alpha:]]", b"a]", true),
            ("[[:alpha:]z-a]", b"a", true),
            ("[[[:<:]]", b"[", false),
            ("[[:-", b"[[:-", false),
            ("[[[.a]", b"[", false),
            ("[[[=a", b"[[[=a", false),
        ] {
            let p = CompatibilityPattern::new(pattern.as_bytes())
                .unwrap()
                .unwrap();
            let mut terminated = value.to_vec();
            terminated.push(0);
            assert_eq!(
                p.matches(&terminated),
                want,
                "{locale:?}: {pattern:?} {value:?}"
            );
        }

        let pattern = CompatibilityPattern::new(b"[[:digit:]a-z]")
            .unwrap()
            .unwrap();
        for value in ["a", "A", "é", "É", "à", "z"] {
            let value = std::ffi::CString::new(value).unwrap();
            let expected = unsafe {
                libc::fnmatch(c"[a-z]".as_ptr(), value.as_ptr(), libc::FNM_NOESCAPE) == 0
            };
            assert_eq!(
                pattern.matches(value.as_bytes_with_nul()),
                expected,
                "native range: {value:?}"
            );
        }
    }
}
