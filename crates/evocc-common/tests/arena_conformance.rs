use evocc_common::Arena;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::rc::Rc;

/// Exercise the Arena with only the alignment its backing allocation asks
/// for. General-purpose allocators often over-align by accident, which can
/// hide a broken bump allocator in ordinary tests.
struct MinimallyAligned;

unsafe impl GlobalAlloc for MinimallyAligned {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 4096 && layout.align() == 8 {
            let expanded = Layout::from_size_align_unchecked(4104, 8);
            let base = System.alloc(expanded);
            if base.is_null() {
                return base;
            }
            return base.add(8);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == 4096 && layout.align() == 8 {
            let expanded = Layout::from_size_align_unchecked(4104, 8);
            System.dealloc(ptr.sub(8), expanded);
        } else {
            System.dealloc(ptr, layout);
        }
    }
}

#[global_allocator]
static ALLOCATOR: MinimallyAligned = MinimallyAligned;

#[repr(align(64))]
struct OverAlignedDrop {
    ordinal: usize,
    drops: Rc<RefCell<Vec<usize>>>,
}

impl Default for OverAlignedDrop {
    fn default() -> Self {
        Self {
            ordinal: 0,
            drops: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl Drop for OverAlignedDrop {
    fn drop(&mut self) {
        self.drops.borrow_mut().push(self.ordinal);
    }
}

#[test]
fn arena_slice_honors_alignment_and_runs_destructors() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    {
        let mut arena = Arena::new(4096);
        let values = arena.alloc_slice_with(3, |ordinal| OverAlignedDrop {
            ordinal,
            drops: Rc::clone(&drops),
        });
        for value in values.iter() {
            assert_eq!((value as *const OverAlignedDrop as usize) % 64, 0);
        }
        assert!(drops.borrow().is_empty());
    }
    assert_eq!(&*drops.borrow(), &[2, 1, 0]);
}

#[test]
fn arena_rolls_back_a_partially_initialized_slice() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut arena = Arena::new(4096);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        arena.alloc_slice_with(5, |ordinal| {
            if ordinal == 3 {
                panic!("controlled initializer failure");
            }
            OverAlignedDrop {
                ordinal,
                drops: Rc::clone(&drops),
            }
        });
    }));
    assert!(result.is_err());
    assert_eq!(&*drops.borrow(), &[2, 1, 0]);

    let value = arena.alloc(41_u64);
    assert_eq!(*value, 41);
}

#[test]
fn arena_continues_after_an_element_destructor_panics() {
    struct PanicDrop {
        ordinal: usize,
        drops: Rc<RefCell<Vec<usize>>>,
    }

    impl Drop for PanicDrop {
        fn drop(&mut self) {
            self.drops.borrow_mut().push(self.ordinal);
            if self.ordinal == 1 {
                panic!("controlled destructor failure");
            }
        }
    }

    let drops = Rc::new(RefCell::new(Vec::new()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe({
        let drops = Rc::clone(&drops);
        move || {
            let mut arena = Arena::new(4096);
            arena.alloc_slice_with(3, |ordinal| PanicDrop {
                ordinal,
                drops: Rc::clone(&drops),
            });
            drop(arena);
        }
    }));
    assert!(result.is_err());
    assert_eq!(&*drops.borrow(), &[2, 1, 0]);
}
