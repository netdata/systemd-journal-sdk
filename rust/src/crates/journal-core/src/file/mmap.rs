use crate::error::{JournalError, Result};
use journal_common::compat::is_multiple_of;
use std::fs::File;
#[cfg(not(unix))]
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::{Deref, DerefMut};
#[cfg(unix)]
use std::os::unix::fs::FileExt;
use std::sync::atomic::{Ordering, fence};
use tracing::error;

// Re-export memmap2 types for other crates and import for internal use
pub use memmap2::{Mmap, MmapMut, MmapOptions};

const PAGE_SIZE: u64 = 4096;

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExperimentalMmapStrategy {
    #[default]
    Windowed,
    WholeFile,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowManagerStats {
    pub strategy: ExperimentalMmapStrategy,
    pub file_size: u64,
    pub window_count: usize,
    pub row_pin_count: usize,
    pub row_pin_limit: usize,
    pub row_overflow_object_count: usize,
    pub current_mapped_bytes: u64,
    pub max_mapped_bytes: u64,
    pub map_count: u64,
    pub remap_count: u64,
    pub eviction_count: u64,
}

pub trait MemoryMap: Deref<Target = [u8]> {
    fn create(file: &File, offset: u64, size: u64) -> Result<Self>
    where
        Self: Sized;

    fn create_checked(file: &File, offset: u64, size: u64, file_size: u64) -> Result<Self>
    where
        Self: Sized,
    {
        let _ = file_size;
        Self::create(file, offset, size)
    }
}

pub trait MemoryMapMut: MemoryMap + DerefMut {
    /// Flushes outstanding memory map modifications to disk
    fn flush(&self) -> Result<()>;
}

impl MemoryMap for Mmap {
    fn create(file: &File, offset: u64, size: u64) -> Result<Self> {
        let end = offset
            .checked_add(size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let file_size = file.metadata()?.len();
        if end > file_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Self::create_checked(file, offset, size, file_size)
    }

    fn create_checked(file: &File, offset: u64, size: u64, file_size: u64) -> Result<Self> {
        let end = offset
            .checked_add(size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        if end > file_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        // SAFETY: `offset + size` was checked against the current file size
        // above, so this read-only mapping stays within file bounds.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let mmap = unsafe {
            MmapOptions::new()
                .offset(offset)
                .len(size as usize)
                .map(file)?
        };

        Ok(mmap)
    }
}

impl MemoryMap for MmapMut {
    fn create(file: &File, offset: u64, size: u64) -> Result<Self> {
        let required_size = offset
            .checked_add(size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;

        let mut file_size = file.metadata()?.len();
        if required_size > file_size {
            file.set_len(required_size)?;
            file_size = required_size;
        }
        Self::create_checked(file, offset, size, file_size)
    }

    fn create_checked(file: &File, offset: u64, size: u64, file_size: u64) -> Result<Self> {
        let required_size = offset
            .checked_add(size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        if required_size > file_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }

        // SAFETY: `required_size` was checked against the file size above, and
        // `create` extends the file before calling this checked constructor.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let mmap = unsafe {
            MmapOptions::new()
                .offset(offset)
                .len(size as usize)
                .map_mut(file)?
        };

        Ok(mmap)
    }
}

impl MemoryMapMut for MmapMut {
    fn flush(&self) -> Result<()> {
        MmapMut::flush(self)?;
        Ok(())
    }
}

struct Window<M: MemoryMap> {
    offset: u64,
    size: u64,
    mmap: M,
    row_pinned: bool,
}

impl<M: MemoryMap> std::fmt::Debug for Window<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("offset", &self.offset)
            .field("size", &self.size)
            .finish()
    }
}

impl<M: MemoryMap> Window<M> {
    fn end_offset(&self) -> Option<u64> {
        self.offset.checked_add(self.size)
    }

    fn contains(&self, position: u64) -> bool {
        self.end_offset()
            .is_some_and(|end_offset| position >= self.offset && position < end_offset)
    }

    fn contains_range(&self, position: u64, size: u64) -> bool {
        let Some(end) = position.checked_add(size) else {
            return false;
        };
        self.end_offset()
            .is_some_and(|end_offset| position >= self.offset && end <= end_offset)
    }

    fn get_slice(&self, position: u64, size: u64) -> &[u8] {
        debug_assert!(self.contains_range(position, size));

        let offset = (position - self.offset) as usize;
        &self.mmap[offset..offset + size as usize]
    }
}

impl<M: MemoryMapMut> Window<M> {
    pub fn get_mut_slice(&mut self, position: u64, size: u64) -> &mut [u8] {
        debug_assert!(self.contains_range(position, size));

        let offset = (position - self.offset) as usize;
        &mut self.mmap[offset..offset + size as usize]
    }
}

pub struct WindowManager<M: MemoryMap> {
    file: File,
    file_size: u64,
    bounds_mode: BoundsMode,
    strategy: ExperimentalMmapStrategy,
    chunk_size: u64,
    active_window_idx: Option<usize>,
    max_windows: usize,
    windows: Vec<Window<M>>,
    row_pin_count: usize,
    map_count: u64,
    remap_count: u64,
    eviction_count: u64,
    max_mapped_bytes: u64,
    row_overflow_objects: Vec<Box<[u8]>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundsMode {
    LiveFile,
    Snapshot,
    WriterOwned,
}

impl<M: MemoryMap> WindowManager<M> {
    pub fn new(file: File, chunk_size: u64, max_windows: usize) -> Result<Self> {
        Self::new_with_strategy(
            file,
            chunk_size,
            max_windows,
            ExperimentalMmapStrategy::Windowed,
        )
    }

    pub fn new_with_strategy(
        file: File,
        chunk_size: u64,
        max_windows: usize,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        Self::new_with_bounds_mode(
            file,
            chunk_size,
            max_windows,
            BoundsMode::LiveFile,
            strategy,
        )
    }

    pub fn new_snapshot(
        file: File,
        chunk_size: u64,
        max_windows: usize,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        Self::new_with_bounds_mode(
            file,
            chunk_size,
            max_windows,
            BoundsMode::Snapshot,
            strategy,
        )
    }

    pub fn new_writer_owned(file: File, chunk_size: u64, max_windows: usize) -> Result<Self> {
        Self::new_writer_owned_with_strategy(
            file,
            chunk_size,
            max_windows,
            ExperimentalMmapStrategy::Windowed,
        )
    }

    pub fn new_writer_owned_with_strategy(
        file: File,
        chunk_size: u64,
        max_windows: usize,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        Self::new_with_bounds_mode(
            file,
            chunk_size,
            max_windows,
            BoundsMode::WriterOwned,
            strategy,
        )
    }

    fn new_with_bounds_mode(
        file: File,
        chunk_size: u64,
        max_windows: usize,
        bounds_mode: BoundsMode,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        debug_assert!(chunk_size != 0 && is_multiple_of(chunk_size, PAGE_SIZE));
        debug_assert!(max_windows != 0);

        let file_size = file.metadata()?.len();

        Ok(WindowManager {
            file,
            file_size,
            bounds_mode,
            strategy,
            chunk_size,
            max_windows,
            windows: Vec::new(),
            row_pin_count: 0,
            active_window_idx: None,
            map_count: 0,
            remap_count: 0,
            eviction_count: 0,
            max_mapped_bytes: 0,
            row_overflow_objects: Vec::new(),
        })
    }

    pub fn stats(&self) -> WindowManagerStats {
        let current_mapped_bytes = self.current_mapped_bytes();
        WindowManagerStats {
            strategy: self.strategy,
            file_size: self.file_size,
            window_count: self.windows.len(),
            row_pin_count: self.row_pin_count,
            row_pin_limit: self.max_windows,
            row_overflow_object_count: self.row_overflow_objects.len(),
            current_mapped_bytes,
            max_mapped_bytes: self.max_mapped_bytes.max(current_mapped_bytes),
            map_count: self.map_count,
            remap_count: self.remap_count,
            eviction_count: self.eviction_count,
        }
    }

    fn current_mapped_bytes(&self) -> u64 {
        self.windows.iter().map(|window| window.size).sum()
    }

    fn record_mapped_bytes(&mut self) {
        self.max_mapped_bytes = self.max_mapped_bytes.max(self.current_mapped_bytes());
    }

    fn refresh_file_size(&mut self) -> Result<u64> {
        self.file_size = self.file.metadata()?.len();
        Ok(self.file_size)
    }

    fn ensure_cached_file_contains(&mut self, end: u64) -> Result<()> {
        if end <= self.file_size {
            return Ok(());
        }
        if self.bounds_mode == BoundsMode::LiveFile && end <= self.refresh_file_size()? {
            return Ok(());
        }
        Err(JournalError::ObjectExceedsFileBounds)
    }

    pub(crate) fn read_exact_at(&mut self, position: u64, output: &mut [u8]) -> Result<()> {
        let end = position
            .checked_add(output.len() as u64)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        self.ensure_cached_file_contains(end)?;

        #[cfg(unix)]
        {
            let mut read = 0usize;
            while read < output.len() {
                let bytes_read = self
                    .file
                    .read_at(&mut output[read..], position + read as u64)?;
                if bytes_read == 0 {
                    return Err(JournalError::Io(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "short journal file read",
                    )));
                }
                read += bytes_read;
            }
        }

        #[cfg(not(unix))]
        {
            self.file.seek(SeekFrom::Start(position))?;
            self.file.read_exact(output)?;
        }

        Ok(())
    }

    fn get_chunk_aligned_start(&self, position: u64) -> u64 {
        (position / self.chunk_size) * self.chunk_size
    }

    fn get_chunk_aligned_end(&self, position: u64) -> Result<u64> {
        position
            .div_ceil(self.chunk_size)
            .checked_mul(self.chunk_size)
            .ok_or(JournalError::ObjectExceedsFileBounds)
    }

    fn create_window(&mut self, window_start: u64, chunk_count: u64) -> Result<Window<M>> {
        debug_assert_ne!(chunk_count, 0);

        let requested_size = chunk_count
            .checked_mul(self.chunk_size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let requested_end = window_start
            .checked_add(requested_size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let size = match self.bounds_mode {
            BoundsMode::LiveFile => {
                if window_start >= self.file_size {
                    self.refresh_file_size()?;
                }
                if window_start >= self.file_size {
                    return Err(JournalError::ObjectExceedsFileBounds);
                }
                requested_size.min(self.file_size - window_start)
            }
            BoundsMode::Snapshot => {
                if window_start >= self.file_size {
                    return Err(JournalError::ObjectExceedsFileBounds);
                }
                requested_size.min(self.file_size - window_start)
            }
            BoundsMode::WriterOwned => {
                if requested_end > self.file_size {
                    self.file.set_len(requested_end)?;
                    self.file_size = requested_end;
                }
                requested_size
            }
        };
        let mmap =
            M::create_checked(&self.file, window_start, size, self.file_size).map_err(|e| {
                error!(
                    window_start,
                    size,
                    chunk_count,
                    chunk_size = self.chunk_size,
                    "mmap failed: {e}"
                );
                e
            })?;
        self.map_count += 1;
        Ok(Window {
            offset: window_start,
            size,
            mmap,
            row_pinned: false,
        })
    }

    fn lookup_window_by_range(&self, position: u64, size_needed: u64) -> Option<usize> {
        if let Some(idx) = self.active_window_idx {
            if self.windows[idx].contains_range(position, size_needed) {
                return Some(idx);
            }
        }

        for (idx, window) in self.windows.iter().enumerate() {
            if window.contains_range(position, size_needed) {
                return Some(idx);
            }
        }

        None
    }

    fn lookup_window_by_position(&self, position: u64) -> Option<usize> {
        if let Some(idx) = self.active_window_idx {
            if self.windows[idx].contains(position) {
                return Some(idx);
            }
        }

        for (idx, window) in self.windows.iter().enumerate() {
            if window.contains(position) {
                return Some(idx);
            }
        }

        None
    }

    pub(crate) fn active_slice_if_contains(&self, position: u64, size: u64) -> Option<&[u8]> {
        let idx = self.active_window_idx?;
        let window = &self.windows[idx];
        if window.contains_range(position, size) {
            Some(window.get_slice(position, size))
        } else {
            None
        }
    }

    pub(crate) fn active_window_contains(&self, position: u64, size: u64) -> bool {
        self.active_window_idx
            .and_then(|idx| self.windows.get(idx))
            .is_some_and(|window| window.contains_range(position, size))
    }

    pub(crate) fn active_slice(&self, position: u64, size: u64) -> &[u8] {
        let idx = self
            .active_window_idx
            .expect("active window should exist when active_window_contains returned true");
        let window = &self.windows[idx];
        debug_assert!(window.contains_range(position, size));
        window.get_slice(position, size)
    }

    pub(crate) fn clear_row_pins(&mut self) {
        if self.row_pin_count == 0 {
            self.row_overflow_objects.clear();
            return;
        }
        for window in &mut self.windows {
            window.row_pinned = false;
        }
        self.row_pin_count = 0;
        self.row_overflow_objects.clear();
    }

    #[inline(always)]
    pub(crate) fn row_pin_limit_reached(&self) -> bool {
        self.strategy != ExperimentalMmapStrategy::WholeFile
            && self.row_pin_count >= self.max_windows
    }

    #[cold]
    #[inline(never)]
    fn get_row_overflow_slice(&mut self, position: u64, size: u64) -> Result<&[u8]> {
        let len = usize::try_from(size).map_err(|_| JournalError::ObjectExceedsFileBounds)?;
        let mut data = vec![0u8; len].into_boxed_slice();
        self.read_exact_at(position, &mut data)?;
        self.row_overflow_objects.push(data);
        Ok(self
            .row_overflow_objects
            .last()
            .expect("just pushed row overflow object")
            .as_ref())
    }

    pub(crate) fn get_row_pinned_slice(&mut self, position: u64, size: u64) -> Result<&[u8]> {
        let end = position
            .checked_add(size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        self.ensure_cached_file_contains(end)?;
        let Some(idx) = self.get_window_index_preserving_row_pins(position, size)? else {
            return self.get_row_overflow_slice(position, size);
        };
        self.active_window_idx = Some(idx);
        if !self.windows[idx].row_pinned {
            if self.row_pin_limit_reached() {
                return self.get_row_overflow_slice(position, size);
            }
            self.windows[idx].row_pinned = true;
            self.row_pin_count += 1;
        }
        let window = &mut self.windows[idx];
        Ok(window.get_slice(position, size))
    }

    fn push_window(&mut self, window: Window<M>) -> usize {
        self.windows.push(window);
        self.record_mapped_bytes();
        self.windows.len() - 1
    }

    fn get_window_index_preserving_row_pins(
        &mut self,
        position: u64,
        size_needed: u64,
    ) -> Result<Option<usize>> {
        if self.strategy == ExperimentalMmapStrategy::WholeFile {
            let was_unpinned = {
                let window = self.get_whole_file_window(position, size_needed)?;
                let was_unpinned = !window.row_pinned;
                window.row_pinned = true;
                was_unpinned
            };
            if was_unpinned {
                self.row_pin_count += 1;
            }
            return Ok(Some(0));
        }

        if let Some(idx) = self.lookup_window_by_range(position, size_needed) {
            return Ok(Some(idx));
        }

        let range_end = position
            .checked_add(size_needed)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let window_start = self.get_chunk_aligned_start(position);
        let window_end = self.get_chunk_aligned_end(range_end)?;
        let num_chunks = (window_end - window_start) / self.chunk_size;

        if let Some(idx) = self.lookup_window_by_position(position) {
            if !self.windows[idx].row_pinned {
                if self.row_pin_limit_reached() {
                    return Ok(None);
                }
                // The overlapping window is not pinned, so no current-row
                // payload can point into it. Replace it with a wider window;
                // get_row_pinned_slice() pins the replacement before returning
                // borrowed bytes to the reader.
                let _window = self.windows.remove(idx);
                self.active_window_idx = None;
                let new_window = self.create_window(window_start, num_chunks)?;
                self.remap_count += 1;
                return Ok(Some(self.push_window(new_window)));
            }
            // The pinned window contains the requested start but not the full
            // requested range; lookup_window_by_range would have matched
            // otherwise. Do not remap it because existing row slices may point
            // into it. Map a wider overlapping window for this row instead.
        }

        if self.windows.len() >= self.max_windows {
            if let Some(idx) = self.windows.iter().position(|window| !window.row_pinned) {
                self.windows.remove(idx);
                self.eviction_count += 1;
                self.active_window_idx = None;
            } else {
                return Ok(None);
            }
        }

        let new_window = self.create_window(window_start, num_chunks)?;
        Ok(Some(self.push_window(new_window)))
    }

    fn get_window(&mut self, position: u64, size_needed: u64) -> Result<&mut Window<M>> {
        if self.strategy == ExperimentalMmapStrategy::WholeFile {
            return self.get_whole_file_window(position, size_needed);
        }

        if let Some(idx) = self.lookup_window_by_range(position, size_needed) {
            // Use the existing window
            self.active_window_idx = Some(idx);
            Ok(&mut self.windows[idx])
        } else if let Some(idx) = self.lookup_window_by_position(position) {
            if self.row_pin_count > 0 && self.windows[idx].row_pinned {
                let range_end = position
                    .checked_add(size_needed)
                    .ok_or(JournalError::ObjectExceedsFileBounds)?;
                let window_start = self.get_chunk_aligned_start(position);
                let window_end = self.get_chunk_aligned_end(range_end)?;
                let num_chunks = (window_end - window_start) / self.chunk_size;

                if self.windows.len() >= self.max_windows {
                    if let Some(evict_idx) =
                        self.windows.iter().position(|window| !window.row_pinned)
                    {
                        self.windows.remove(evict_idx);
                        self.eviction_count += 1;
                        self.active_window_idx = None;
                    }
                }

                // If every cached window is row-pinned, retain row-valid
                // payloads and use one replaceable transient window for this
                // immediate non-row access. Later non-row accesses evict that
                // unpinned transient window instead of growing with the row.
                let new_window = self.create_window(window_start, num_chunks)?;
                let idx = self.push_window(new_window);
                self.active_window_idx = Some(idx);
                return Ok(&mut self.windows[idx]);
            }

            // Remap the window

            let _window = self.windows.remove(idx);
            // Invalidate active_window_idx before removal to maintain consistency.
            // If create_window fails, the index won't point to a non-existent window.
            self.active_window_idx = None;

            // Keep the remapped window chunk-aligned around the requested
            // position instead of preserving the old window start. Preserving
            // the old start lets sequential append access grow one mapping from
            // the beginning of the file toward the tail, which defeats the
            // intended bounded-window model.
            let range_end = position
                .checked_add(size_needed)
                .ok_or(JournalError::ObjectExceedsFileBounds)?;
            let window_start = self.get_chunk_aligned_start(position);
            let window_end = self.get_chunk_aligned_end(range_end)?;
            let num_chunks = (window_end - window_start) / self.chunk_size;

            let new_window = self.create_window(window_start, num_chunks)?;

            self.remap_count += 1;
            self.windows.push(new_window);
            self.record_mapped_bytes();
            self.active_window_idx = Some(self.windows.len() - 1);
            Ok(self.windows.last_mut().unwrap())
        } else {
            // Create a brand new window

            if self.windows.len() >= self.max_windows {
                if self.row_pin_count == 0 {
                    let idx = if self.active_window_idx == Some(0) && self.windows.len() > 1 {
                        1
                    } else {
                        0
                    };
                    self.windows.remove(idx);
                    self.eviction_count += 1;
                    // Invalidate active_window_idx after removal to maintain consistency.
                    // If create_window fails below, the index won't point to a non-existent window.
                    self.active_window_idx = None;
                } else if let Some(idx) = self.windows.iter().position(|window| !window.row_pinned)
                {
                    self.windows.remove(idx);
                    self.eviction_count += 1;
                    // Invalidate active_window_idx after removal to maintain consistency.
                    // If create_window fails below, the index won't point to a non-existent window.
                    self.active_window_idx = None;
                }
            }

            {
                // Calculate window start for this position
                let range_end = position
                    .checked_add(size_needed)
                    .ok_or(JournalError::ObjectExceedsFileBounds)?;
                let window_start = self.get_chunk_aligned_start(position);
                let window_end = self.get_chunk_aligned_end(range_end)?;
                let num_chunks = (window_end - window_start) / self.chunk_size;

                let new_window = self.create_window(window_start, num_chunks)?;

                self.windows.push(new_window);
                self.record_mapped_bytes();
            }

            self.active_window_idx = Some(self.windows.len() - 1);
            Ok(self.windows.last_mut().unwrap())
        }
    }

    fn get_whole_file_window(&mut self, position: u64, size_needed: u64) -> Result<&mut Window<M>> {
        if let Some(idx) = self.lookup_window_by_range(position, size_needed) {
            self.active_window_idx = Some(idx);
            return Ok(&mut self.windows[idx]);
        }

        let requested_end = position
            .checked_add(size_needed)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        match self.bounds_mode {
            BoundsMode::LiveFile | BoundsMode::Snapshot => {
                self.ensure_cached_file_contains(requested_end)?
            }
            BoundsMode::WriterOwned => {}
        }
        let target_end = requested_end.max(self.file_size);
        let window_end = self.get_chunk_aligned_end(target_end)?;
        let chunk_count = (window_end / self.chunk_size).max(1);

        let had_windows = !self.windows.is_empty();
        if had_windows {
            self.windows.clear();
            self.active_window_idx = None;
            self.row_pin_count = 0;
            self.row_overflow_objects.clear();
        }

        let new_window = self.create_window(0, chunk_count)?;
        if had_windows {
            self.remap_count += 1;
        }
        self.windows.push(new_window);
        self.record_mapped_bytes();
        self.active_window_idx = Some(0);
        Ok(&mut self.windows[0])
    }

    pub fn get_slice(&mut self, position: u64, size: u64) -> Result<&[u8]> {
        let end = position
            .checked_add(size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        self.ensure_cached_file_contains(end)?;
        let window = self.get_window(position, size)?;
        Ok(window.get_slice(position, size))
    }
}

impl<M: MemoryMapMut> WindowManager<M> {
    pub fn get_slice_mut(&mut self, position: u64, size: u64) -> Result<&mut [u8]> {
        let _end = position
            .checked_add(size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let window = self.get_window(position, size)?;
        Ok(window.get_mut_slice(position, size))
    }

    /// Syncs all file data to disk
    pub fn sync(&mut self, logical_size: u64, header_bytes: &[u8]) -> Result<()> {
        for window in &self.windows {
            window.mmap.flush()?;
        }
        self.windows.clear();
        self.active_window_idx = None;
        self.row_pin_count = 0;
        self.row_overflow_objects.clear();
        self.file.set_len(logical_size)?;
        #[cfg(unix)]
        {
            let mut written = 0usize;
            while written < header_bytes.len() {
                written += self
                    .file
                    .write_at(&header_bytes[written..], written as u64)?;
            }
        }
        #[cfg(not(unix))]
        {
            self.file.seek(SeekFrom::Start(0))?;
            self.file.write_all(header_bytes)?;
        }
        self.file.sync_data()?;
        self.file_size = logical_size;
        Ok(())
    }

    /// Publish mmap writes to stock follow readers by triggering an inotify
    /// event with the same-size truncate used by systemd.
    pub fn post_change(&mut self, logical_size: u64) -> Result<()> {
        fence(Ordering::SeqCst);
        if logical_size < self.file_size {
            self.windows.clear();
            self.active_window_idx = None;
            self.row_pin_count = 0;
            self.row_overflow_objects.clear();
        }
        self.file.set_len(logical_size)?;
        self.file_size = logical_size;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::JournalError;
    use std::cell::Cell;
    use std::io::Write;
    use std::rc::Rc;
    use tempfile::NamedTempFile;

    const PAGE_SIZE_TEST: u64 = 4096;

    /// A mock MemoryMap that can be configured to fail on specific calls.
    /// This allows us to test error handling in WindowManager.
    struct FailingMmap {
        data: Vec<u8>,
    }

    impl Deref for FailingMmap {
        type Target = [u8];
        fn deref(&self) -> &[u8] {
            &self.data
        }
    }

    /// Shared state to control when the mock should fail
    struct MockController {
        fail_next_create: Cell<bool>,
        create_count: Cell<usize>,
    }

    impl MockController {
        fn new() -> Self {
            Self {
                fail_next_create: Cell::new(false),
                create_count: Cell::new(0),
            }
        }

        fn set_fail_next(&self, fail: bool) {
            self.fail_next_create.set(fail);
        }

        fn should_fail(&self) -> bool {
            let count = self.create_count.get();
            self.create_count.set(count + 1);
            self.fail_next_create.get()
        }
    }

    // Thread-local controller for the mock
    thread_local! {
        static MOCK_CONTROLLER: Rc<MockController> = Rc::new(MockController::new());
    }

    impl MemoryMap for FailingMmap {
        fn create(_file: &File, _offset: u64, size: u64) -> Result<Self> {
            let mmap_size = size as usize;
            MOCK_CONTROLLER.with(|ctrl| {
                if ctrl.should_fail() {
                    return Err(JournalError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "simulated mmap failure",
                    )));
                }
                // Create a mock mmap with zeros
                Ok(FailingMmap {
                    data: vec![0u8; mmap_size],
                })
            })
        }
    }

    /// This test verifies that WindowManager maintains consistent state
    /// after a failed remap operation.
    ///
    /// The scenario:
    /// 1. A window exists at some position
    /// 2. A request comes in that requires remapping (position in window, but size extends beyond)
    /// 3. The old window is removed
    /// 4. Creating the new (larger) window fails (e.g., mmap error)
    /// 5. The WindowManager should remain in a consistent state
    /// 6. Subsequent operations should not panic
    #[test]
    fn test_consistent_state_after_failed_remap() {
        // Create a temporary file (content doesn't matter for mock)
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&[0u8; 8192]).unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();

        // Create WindowManager with mock mmap, 4KB chunks, max 1 window
        let mut wm: WindowManager<FailingMmap> =
            WindowManager::new(file, PAGE_SIZE_TEST, 1).unwrap();

        // Reset controller state
        MOCK_CONTROLLER.with(|ctrl| {
            ctrl.set_fail_next(false);
            ctrl.create_count.set(0);
        });

        // First read: creates a window at offset 0, size 4KB (this should succeed)
        {
            let slice = wm.get_slice(0, 100).unwrap();
            assert_eq!(slice.len(), 100);
        }
        assert_eq!(wm.windows.len(), 1);
        assert_eq!(wm.active_window_idx, Some(0));

        // Configure mock to fail on the next create call
        MOCK_CONTROLLER.with(|ctrl| ctrl.set_fail_next(true));

        // Request a slice that requires remapping:
        // - Position 100 is within the existing window [0, 4096)
        // - But size 4000 means we need bytes [100, 4100), which extends beyond window
        // - This triggers the "Remap the window" branch
        // - The old window is removed
        // - Then create_window is called and FAILS
        let remap_result = wm.get_slice(100, 4000);
        assert!(remap_result.is_err(), "Expected remap to fail");

        // Verify state is consistent after the failure:
        // - windows is empty (the old window was removed, new one failed to create)
        // - active_window_idx should be None (not pointing to non-existent window)
        assert_eq!(wm.windows.len(), 0);
        assert_eq!(wm.active_window_idx, None);

        // Allow the next create to succeed
        MOCK_CONTROLLER.with(|ctrl| ctrl.set_fail_next(false));

        // The next operation should NOT panic - it should succeed by creating a new window
        let result = wm.get_slice(0, 100);
        assert!(
            result.is_ok(),
            "Expected get_slice to succeed after recovery"
        );
        assert_eq!(wm.windows.len(), 1);
    }

    /// This test verifies that WindowManager maintains consistent state
    /// after a failed window creation in the eviction path.
    ///
    /// The scenario:
    /// 1. A window exists and we're at max_windows
    /// 2. A request comes in for a different region requiring a new window
    /// 3. The old window is evicted to make room
    /// 4. Creating the new window fails (e.g., mmap error)
    /// 5. The WindowManager should remain in a consistent state
    /// 6. Subsequent operations should not panic
    #[test]
    fn test_consistent_state_after_failed_eviction() {
        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&[0u8; 8192]).unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();

        // Create WindowManager with mock mmap, 4KB chunks, max 1 window
        let mut wm: WindowManager<FailingMmap> =
            WindowManager::new(file, PAGE_SIZE_TEST, 1).unwrap();

        // Reset controller state
        MOCK_CONTROLLER.with(|ctrl| {
            ctrl.set_fail_next(false);
            ctrl.create_count.set(0);
        });

        // Create first window at offset 0
        {
            let _slice = wm.get_slice(0, 100).unwrap();
        }
        assert_eq!(wm.windows.len(), 1);
        assert_eq!(wm.active_window_idx, Some(0));

        // Configure mock to fail on the next create call
        MOCK_CONTROLLER.with(|ctrl| ctrl.set_fail_next(true));

        // Request a slice at a completely different position (second page)
        // This triggers:
        // - lookup_window_by_range returns None (position 4096 not in window [0, 4096))
        // - lookup_window_by_position returns None
        // - "Create a brand new window" branch
        // - Eviction: windows.remove(0) since we're at max_windows
        // - create_window fails
        let result = wm.get_slice(4096, 100);
        assert!(result.is_err(), "Expected mmap to fail");

        // Verify state is consistent after the failure:
        // - windows is empty (the old window was evicted, new one failed to create)
        // - active_window_idx should be None (not pointing to non-existent window)
        assert_eq!(wm.windows.len(), 0);
        assert_eq!(wm.active_window_idx, None);

        // Allow the next create to succeed
        MOCK_CONTROLLER.with(|ctrl| ctrl.set_fail_next(false));

        // The next operation should NOT panic - it should succeed by creating a new window
        let result = wm.get_slice(0, 100);
        assert!(
            result.is_ok(),
            "Expected get_slice to succeed after recovery"
        );
        assert_eq!(wm.windows.len(), 1);
    }

    #[test]
    fn row_pinned_slice_uses_overflow_storage_at_window_limit_one() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(&vec![1u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file
            .write_all(&vec![2u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();
        let mut wm: WindowManager<Mmap> = WindowManager::new(file, PAGE_SIZE_TEST, 1).unwrap();

        let first = wm.get_row_pinned_slice(0, 16).unwrap();
        let first_ptr = first.as_ptr();
        let first_len = first.len();
        assert_eq!(first, &[1u8; 16]);

        let second = wm.get_row_pinned_slice(PAGE_SIZE_TEST, 16).unwrap();
        assert_eq!(second, &[2u8; 16]);

        let stats = wm.stats();
        assert_eq!(stats.row_pin_limit, 1);
        assert_eq!(stats.row_pin_count, 1);
        assert_eq!(stats.window_count, 1);
        assert_eq!(stats.row_overflow_object_count, 1);

        // SAFETY: The first slice points into a row-pinned mmap window. The
        // second access forced overflow storage, but it must not unmap the
        // first row-pinned window.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let first_after_overflow = unsafe { std::slice::from_raw_parts(first_ptr, first_len) };
        assert_eq!(first_after_overflow, &[1u8; 16]);

        wm.clear_row_pins();
        let stats = wm.stats();
        assert_eq!(stats.row_pin_count, 0);
        assert_eq!(stats.row_overflow_object_count, 0);
    }

    #[test]
    fn live_reader_refreshes_file_size_only_when_access_exceeds_cache() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(&vec![1u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();
        let mut wm: WindowManager<Mmap> = WindowManager::new(file, PAGE_SIZE_TEST, 2).unwrap();
        assert_eq!(wm.stats().file_size, PAGE_SIZE_TEST);

        assert_eq!(wm.get_slice(0, 16).unwrap(), &[1u8; 16]);

        temp_file
            .write_all(&vec![2u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        assert_eq!(wm.get_slice(128, 16).unwrap(), &[1u8; 16]);
        assert_eq!(wm.stats().file_size, PAGE_SIZE_TEST);

        assert_eq!(wm.get_slice(PAGE_SIZE_TEST + 128, 16).unwrap(), &[2u8; 16]);
        assert_eq!(wm.stats().file_size, PAGE_SIZE_TEST * 2);
    }

    #[test]
    fn snapshot_reader_does_not_refresh_file_size_after_growth() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(&vec![1u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();
        let mut wm: WindowManager<Mmap> = WindowManager::new_snapshot(
            file,
            PAGE_SIZE_TEST,
            2,
            ExperimentalMmapStrategy::Windowed,
        )
        .unwrap();
        assert_eq!(wm.stats().file_size, PAGE_SIZE_TEST);

        temp_file
            .write_all(&vec![2u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        assert!(matches!(
            wm.get_slice(PAGE_SIZE_TEST + 128, 16).unwrap_err(),
            JournalError::ObjectExceedsFileBounds
        ));
        assert_eq!(wm.stats().file_size, PAGE_SIZE_TEST);
    }

    #[test]
    fn snapshot_whole_file_maps_cached_file_once() {
        let temp_file = NamedTempFile::new().unwrap();
        temp_file.as_file().set_len(PAGE_SIZE_TEST * 2).unwrap();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(temp_file.path())
            .unwrap();
        let mut wm: WindowManager<Mmap> = WindowManager::new_snapshot(
            file,
            PAGE_SIZE_TEST,
            32,
            ExperimentalMmapStrategy::WholeFile,
        )
        .unwrap();

        assert_eq!(wm.get_slice(PAGE_SIZE_TEST + 128, 16).unwrap(), &[0; 16]);
        assert_eq!(wm.get_slice(128, 16).unwrap(), &[0; 16]);

        let stats = wm.stats();
        assert_eq!(stats.strategy, ExperimentalMmapStrategy::WholeFile);
        assert_eq!(stats.file_size, PAGE_SIZE_TEST * 2);
        assert_eq!(stats.current_mapped_bytes, PAGE_SIZE_TEST * 2);
        assert_eq!(stats.map_count, 1);
        assert_eq!(stats.remap_count, 0);
    }

    #[test]
    fn snapshot_whole_file_does_not_refresh_file_size_after_growth() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(&vec![1u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();
        let mut wm: WindowManager<Mmap> = WindowManager::new_snapshot(
            file,
            PAGE_SIZE_TEST,
            32,
            ExperimentalMmapStrategy::WholeFile,
        )
        .unwrap();
        assert_eq!(wm.get_slice(128, 16).unwrap(), &[1u8; 16]);

        temp_file
            .write_all(&vec![2u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        assert!(matches!(
            wm.get_slice(PAGE_SIZE_TEST + 128, 16).unwrap_err(),
            JournalError::ObjectExceedsFileBounds
        ));
        assert_eq!(wm.stats().file_size, PAGE_SIZE_TEST);
    }

    #[test]
    fn live_whole_file_maps_cached_file_once_and_remaps_on_growth() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(&vec![1u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();
        let mut wm: WindowManager<Mmap> = WindowManager::new_with_strategy(
            file,
            PAGE_SIZE_TEST,
            32,
            ExperimentalMmapStrategy::WholeFile,
        )
        .unwrap();

        assert_eq!(wm.get_slice(128, 16).unwrap(), &[1u8; 16]);
        let stats = wm.stats();
        assert_eq!(stats.strategy, ExperimentalMmapStrategy::WholeFile);
        assert_eq!(stats.file_size, PAGE_SIZE_TEST);
        assert_eq!(stats.current_mapped_bytes, PAGE_SIZE_TEST);
        assert_eq!(stats.map_count, 1);
        assert_eq!(stats.remap_count, 0);

        temp_file
            .write_all(&vec![2u8; PAGE_SIZE_TEST as usize])
            .unwrap();
        temp_file.flush().unwrap();

        assert_eq!(wm.get_slice(256, 16).unwrap(), &[1u8; 16]);
        let stats = wm.stats();
        assert_eq!(stats.file_size, PAGE_SIZE_TEST);
        assert_eq!(stats.map_count, 1);
        assert_eq!(stats.remap_count, 0);

        assert_eq!(wm.get_slice(PAGE_SIZE_TEST + 128, 16).unwrap(), &[2u8; 16]);
        let stats = wm.stats();
        assert_eq!(stats.file_size, PAGE_SIZE_TEST * 2);
        assert_eq!(stats.current_mapped_bytes, PAGE_SIZE_TEST * 2);
        assert_eq!(stats.map_count, 2);
        assert_eq!(stats.remap_count, 1);
    }

    #[test]
    fn whole_file_writer_owned_remaps_after_post_change_growth() {
        let temp_file = NamedTempFile::new().unwrap();
        temp_file.as_file().set_len(PAGE_SIZE_TEST).unwrap();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp_file.path())
            .unwrap();
        let mut wm: WindowManager<MmapMut> = WindowManager::new_writer_owned_with_strategy(
            file,
            PAGE_SIZE_TEST,
            32,
            ExperimentalMmapStrategy::WholeFile,
        )
        .unwrap();

        wm.get_slice_mut(0, 16).unwrap().copy_from_slice(&[1; 16]);
        assert_eq!(wm.stats().current_mapped_bytes, PAGE_SIZE_TEST);

        wm.post_change(PAGE_SIZE_TEST * 2).unwrap();
        assert_eq!(wm.get_slice(0, 16).unwrap(), &[1; 16]);

        let new_offset = PAGE_SIZE_TEST + 128;
        wm.get_slice_mut(new_offset, 16)
            .unwrap()
            .copy_from_slice(&[2; 16]);
        assert_eq!(wm.get_slice(new_offset, 16).unwrap(), &[2; 16]);

        let stats = wm.stats();
        assert_eq!(stats.current_mapped_bytes, PAGE_SIZE_TEST * 2);
        assert_eq!(stats.max_mapped_bytes, PAGE_SIZE_TEST * 2);
        assert_eq!(stats.remap_count, 1);
    }

    #[test]
    fn post_change_drops_mappings_before_truncating_oversized_windows() {
        let temp_file = NamedTempFile::new().unwrap();
        temp_file.as_file().set_len(PAGE_SIZE_TEST).unwrap();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp_file.path())
            .unwrap();
        let oversized_window = PAGE_SIZE_TEST * 4;
        let mut wm: WindowManager<MmapMut> = WindowManager::new_writer_owned_with_strategy(
            file,
            oversized_window,
            32,
            ExperimentalMmapStrategy::WholeFile,
        )
        .unwrap();

        wm.get_slice_mut(0, 16).unwrap().copy_from_slice(&[1; 16]);
        assert_eq!(wm.stats().current_mapped_bytes, oversized_window);

        wm.post_change(PAGE_SIZE_TEST * 2).unwrap();
        let stats_after_truncate = wm.stats();
        assert_eq!(stats_after_truncate.file_size, PAGE_SIZE_TEST * 2);
        assert_eq!(stats_after_truncate.current_mapped_bytes, 0);
        assert_eq!(stats_after_truncate.window_count, 0);

        let crossing_offset = PAGE_SIZE_TEST * 2 - 1024;
        let crossing_payload = vec![2; 2048];
        wm.get_slice_mut(crossing_offset, 2048)
            .unwrap()
            .copy_from_slice(&crossing_payload);
        assert_eq!(
            wm.get_slice(crossing_offset, 2048).unwrap(),
            crossing_payload.as_slice()
        );
        assert_eq!(wm.stats().current_mapped_bytes, oversized_window);
    }

    #[test]
    fn sequential_boundary_crossing_slides_window_instead_of_growing_from_start() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&[0u8; 64 * 1024]).unwrap();
        temp_file.flush().unwrap();

        let file = File::open(temp_file.path()).unwrap();
        let mut wm: WindowManager<FailingMmap> =
            WindowManager::new(file, PAGE_SIZE_TEST, 1).unwrap();

        MOCK_CONTROLLER.with(|ctrl| {
            ctrl.set_fail_next(false);
            ctrl.create_count.set(0);
        });

        let _ = wm.get_slice(0, 100).unwrap();
        assert_eq!(wm.windows[0].offset, 0);
        assert_eq!(wm.windows[0].size, PAGE_SIZE_TEST);

        let _ = wm.get_slice(PAGE_SIZE_TEST - 6, 32).unwrap();
        assert_eq!(wm.windows[0].offset, 0);
        assert_eq!(wm.windows[0].size, PAGE_SIZE_TEST * 2);

        let _ = wm.get_slice((PAGE_SIZE_TEST * 2) - 12, 32).unwrap();
        assert_eq!(wm.windows[0].offset, PAGE_SIZE_TEST);
        assert_eq!(wm.windows[0].size, PAGE_SIZE_TEST * 2);

        let _ = wm.get_slice((PAGE_SIZE_TEST * 3) - 20, 32).unwrap();
        assert_eq!(wm.windows[0].offset, PAGE_SIZE_TEST * 2);
        assert_eq!(wm.windows[0].size, PAGE_SIZE_TEST * 2);
    }
}
