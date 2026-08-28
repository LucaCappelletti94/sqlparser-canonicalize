use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use sqlparser::dialect::PostgreSqlDialect;
use sqlparser_canonicalize::normalize_sql;

static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

// SAFETY: Every allocation and deallocation is forwarded unchanged to `System`.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `GlobalAlloc` callers provide a valid layout.
        let pointer = unsafe { System.alloc(layout) };
        if ENABLED.load(Ordering::Relaxed) && !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: `GlobalAlloc` callers return the original pointer and layout pair.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn measure(sql: &str) -> usize {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
    let normalized = black_box(normalize_sql(
        black_box(sql),
        black_box(&PostgreSqlDialect {}),
    ))
    .unwrap();
    ENABLED.store(false, Ordering::Relaxed);
    black_box(normalized);
    ALLOCATIONS.load(Ordering::Relaxed)
}

#[test]
fn fixed_corpus_allocation_count_does_not_grow() {
    let sql = "SELECT * FROM t WHERE (a = 1 AND b = 2) OR (c = 3 AND d = 4) AND e IN (5, 6, 7)";
    let _ = measure(sql);
    let counts = std::array::from_fn::<_, 16, _>(|_| measure(sql));
    assert_eq!(counts, [194; 16]);
}
