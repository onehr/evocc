use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, Token, TokenKind};
use evocc_parser::Parser;

static MEASURING: AtomicBool = AtomicBool::new(false);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if MEASURING.load(Ordering::Relaxed) {
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: the request is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if MEASURING.load(Ordering::Relaxed) {
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: the request is forwarded unchanged to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout came from the same system allocator.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if MEASURING.load(Ordering::Relaxed) {
            ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
        }
        // SAFETY: the pointer and layout came from the same system allocator.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Measurement;

impl Measurement {
    fn start() -> Self {
        ALLOCATED_BYTES.store(0, Ordering::SeqCst);
        assert!(!MEASURING.swap(true, Ordering::SeqCst));
        Self
    }

    fn finish(self) -> usize {
        MEASURING.store(false, Ordering::SeqCst);
        let bytes = ALLOCATED_BYTES.load(Ordering::SeqCst);
        std::mem::forget(self);
        bytes
    }
}

impl Drop for Measurement {
    fn drop(&mut self) {
        MEASURING.store(false, Ordering::SeqCst);
    }
}

struct PreparedUnit {
    source: String,
    tokens: Vec<Token>,
}

fn prepare(functions: usize) -> PreparedUnit {
    let mut source = String::with_capacity(functions * 48);
    for ordinal in 0..functions {
        writeln!(
            source,
            "void probe_{ordinal}(int parameter, typeof(parameter) copy);"
        )
        .unwrap();
    }
    let mut lexer = Lexer::new(FileId::new(97_001), &source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error);
        tokens.push(token);
    }
    PreparedUnit { source, tokens }
}

fn parse_allocated_bytes(prepared: PreparedUnit) -> usize {
    let arena = Arena::default();
    let measurement = Measurement::start();
    let mut parser = Parser::new(prepared.tokens, FileId::new(97_001), &prepared.source);
    let unit = parser.parse_translation_unit_in(&arena);
    std::hint::black_box(unit.symbols().len());
    let bytes = measurement.finish();
    assert!(
        parser.diagnostic().is_none(),
        "generated declaration corpus must remain semantically valid: {:?}",
        parser.diagnostic()
    );
    bytes
}

#[test]
fn speculative_declaration_allocations_scale_near_linearly() {
    let _ = parse_allocated_bytes(prepare(4));
    let small = parse_allocated_bytes(prepare(32));
    let large = parse_allocated_bytes(prepare(128));
    eprintln!("checkpoint-allocation-bytes: small={small} large={large}");

    assert!(
        large <= small.saturating_mul(6),
        "quadrupling declarations allocated {large} bytes versus {small}; \
         speculative semantic checkpoints must remain near-linear"
    );
}
