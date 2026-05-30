#![allow(dead_code)]

#[cfg(unix)]
use crate::error::JournalError;
use crate::error::Result;
#[cfg(unix)]
use std::sync::OnceLock;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(unix)]
static SIGBUS_OCCURRED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static HANDLER_INSTALLED: OnceLock<i32> = OnceLock::new();

#[cfg(unix)]
extern "C" fn sigbus_handler(
    _sig: libc::c_int,
    info: *mut libc::siginfo_t,
    _ucontext: *mut libc::c_void,
) {
    unsafe {
        let si = &*info;
        let fault_addr = si.si_addr();

        let page_addr = (fault_addr as usize & !(4096 - 1)) as *mut libc::c_void;
        libc::mmap(
            page_addr,
            4096,
            libc::PROT_READ,
            libc::MAP_PRIVATE | anonymous_map_flag() | libc::MAP_FIXED,
            -1,
            0,
        );

        SIGBUS_OCCURRED.store(true, Ordering::Relaxed);
    }
}

#[cfg(all(unix, target_os = "linux"))]
fn anonymous_map_flag() -> libc::c_int {
    libc::MAP_ANONYMOUS
}

#[cfg(all(unix, not(target_os = "linux")))]
fn anonymous_map_flag() -> libc::c_int {
    libc::MAP_ANON
}

#[cfg(unix)]
pub fn signalled() -> bool {
    SIGBUS_OCCURRED.load(Ordering::Relaxed)
}

#[cfg(not(unix))]
pub fn signalled() -> bool {
    false
}

#[cfg(unix)]
pub fn install_handler() -> Result<()> {
    let rc = HANDLER_INSTALLED.get_or_init(|| unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();

        sa.sa_flags = libc::SA_SIGINFO;
        sa.sa_sigaction = sigbus_handler as usize;

        libc::sigaction(libc::SIGBUS, &sa, std::ptr::null_mut())
    });

    match rc {
        -1 => Err(JournalError::SigbusHandlerError),
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
pub fn install_handler() -> Result<()> {
    Ok(())
}
