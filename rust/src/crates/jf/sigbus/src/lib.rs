use error::{JournalError, Result};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static SIGBUS_OCCURRED: AtomicBool = AtomicBool::new(false);
static HANDLER_INSTALLED: OnceLock<i32> = OnceLock::new();

extern "C" fn sigbus_handler(
    _sig: libc::c_int,
    info: *mut libc::siginfo_t,
    _ucontext: *mut libc::c_void,
) {
    // SAFETY: This signal handler only reads the provided siginfo pointer,
    // remaps the faulting page with async-signal-safe `mmap`, and stores an
    // atomic flag. It does not retain pointers after returning.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let si = &*info;
        let fault_addr = si.si_addr();

        let page_addr = (fault_addr as usize & !(4096 - 1)) as *mut libc::c_void;
        libc::mmap(
            page_addr,
            4096,
            libc::PROT_READ,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED,
            -1,
            0,
        );

        SIGBUS_OCCURRED.store(true, Ordering::Relaxed);
    }
}

pub fn signalled() -> bool {
    SIGBUS_OCCURRED.load(Ordering::Relaxed)
}

pub fn install_handler() -> Result<()> {
    // SAFETY: The handler function has C ABI and `sigaction` is initialized
    // before registration for SIGBUS.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
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
