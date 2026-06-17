use crate::{LoadOptions, LocalJournalProvider};
use std::io;

pub(crate) fn load(_options: LoadOptions) -> io::Result<LocalJournalProvider> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "journal host helper is not supported on this platform",
    ))
}
