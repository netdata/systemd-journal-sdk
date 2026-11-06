use super::error::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use tokio::sync::mpsc;
use tracing::warn;

/// File system watcher that sends events through an async channel
#[derive(Debug)]
pub struct Monitor {
    /// The watcher instance
    watcher: RecommendedWatcher,
    /// Kept so the watcher can be rebuilt without changing the receiver
    event_sender: mpsc::UnboundedSender<Event>,
}

impl Monitor {
    /// Create a new monitor with an event receiver channel
    pub fn new() -> Result<(Self, mpsc::UnboundedReceiver<Event>)> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let watcher = Self::watcher(event_sender.clone())?;

        Ok((
            Self {
                watcher,
                event_sender,
            },
            event_receiver,
        ))
    }

    fn watcher(event_sender: mpsc::UnboundedSender<Event>) -> Result<RecommendedWatcher> {
        Ok(RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res {
                    if let Err(e) = event_sender.send(event) {
                        warn!("Failed to send file system event: receiver dropped ({})", e);
                    }
                }
            },
            notify::Config::default(),
        )?)
    }

    /// Start watching a directory recursively
    pub fn watch_directory(&mut self, path: &str) -> Result<()> {
        self.watcher
            .watch(Path::new(path), RecursiveMode::Recursive)?;

        Ok(())
    }

    /// Stop watching a directory
    pub fn unwatch_directory(&mut self, path: &str) -> Result<()> {
        self.watcher.unwatch(Path::new(path))?;
        Ok(())
    }

    /// Replace the watcher with a fresh one that watches nothing, keeping the
    /// same event receiver.
    ///
    /// notify 8.2 (inotify) keeps a stale entry when a directory under a
    /// recursive watch is deleted, and every later unwatch of a directory
    /// above it fails with `EINVAL`; a fresh watcher is the only way out.
    pub(crate) fn reset(&mut self) -> Result<()> {
        self.watcher = Self::watcher(self.event_sender.clone())?;
        Ok(())
    }
}
