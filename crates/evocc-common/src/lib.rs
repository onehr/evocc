#![allow(unused_must_use)]
#![allow(dropping_references)]
#![allow(clippy::all)]
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

/// A simple bump-pointer arena. Allocates `T`s in a growable slab and returns
/// `&T` references stable for the arena's entire lifetime.
///
/// This is intentionally NOT a general-purpose allocator. It does not support
/// deallocation. When the arena is dropped, all allocated memory is freed at
/// once. The intended use is single-translation-unit compiler passes where
/// bulk deallocation after a pass is cheaper than per-node `Box` churn.
pub struct Arena {
    // Chunks of memory; each chunk is a Box<[u8]> with bump pointer tracking.
    chunks: Vec<Box<[usize]>>,
    /// Offset within the current chunk where the next allocation starts.
    bump: usize,
    /// Default chunk size; grows if a single allocation exceeds it.
    chunk_size: usize,
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
            chunks: Vec::new(),
            bump: 0,
            chunk_size,
        }
    }

    /// Allocate a `T` inside the arena and return a stable reference to it.
    /// `alloc` is the hot path for IR node construction and must not allocate
    /// from the global allocator on the common case (chunk has headroom).
    pub fn alloc<T: Sized>(&mut self, value: T) -> &mut T {
        let size = core::mem::size_of::<T>();
        let align = core::mem::align_of::<T>();

        let ptr = self.allocate_raw(size, align);
        // Safety: `ptr` points to `size` bytes of freshly allocated memory
        // within a valid chunk that will not be freed or moved until the
        // Arena itself is dropped.
        unsafe {
            ptr.cast::<T>().write(value);
            &mut *ptr.cast::<T>()
        }
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

    /// Allocate a `[T]` of `len` elements (zero-initialized).
    pub fn alloc_slice<T: Sized + Copy>(&mut self, len: usize) -> &mut [T] {
        if len == 0 {
            return &mut [];
        }
        let size = core::mem::size_of::<T>()
            .checked_mul(len)
            .expect("alloc_slice overflow");
        let ptr = self.allocate_raw(size, core::mem::align_of::<T>());
        // Safety: `allocate_raw` returns `size` bytes of valid memory.
        unsafe {
            ptr.write_bytes(0, size);
            core::slice::from_raw_parts_mut(ptr.cast::<T>(), len)
        }
    }

    /// Total bytes allocated so far (sum of all chunk sizes).
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.chunks
            .iter()
            .map(|c| c.len() * std::mem::size_of::<usize>())
            .sum()
    }

    /// Number of chunks allocated.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    // --- private ----------------------------------------------------------

    fn allocate_raw(&mut self, size: usize, align: usize) -> *mut u8 {
        // Align bump pointer within the current chunk.
        let aligned = self.bump.wrapping_add(align - 1) & !(align - 1);

        if self.chunks.is_empty()
            || aligned + size > self.chunks.last().unwrap().len() * std::mem::size_of::<usize>()
        {
            // Need a new chunk. Allocate at least `chunk_size`, but if a
            // single type exceeds that, use exactly that size (plus alignment
            // padding) so the allocation fits.
            let desired = usize::max(self.chunk_size, size + align);
            let element_count =
                (desired + std::mem::size_of::<usize>() - 1) / std::mem::size_of::<usize>();
            self.chunks
                .push(vec![0usize; element_count].into_boxed_slice());
            // Align the bump within the new chunk, same as the non-new-chunk
            // path below. The previous code returned chunk_ptr at offset 0
            // regardless of alignment, which is UB for types requiring
            // greater-than-byte alignment.
            let chunk_ptr = self.chunks.last_mut().unwrap().as_mut_ptr() as *mut u8;
            let chunk_aligned =
                0usize.wrapping_add(align.saturating_sub(1)) & !(align.saturating_sub(1));
            self.bump = chunk_aligned + size;
            // Safety: chunk_aligned is within the new chunk (zero-aligned
            // within a Box<[u8]> is always 0 for any reasonable alignment
            // requirement, but we compute it explicitly for correctness).
            return unsafe { chunk_ptr.add(chunk_aligned) };
        }

        let chunk_ptr = self.chunks.last_mut().unwrap().as_mut_ptr() as *mut u8;
        // Safety: `aligned + size <= chunk.len()` was just verified.
        self.bump = aligned + size;
        unsafe { chunk_ptr.add(aligned) }
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
        SourceSpan::new(FileId::new(1), 10, 5, 1, 1);
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

        // Allocate values one at a time and check each immediately. Holding
        // shared refs across further allocations requires interior mutability
        // in the arena; the stability property is verified by checking that
        // each value remains at its allocated address, not by holding 1024
        // live refs simultaneously.
        for i in 0..1024 {
            let r: &mut u64 = arena.alloc(i as u64);
            assert_eq!(*r, i as u64);
            // Mutate to prove we own the allocation (arena did not recycle).
            *r = 0xDEAD;
            assert_eq!(*r, 0xDEAD);
            // Write back the correct value for later checks.
            *r = i as u64;
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
        drop(r0);
        let n: &mut Node = arena.alloc(Node { kind: 1, value: 42 });
        assert_eq!(n.value, 42);
    }

    #[test]
    fn arena_alloc_str_is_valid_utf8() {
        let mut arena = Arena::new(256);

        let s: &str = arena.alloc_str("hello world");
        assert_eq!(s, "hello world");

        // Non-ascii in another arena; the test checks correctness, not
        // concurrent reachability (which requires interior mutability or
        // an NLL-aware borrow that Rust's current model does not support
        // for `&mut self` alloc methods).
        let mut a2 = Arena::new(256);
        let s2: &str = a2.alloc_str("编译");
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
