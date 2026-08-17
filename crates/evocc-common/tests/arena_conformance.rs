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

thread_local! {
    static ZST_DROPS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

#[repr(align(64))]
struct OverAlignedZst<const ORDINAL: usize>;

impl<const ORDINAL: usize> Drop for OverAlignedZst<ORDINAL> {
    fn drop(&mut self) {
        ZST_DROPS.with(|drops| drops.borrow_mut().push(ORDINAL));
    }
}

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

/// Allocate a test node through the shared arena API.
///
/// # Safety
///
/// Callers in this module only use values whose transitive destructors do not
/// access `arena`; references between values always point to older allocations.
unsafe fn alloc_shared<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
    // Safety: upheld by this helper's module-local contract.
    unsafe { arena.alloc_shared(value) }
}

#[test]
fn arena_allocates_through_shared_borrow_while_prior_references_live() {
    let arena = Arena::new(128);

    // Safety: String and Vec destructors do not access their owning Arena.
    let first = unsafe { alloc_shared(&arena, String::from("first")) };
    let first_address = std::ptr::from_ref(first);
    // Safety: same as above; these values do not contain an Arena reference.
    let second = unsafe { alloc_shared(&arena, String::from("second")) };
    let numbers = unsafe { alloc_shared(&arena, vec![10, 11, 12]) };

    assert_eq!(first, "first");
    assert_eq!(std::ptr::from_ref(first), first_address);
    assert_eq!(second, "second");
    assert_eq!(numbers.as_slice(), [10, 11, 12]);
}

#[test]
fn arena_contains_only_storage_backed_by_its_chunks() {
    let arena = Arena::new(128);
    // Safety: integers have no destructor and cannot access the Arena.
    let allocated = unsafe { alloc_shared(&arena, 73_u64) };
    let stack = 73_u64;
    let boxed = Box::new(73_u64);

    assert!(arena.contains(allocated));
    assert!(!arena.contains(&stack));
    assert!(!arena.contains(&*boxed));

    // Safety: integers have no destructor and cannot access the Arena.
    let later = unsafe { alloc_shared(&arena, 91_u64) };
    assert!(arena.contains(allocated));
    assert!(arena.contains(later));
}

#[test]
fn arena_drops_overaligned_zsts_in_exact_lifo_order() {
    ZST_DROPS.with(|drops| drops.borrow_mut().clear());
    assert_eq!(std::mem::size_of::<OverAlignedZst<1>>(), 0);

    {
        let arena = Arena::new(128);
        // Safety: this destructor only writes to a thread-local log.
        let first = unsafe { alloc_shared(&arena, OverAlignedZst::<1>) };
        let first_address = std::ptr::from_ref(first) as usize;
        assert_eq!(first_address % 64, 0);

        // Safety: each destructor only writes to a thread-local log.
        let middle = unsafe {
            alloc_shared(
                &arena,
                [
                    OverAlignedZst::<2>,
                    OverAlignedZst::<2>,
                    OverAlignedZst::<2>,
                ],
            )
        };
        assert_eq!(middle.as_ptr() as usize % 64, 0);
        // Safety: this destructor only writes to a thread-local log.
        let last = unsafe { alloc_shared(&arena, OverAlignedZst::<3>) };
        assert_eq!(std::ptr::from_ref(last) as usize % 64, 0);

        assert_eq!(std::ptr::from_ref(first) as usize, first_address);
        ZST_DROPS.with(|drops| assert!(drops.borrow().is_empty()));
    }

    ZST_DROPS.with(|drops| assert_eq!(&*drops.borrow(), &[3, 2, 2, 2, 1]));
}

#[test]
fn arena_drops_dependents_before_the_older_nodes_they_read() {
    struct Older {
        text: String,
        events: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for Older {
        fn drop(&mut self) {
            self.events.borrow_mut().push("older");
        }
    }

    struct Dependent<'arena> {
        older: &'arena Older,
        owned: Vec<String>,
        events: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for Dependent<'_> {
        fn drop(&mut self) {
            assert_eq!(self.older.text, "still live");
            assert_eq!(self.owned, ["owned Vec element"]);
            self.events.borrow_mut().push("dependent");
        }
    }

    let events = Rc::new(RefCell::new(Vec::new()));
    {
        let arena = Arena::new(128);
        // Safety: `Older` does not access the Arena from Drop.
        let older = unsafe {
            alloc_shared(
                &arena,
                Older {
                    text: String::from("still live"),
                    events: Rc::clone(&events),
                },
            )
        };
        // Safety: the destructor reads only `older`, which was allocated first
        // and therefore remains live until after this dependent is dropped.
        let _dependent = unsafe {
            alloc_shared(
                &arena,
                Dependent {
                    older,
                    owned: vec![String::from("owned Vec element")],
                    events: Rc::clone(&events),
                },
            )
        };
    }

    assert_eq!(&*events.borrow(), &["dependent", "older"]);
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
