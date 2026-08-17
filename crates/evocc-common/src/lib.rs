#![allow(clippy::must_use_candidate)]
//! Diagnostics, source-span tracking, and arena allocation.
//!
//! These are the compiler's shared backbone. Every component that produces
//! user-visible diagnostics, owns heap-allocated IR nodes, or records source
//! locations crosses through this crate.
//!
//! Crate diagram (dependencies up):
//!
//! ```text
//!   evocc-lex  evocc-chir  evocc
//!       │          │         │
//!       └──────────┼─────────┘
//!                  │
//!            evocc-common
//! ```

// ---------------------------------------------------------------------------
// SourceSpan and SourceFile
// ---------------------------------------------------------------------------

/// Opaque handle into a per-compilation file table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl FileId {
    pub const DUMMY: Self = FileId(0);

    #[must_use]
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// Location in a source file: byte range for editor integration, line/column
/// for human-readable messages. The primary identity of a span is its file
/// and byte offsets; line/column are derived, not canonical.
///
/// Zero-width spans (byte_start == byte_end) are legal; they point at
/// positions (e.g. between tokens) rather than ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceSpan {
    pub file: FileId,
    pub byte_start: u32,
    pub byte_end: u32,
    pub line_start: u32,
    pub column_start: u32,
}

impl SourceSpan {
    #[must_use]
    pub fn new(
        file: FileId,
        byte_start: u32,
        byte_end: u32,
        line_start: u32,
        column_start: u32,
    ) -> Self {
        assert!(byte_start <= byte_end);
        Self {
            file,
            byte_start,
            byte_end,
            line_start,
            column_start,
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.file != FileId::DUMMY && self.byte_start <= self.byte_end
    }
}

/// Logical source file known to the compiler.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub path: String,
    pub contents: String,
    /// Byte offsets of line starts for fast offset-to-line-column conversion.
    pub line_offsets: Vec<usize>,
}

impl SourceFile {
    #[must_use]
    pub fn new(id: FileId, path: String, contents: String) -> Self {
        let mut line_offsets = vec![0];
        for (i, &b) in contents.as_bytes().iter().enumerate() {
            if b == b'\n' {
                line_offsets.push(i + 1);
            }
        }
        Self {
            id,
            path,
            contents,
            line_offsets,
        }
    }

    /// Convert a zero-indexed byte offset to a human-readable (1-indexed) line and column.
    #[must_use]
    pub fn line_col(&self, offset: usize) -> (u32, u32) {
        // partition_point returns the count of offsets ≤ `offset`.
        // The line containing `offset` is at index (count - 1): the last
        // line whose start offset is ≤ the queried offset.
        let count_le = self.line_offsets.partition_point(|&lo| lo <= offset);
        let line_idx = count_le.saturating_sub(1);
        let line_start = self.line_offsets.get(line_idx).copied().unwrap_or(0);
        let col = offset.saturating_sub(line_start);
        (line_idx as u32 + 1, col as u32 + 1)
    }
}

// ---------------------------------------------------------------------------
// Diagnostic system
// ---------------------------------------------------------------------------

/// Severity of a diagnostic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// One diagnostic message, carrying a severity, source location, and text.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: SourceSpan,
    pub message: String,
}

impl Diagnostic {
    #[must_use]
    pub fn error(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            span,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn warning(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            span,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn note(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Note,
            span,
            message: message.into(),
        }
    }
}

/// A lossless collector of diagnostics. Diagnostics are appended in order,
/// never dropped, never deduplicated. The caller decides whether to emit
/// them or discard them downstream.
#[derive(Debug, Default)]
pub struct DiagnosticCollector {
    diags: Vec<Diagnostic>,
}

impl DiagnosticCollector {
    #[must_use]
    pub fn new() -> Self {
        Self { diags: Vec::new() }
    }

    pub fn push(&mut self, diag: Diagnostic) {
        self.diags.push(diag);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diags.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.diags.len()
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diags.iter().any(|d| d.severity == Severity::Error)
    }

    /// Returns an iterator over the collected diagnostics in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diags.iter()
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diags
    }
}

// ---------------------------------------------------------------------------
// Bump allocator (arena)
// ---------------------------------------------------------------------------

/// A simple bump-pointer arena. Allocates `T`s in growable slabs and returns
/// references stable for the arena's entire lifetime.
///
/// This is intentionally NOT a general-purpose allocator. It does not support
/// deallocation. When the arena is dropped, all allocated memory is freed at
/// once. The intended use is single-translation-unit compiler passes where
/// bulk deallocation after a pass is cheaper than per-node `Box` churn.
///
/// The arena is deliberately neither [`Send`] nor [`Sync`]. It may own values
/// with thread affinity, and its unsafe shared-borrow allocation relies on all
/// allocation calls being serialized on the owner thread.
///
/// ```compile_fail
/// use evocc_common::Arena;
///
/// let arena = Arena::default();
/// std::thread::spawn(move || drop(arena));
/// ```
///
/// ```compile_fail
/// use evocc_common::Arena;
///
/// let arena = Arena::default();
/// std::thread::scope(|scope| {
///     scope.spawn(|| {
///         let _ = arena.chunk_count();
///     });
/// });
/// ```
pub struct Arena {
    state: std::cell::RefCell<ArenaState>,
    /// Default chunk size; grows if a single allocation exceeds it.
    chunk_size: usize,
    /// Defense-in-depth phase marker checked before `state`. It does not make
    /// an `Arena` reference retained by an allocated value valid during Drop.
    dropping: std::cell::Cell<bool>,
    /// An arena accepts arbitrary non-Send values, so the arena itself must
    /// remain on the creating thread until those values have been dropped.
    _thread_local: core::marker::PhantomData<std::rc::Rc<()>>,
}

/// Mutable bookkeeping lives behind one `RefCell`; references returned by the
/// arena point only into separately allocated chunks, never into these vectors.
/// Moving or growing either vector therefore cannot invalidate an allocation.
struct ArenaState {
    chunks: Vec<ArenaChunk>,
    drops: Vec<DropRecord>,
}

struct ArenaChunk {
    ptr: core::ptr::NonNull<u8>,
    layout: std::alloc::Layout,
    used: usize,
}

impl Drop for ArenaChunk {
    fn drop(&mut self) {
        // Safety: `ptr` was allocated with exactly `layout` and is owned by
        // this chunk until this destructor runs.
        unsafe { std::alloc::dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

struct DropRecord {
    ptr: core::ptr::NonNull<u8>,
    len: usize,
    drop_fn: unsafe fn(*mut u8, usize),
}

struct SliceInitGuard<T> {
    ptr: *mut T,
    initialized: usize,
}

impl<T> Drop for SliceInitGuard<T> {
    fn drop(&mut self) {
        // Safety: precisely `initialized` consecutive elements were written.
        unsafe { drop_elements::<T>(self.ptr.cast(), self.initialized) };
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl Arena {
    /// Create an arena with a default chunk size (bytes).
    #[must_use]
    pub fn new(chunk_size: usize) -> Self {
        assert!(chunk_size >= core::mem::size_of::<usize>());
        Self {
            state: std::cell::RefCell::new(ArenaState {
                chunks: Vec::new(),
                drops: Vec::new(),
            }),
            chunk_size,
            dropping: std::cell::Cell::new(false),
            _thread_local: core::marker::PhantomData,
        }
    }

    /// Allocate a `T` inside the arena and return a stable reference to it.
    /// `alloc` is the hot path for IR node construction and must not allocate
    /// from the global allocator on the common case (chunk has headroom).
    pub fn alloc<T: Sized>(&mut self, value: T) -> &mut T {
        let ptr = self.allocate_value(value);

        // Safety: the exclusive arena borrow prevents any other safe
        // allocation while this mutable reference is live. The initialized
        // value remains in its stable chunk until the arena is dropped.
        unsafe { &mut *ptr }
    }

    /// Allocate a `T` through a shared arena borrow.
    ///
    /// This narrow escape hatch supports immutable, arena-linked IR graphs.
    /// Prefer [`Arena::alloc`] whenever a mutable arena borrow is available.
    ///
    /// # Safety
    ///
    /// `T` must not directly or transitively store a reference or pointer to
    /// this `Arena`. Neither `T` nor any destructor it transitively runs may
    /// access this arena during teardown. If a destructor reads another value
    /// in this arena, that value must have been allocated earlier and must
    /// remain live under the arena's reverse-allocation drop order. Callers
    /// must also not mutate stored dependency references to violate that order.
    pub unsafe fn alloc_shared<'arena, T: Sized + 'arena>(&'arena self, value: T) -> &'arena T {
        let ptr = self.allocate_value(value);

        // Safety: `allocate_value` initialized this fresh location. The caller
        // upholds the teardown and dependency-order requirements above.
        unsafe { &*ptr }
    }

    fn allocate_value<T: Sized>(&self, value: T) -> *mut T {
        self.assert_can_allocate();
        let size = core::mem::size_of::<T>();
        let align = core::mem::align_of::<T>();

        let ptr = {
            let mut state = self.state.borrow_mut();
            if core::mem::needs_drop::<T>() {
                // Reserve before moving `value` into arena storage. Once it is
                // written, registering its destructor cannot allocate/panic.
                state.drops.reserve(1);
            }
            let ptr = Self::allocate_raw(&mut state, self.chunk_size, size, align);
            // Safety: `ptr` is correctly aligned fresh storage reserved only
            // for this value. Chunk buffers do not move until arena teardown.
            unsafe {
                ptr.cast::<T>().write(value);
            }
            if core::mem::needs_drop::<T>() {
                state.drops.push(DropRecord {
                    ptr: core::ptr::NonNull::new(ptr)
                        .expect("arena allocation returned a null pointer"),
                    len: 1,
                    drop_fn: drop_elements::<T>,
                });
            }
            ptr.cast::<T>()
        };
        ptr
    }

    /// Allocate a string (`&str`) inside the arena. Common enough to warrant
    /// its own method so callers do not repeat the `from_utf8_unchecked`
    /// ceremony.
    pub fn alloc_str(&mut self, s: &str) -> &str {
        let bytes = s.as_bytes();
        let slice = self.alloc_slice::<u8>(bytes.len());
        slice.copy_from_slice(bytes);
        // Safety: `bytes` was a valid `&str`, so `slice` encodes valid UTF-8.
        unsafe { core::str::from_utf8_unchecked(slice) }
    }

    /// Allocate a `[T]` of `len` elements, each initialized to `T::default()`.
    /// The default-initialization makes this safe for types where all-zero
    /// bytes would be an invalid bit pattern (NonZeroU8, bool, etc.).
    pub fn alloc_slice<T: Default>(&mut self, len: usize) -> &mut [T] {
        self.alloc_slice_with(len, |_| T::default())
    }

    /// Allocate and initialize a slice inside the arena. If an initializer
    /// panics, every already-created element is dropped exactly once and the
    /// arena remains usable.
    pub fn alloc_slice_with<T, F>(&mut self, len: usize, mut init: F) -> &mut [T]
    where
        F: FnMut(usize) -> T,
    {
        self.assert_can_allocate();
        if len == 0 {
            return &mut [];
        }
        let size = core::mem::size_of::<T>()
            .checked_mul(len)
            .expect("alloc_slice overflow");
        let ptr = {
            let mut state = self.state.borrow_mut();
            if core::mem::needs_drop::<T>() {
                state.drops.reserve(1);
            }
            Self::allocate_raw(
                &mut state,
                self.chunk_size,
                size,
                core::mem::align_of::<T>(),
            )
        };
        let typed = ptr.cast::<T>();
        let mut guard = SliceInitGuard {
            ptr: typed,
            initialized: 0,
        };
        // Safety: `allocate_raw` returned storage with the size and alignment
        // required for `len` values of `T`. The guard owns the initialized
        // prefix until construction has completed.
        unsafe {
            for i in 0..len {
                typed.add(i).write(init(i));
                guard.initialized += 1;
            }
            if core::mem::needs_drop::<T>() {
                // Capacity was reserved before initialization, so this transfer
                // from the rollback guard to the arena ledger cannot allocate.
                self.state.borrow_mut().drops.push(DropRecord {
                    ptr: core::ptr::NonNull::new_unchecked(ptr),
                    len,
                    drop_fn: drop_elements::<T>,
                });
            }
            core::mem::forget(guard);
            core::slice::from_raw_parts_mut(typed, len)
        }
    }

    /// Total bytes allocated so far (sum of all chunk sizes).
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.state
            .borrow()
            .chunks
            .iter()
            .map(|chunk| chunk.layout.size())
            .sum()
    }

    /// Number of chunks allocated.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.state.borrow().chunks.len()
    }

    /// Return whether `value` is backed by one of this arena's live chunks.
    ///
    /// This is an ownership witness for references supplied by callers; it
    /// deliberately accepts subobjects as well as allocation roots.
    #[must_use]
    pub fn contains<T>(&self, value: &T) -> bool {
        let value_start = std::ptr::from_ref(value).cast::<u8>() as usize;
        let physical_size = core::mem::size_of::<T>().max(1);
        let Some(value_end) = value_start.checked_add(physical_size) else {
            return false;
        };

        self.state.borrow().chunks.iter().any(|chunk| {
            let chunk_start = chunk.ptr.as_ptr() as usize;
            let chunk_end = chunk_start + chunk.layout.size();
            chunk_start <= value_start && value_end <= chunk_end
        })
    }

    // --- private ----------------------------------------------------------

    fn assert_can_allocate(&self) {
        // This phase check is defensive. `alloc_shared`'s safety contract still
        // prohibits destructors from retaining and accessing their owner.
        assert!(
            !self.dropping.get(),
            "cannot allocate while Arena is dropping"
        );
    }

    fn allocate_raw(
        state: &mut ArenaState,
        chunk_size: usize,
        size: usize,
        align: usize,
    ) -> *mut u8 {
        debug_assert!(align.is_power_of_two());
        if size == 0 {
            return std::ptr::without_provenance_mut(align);
        }

        if let Some(chunk) = state.chunks.last_mut() {
            if chunk.layout.align() >= align {
                if let Some(start) = align_up(chunk.used, align) {
                    if let Some(end) = start.checked_add(size) {
                        if end <= chunk.layout.size() {
                            chunk.used = end;
                            // Safety: start..end is within this live chunk.
                            return unsafe { chunk.ptr.as_ptr().add(start) };
                        }
                    }
                }
            }
        }

        let capacity = chunk_size.max(size);
        let chunk_align = align.max(core::mem::align_of::<usize>());
        let layout = std::alloc::Layout::from_size_align(capacity, chunk_align)
            .expect("arena allocation layout overflow");
        // Safety: layout has non-zero size and valid power-of-two alignment.
        let allocation = unsafe { std::alloc::alloc(layout) };
        let ptr = core::ptr::NonNull::new(allocation)
            .unwrap_or_else(|| std::alloc::handle_alloc_error(layout));
        state.chunks.push(ArenaChunk {
            ptr,
            layout,
            used: size,
        });
        ptr.as_ptr()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        // Mark teardown before taking the ledger. This is a defensive check for
        // raw-pointer misuse, not permission for `alloc_shared` values to retain
        // and access their owner from a destructor.
        self.dropping.set(true);
        let drops = std::mem::take(&mut self.state.get_mut().drops);
        let mut first_panic = None;
        for record in drops.into_iter().rev() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Safety: every record describes initialized values that have
                // not previously been dropped.
                unsafe { (record.drop_fn)(record.ptr.as_ptr(), record.len) };
            }));
            if first_panic.is_none() {
                first_panic = result.err();
            }
        }
        if !std::thread::panicking() {
            if let Some(payload) = first_panic {
                std::panic::resume_unwind(payload);
            }
        }
    }
}

fn align_up(offset: usize, align: usize) -> Option<usize> {
    offset
        .checked_add(align - 1)
        .map(|value| value & !(align - 1))
}

unsafe fn drop_elements<T>(ptr: *mut u8, len: usize) {
    let typed = ptr.cast::<T>();
    let mut first_panic = None;
    for index in (0..len).rev() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Safety: the caller guarantees all `len` elements are initialized.
            unsafe { core::ptr::drop_in_place(typed.add(index)) };
        }));
        if first_panic.is_none() {
            first_panic = result.err();
        }
    }
    if let Some(payload) = first_panic {
        std::panic::resume_unwind(payload);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // SourceSpan
    // -----------------------------------------------------------------------

    #[test]
    fn source_span_carries_file_line_column() {
        let file = FileId::new(7);
        let span = SourceSpan::new(file, 10, 25, 3, 5);

        assert_eq!(span.file, file);
        assert_eq!(span.byte_start, 10);
        assert_eq!(span.byte_end, 25);
        assert_eq!(span.line_start, 3);
        assert_eq!(span.column_start, 5);
        assert!(span.is_valid());
    }

    #[test]
    fn source_span_zero_width_is_legal() {
        let span = SourceSpan::new(FileId::new(1), 0, 0, 1, 1);
        assert!(span.is_valid());
        assert_eq!(span.byte_end, 0);
    }

    #[test]
    #[should_panic]
    fn source_span_rejects_negative_width() {
        let _ = SourceSpan::new(FileId::new(1), 10, 5, 1, 1);
    }

    #[test]
    fn source_file_line_col_computation() {
        // "ab\ncde\nf\n" => chars: a(0) b(1) \n(2) c(3) d(4) e(5) \n(6) f(7) \n(8)
        let file = SourceFile::new(FileId::new(1), "t.c".into(), "ab\ncde\nf\n".into());

        let (line, col) = file.line_col(0); // 'a'
        assert_eq!((line, col), (1, 1));
        let (line, col) = file.line_col(1); // 'b'
        assert_eq!((line, col), (1, 2));
        let (line, col) = file.line_col(3); // 'c'
        assert_eq!((line, col), (2, 1));
        let (line, col) = file.line_col(5); // 'e'
        assert_eq!((line, col), (2, 3));
        let (line, col) = file.line_col(7); // 'f'
        assert_eq!((line, col), (3, 1));
    }

    // -----------------------------------------------------------------------
    // Diagnostic
    // -----------------------------------------------------------------------

    #[test]
    fn diagnostic_collector_retains_all() {
        let span = SourceSpan::new(FileId::new(1), 0, 0, 0, 0);
        let mut col = DiagnosticCollector::new();
        assert!(col.is_empty());

        for i in 0..50 {
            let sev = match i % 3 {
                0 => Severity::Error,
                1 => Severity::Warning,
                _ => Severity::Note,
            };
            col.push(Diagnostic {
                severity: sev,
                span,
                message: format!("msg {i}"),
            });
        }

        assert_eq!(col.len(), 50);
        assert!(col.has_errors());

        let all: Vec<_> = col.iter().collect();
        assert_eq!(all.len(), 50);
        for (i, d) in all.iter().enumerate() {
            assert_eq!(d.message, format!("msg {i}"));
        }

        // No loss on conversion to owned vec.
        let v = col.into_vec();
        assert_eq!(v.len(), 50);
    }

    // -----------------------------------------------------------------------
    // Arena
    // -----------------------------------------------------------------------

    #[test]
    fn arena_allocation_is_stable() {
        let mut arena = Arena::new(4096);

        for i in 0..1024 {
            let r: &mut u64 = arena.alloc(i as u64);
            assert_eq!(*r, i as u64);
            *r = 0xDEAD;
            assert_eq!(*r, 0xDEAD);
        }

        // Large allocation forces a new chunk; arena handles over-size.
        let big: &[u8; 8192] = arena.alloc([0xAA; 8192]);
        assert_eq!(big[0], 0xAA);
        assert!(arena.chunk_count() >= 2);

        // Re-allocate a small value after the large allocation.
        let r0: &mut u64 = arena.alloc(7);
        assert_eq!(*r0, 7);

        // Different types allocated from the same arena.
        #[derive(Debug, PartialEq)]
        struct Node {
            kind: u8,
            value: i64,
        }
        let _ = r0;
        let n: &mut Node = arena.alloc(Node { kind: 1, value: 42 });
        assert_eq!(n.value, 42);
    }

    #[test]
    fn arena_alloc_str_is_valid_utf8() {
        let mut arena = Arena::new(256);

        let s: &str = arena.alloc_str("hello world");
        assert_eq!(s, "hello world");

        let mut second_arena = Arena::new(256);
        let s2: &str = second_arena.alloc_str("编译");
        assert_eq!(s2, "编译");
    }

    #[test]
    fn arena_alloc_slice_returns_zeros() {
        let mut arena = Arena::new(256);
        let slice: &mut [i32] = arena.alloc_slice::<i32>(10);
        assert_eq!(slice.len(), 10);
        assert!(slice.iter().all(|&x| x == 0));
    }
}
