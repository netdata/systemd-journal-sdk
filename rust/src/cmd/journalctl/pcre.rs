use anyhow::{Result, anyhow};
use std::ffi::{c_int, c_void};
use std::ptr::{NonNull, null_mut};

#[link(name = "pcre2-8")]
unsafe extern "C" {
    fn pcre2_compile_8(
        pattern: *const u8,
        length: usize,
        options: u32,
        error: *mut c_int,
        offset: *mut usize,
        context: *mut c_void,
    ) -> *mut c_void;
    fn pcre2_code_free_8(code: *mut c_void);
    fn pcre2_match_data_create_8(count: u32, context: *mut c_void) -> *mut c_void;
    fn pcre2_match_data_free_8(data: *mut c_void);
    fn pcre2_get_ovector_pointer_8(data: *mut c_void) -> *mut usize;
    fn pcre2_match_8(
        code: *const c_void,
        subject: *const u8,
        length: usize,
        start: usize,
        options: u32,
        data: *mut c_void,
        context: *mut c_void,
    ) -> c_int;
    fn pcre2_get_error_message_8(code: c_int, buffer: *mut u8, length: usize) -> c_int;
    fn pcre2_config_8(what: u32, value: *mut c_void) -> c_int;
}

// PCRE2's 8-bit ABI uses size_t even on platforms with a 32-bit C long.
#[derive(Debug)]
pub struct Pattern {
    code: NonNull<c_void>,
    data: NonNull<c_void>,
}

fn error_message(code: c_int) -> String {
    let mut buffer = [0u8; 512];
    // SAFETY: PCRE2 writes at most the supplied buffer length.
    let n = unsafe { pcre2_get_error_message_8(code, buffer.as_mut_ptr(), buffer.len()) };
    if n < 0 {
        return format!("PCRE2 error {code}");
    }
    String::from_utf8_lossy(&buffer[..n as usize]).into_owned()
}

pub fn version() -> String {
    let mut buffer = [0u8; 128];
    // SAFETY: PCRE2_CONFIG_VERSION requires a byte buffer large enough for its version.
    if unsafe { pcre2_config_8(11, buffer.as_mut_ptr().cast()) } < 0 {
        return "unknown".into();
    }
    let n = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..n]).into_owned()
}

impl Pattern {
    pub fn compile(pattern: &[u8], sensitive: bool) -> Result<Self> {
        let mut error = 0;
        let mut offset = 0;
        let flags = if sensitive { 0 } else { 0x0000_0008 }; // PCRE2_CASELESS
        // SAFETY: the pattern is borrowed only for compilation; PCRE2 owns its result.
        let code = NonNull::new(unsafe {
            pcre2_compile_8(
                pattern.as_ptr(),
                pattern.len(),
                flags,
                &mut error,
                &mut offset,
                null_mut(),
            )
        })
        .ok_or_else(|| anyhow!("Bad pattern: {} (offset {offset})", error_message(error)))?;
        // SAFETY: a one-pair match vector suffices for a boolean result, including rc=0.
        let data = NonNull::new(unsafe { pcre2_match_data_create_8(1, null_mut()) });
        let Some(data) = data else {
            // SAFETY: this compiled code has not been transferred to another owner.
            unsafe { pcre2_code_free_8(code.as_ptr()) };
            return Err(anyhow!("Failed to allocate PCRE2 match data"));
        };
        Ok(Self { code, data })
    }

    pub fn matches(&self, subject: &[u8]) -> Result<bool> {
        // SAFETY: this CLI-private handle is not shared across threads. Matching borrows
        // subject bytes for the call; no subsequent operation reads PCRE2's subject pointer.
        let rc = unsafe {
            pcre2_match_8(
                self.code.as_ptr(),
                subject.as_ptr(),
                subject.len(),
                0,
                0,
                self.data.as_ptr(),
                null_mut(),
            )
        };
        if rc == -1 {
            return Ok(false);
        } // PCRE2_ERROR_NOMATCH
        if rc < 0 {
            return Err(anyhow!("Pattern matching failed: {}", error_message(rc)));
        }
        Ok(true)
    }

    pub fn match_with_highlight(
        &self,
        subject: &[u8],
        highlight: &mut super::colors::MatchRange,
    ) -> Result<bool> {
        if !self.matches(subject)? {
            return Ok(false);
        }
        // Read the existing match before another subject can replace its offsets.
        let offsets = unsafe { pcre2_get_ovector_pointer_8(self.data.as_ptr()) };
        *highlight = unsafe {
            super::colors::MatchRange {
                start: *offsets,
                end: *offsets.add(1),
            }
        };
        Ok(true)
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        // SAFETY: both handles are uniquely owned and freed exactly once.
        unsafe {
            pcre2_match_data_free_8(self.data.as_ptr());
            pcre2_code_free_8(self.code.as_ptr());
        }
    }
}
