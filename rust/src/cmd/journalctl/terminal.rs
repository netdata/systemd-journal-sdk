#[cfg(unix)]
pub(super) fn stdout_columns() -> Option<usize> {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    let result = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) };
    if result < 0 {
        return None;
    }
    let width = unsafe { size.assume_init() }.ws_col;
    (width > 0).then_some(width as usize)
}

#[cfg(windows)]
pub(super) fn stdout_columns() -> Option<usize> {
    use std::ffi::c_void;

    #[repr(C)]
    struct Coord {
        x: i16,
        y: i16,
    }

    #[repr(C)]
    struct Rect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }

    #[repr(C)]
    struct ConsoleInfo {
        size: Coord,
        cursor: Coord,
        attributes: u16,
        window: Rect,
        maximum_window: Coord,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> *mut c_void;
        fn GetConsoleScreenBufferInfo(handle: *mut c_void, info: *mut ConsoleInfo) -> i32;
    }

    let mut info = std::mem::MaybeUninit::<ConsoleInfo>::uninit();
    let result =
        unsafe { GetConsoleScreenBufferInfo(GetStdHandle((-11i32) as u32), info.as_mut_ptr()) };
    if result == 0 {
        return None;
    }
    let window = unsafe { info.assume_init() }.window;
    let width = i32::from(window.right) - i32::from(window.left) + 1;
    (width > 0).then_some(width as usize)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn stdout_columns() -> Option<usize> {
    None
}
#[cfg(unix)]
pub(super) fn hostname() -> Option<Vec<u8>> {
    let mut name = std::mem::MaybeUninit::<libc::utsname>::zeroed();
    if unsafe { libc::uname(name.as_mut_ptr()) } != 0 {
        return None;
    }
    let name = unsafe { name.assume_init() };
    Some(
        unsafe { std::ffi::CStr::from_ptr(name.nodename.as_ptr()) }
            .to_bytes()
            .to_vec(),
    )
}

#[cfg(windows)]
pub(super) fn hostname() -> Option<Vec<u8>> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetComputerNameExW(format: u32, buffer: *mut u16, size: *mut u32) -> i32;
        fn GetLastError() -> u32;
    }
    // Use the physical DNS hostname, matching Go's os.Hostname on Windows.
    let mut buffer = vec![0u16; 64];
    loop {
        let mut size = u32::try_from(buffer.len()).ok()?;
        if unsafe { GetComputerNameExW(5, buffer.as_mut_ptr(), &mut size) } != 0 {
            return Some(String::from_utf16_lossy(&buffer[..size as usize]).into_bytes());
        }
        if unsafe { GetLastError() } != 234 || size as usize <= buffer.len() {
            return None;
        }
        buffer.resize(size as usize, 0);
    }
}

#[cfg(not(any(unix, windows)))]
pub(super) fn hostname() -> Option<Vec<u8>> {
    None
}
