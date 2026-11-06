//! Binary-private Unix worker budget; desktop enforcement stays disabled.
//!
//! Counts requested Rust heap bytes, not allocator metadata, mappings, or RSS.
//! Enable exactly once on the main thread before starting any worker threads.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

pub(super) const PARSER_ALLOC_LIMIT_EXIT: i32 = 86;
const PARSER_ALLOC_INVARIANT_EXIT: i32 = 87;
#[derive(Debug, PartialEq, Eq)]
enum BudgetError {
    Limit,
    Invariant,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum EnableBudgetError {
    Invalid,
    AlreadyEnabled,
    BaselineTooLarge,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct AllocationBudgetSnapshot {
    pub limit_bytes: usize,
    pub live_bytes_at_enable: usize,
}
struct Accounting {
    live: AtomicUsize,
    limit: AtomicUsize,
}
impl Accounting {
    const fn new() -> Self {
        Self {
            live: AtomicUsize::new(0),
            limit: AtomicUsize::new(0),
        }
    }
    fn reserve(&self, size: usize) -> Result<(), BudgetError> {
        let limit = self.limit.load(Ordering::Acquire);
        let mut live = self.live.load(Ordering::Acquire);
        loop {
            let next = live.checked_add(size).ok_or(BudgetError::Limit)?;
            if limit != 0 && next > limit {
                return Err(BudgetError::Limit);
            }
            match self
                .live
                .compare_exchange_weak(live, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Ok(()),
                Err(observed) => live = observed,
            }
        }
    }
    fn release(&self, size: usize) -> Result<(), BudgetError> {
        let mut live = self.live.load(Ordering::Acquire);
        loop {
            let next = live.checked_sub(size).ok_or(BudgetError::Invariant)?;
            match self
                .live
                .compare_exchange_weak(live, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Ok(()),
                Err(observed) => live = observed,
            }
        }
    }
    fn enable(&self, bytes: usize) -> Result<AllocationBudgetSnapshot, EnableBudgetError> {
        if bytes == 0 || bytes > isize::MAX as usize {
            return Err(EnableBudgetError::Invalid);
        }
        if self.limit.load(Ordering::Acquire) != 0 {
            return Err(EnableBudgetError::AlreadyEnabled);
        }
        let live = self.live.load(Ordering::Acquire);
        if live > bytes {
            return Err(EnableBudgetError::BaselineTooLarge);
        }
        self.limit
            .compare_exchange(0, bytes, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| EnableBudgetError::AlreadyEnabled)?;
        Ok(AllocationBudgetSnapshot {
            limit_bytes: bytes,
            live_bytes_at_enable: live,
        })
    }
}

struct BudgetedSystem {
    accounting: Accounting,
}
impl BudgetedSystem {
    const fn new() -> Self {
        Self {
            accounting: Accounting::new(),
        }
    }
}
fn enforce(result: Result<(), BudgetError>) {
    if let Err(error) = result {
        let code = match error {
            BudgetError::Limit => PARSER_ALLOC_LIMIT_EXIT,
            BudgetError::Invariant => PARSER_ALLOC_INVARIANT_EXIT,
        };
        // SAFETY: _exit is allocation-free, cannot unwind, and deliberately skips
        // destructors. A worker cannot safely recover inside a global allocator.
        unsafe { libc::_exit(code) }
    }
}
// SAFETY: every pointer/Layout pair is delegated to System with its original
// alignment. Accounting uses only fixed atomics and never allocates or unwinds.
unsafe impl GlobalAlloc for BudgetedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        enforce(self.accounting.reserve(layout.size()));
        // SAFETY: caller provides a valid nonzero allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        if pointer.is_null() {
            enforce(self.accounting.release(layout.size()));
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        enforce(self.accounting.reserve(layout.size()));
        // SAFETY: same contract as alloc; System initializes all requested bytes.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if pointer.is_null() {
            enforce(self.accounting.release(layout.size()));
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: caller provides this allocator's live pointer and original layout.
        unsafe { System.dealloc(pointer, layout) };
        enforce(self.accounting.release(layout.size()));
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if self.accounting.limit.load(Ordering::Acquire) != 0 {
            enforce(self.accounting.reserve(size));
            // SAFETY: GlobalAlloc::realloc requires size > 0 and size rounded to
            // the original alignment to fit isize. Keep the original alignment.
            let next_layout = unsafe { Layout::from_size_align_unchecked(size, layout.align()) };
            let next = unsafe { System.alloc(next_layout) };
            if next.is_null() {
                enforce(self.accounting.release(size));
                return next;
            }
            // SAFETY: both allocations are live, nonoverlapping, and have at
            // least min(old,new) bytes. Old storage is freed only after copying.
            unsafe {
                std::ptr::copy_nonoverlapping(pointer, next, layout.size().min(size));
                System.dealloc(pointer, layout);
            }
            enforce(self.accounting.release(layout.size()));
            next
        } else {
            let growth = size.saturating_sub(layout.size());
            enforce(self.accounting.reserve(growth));
            // SAFETY: pointer/layout/new size meet GlobalAlloc::realloc's contract.
            let next = unsafe { System.realloc(pointer, layout, size) };
            if next.is_null() {
                enforce(self.accounting.release(growth));
            } else {
                enforce(self.accounting.release(layout.size().saturating_sub(size)));
            }
            next
        }
    }
}
#[global_allocator]
static ALLOCATOR: BudgetedSystem = BudgetedSystem::new();

pub(super) fn enable_parser_allocation_limit(
    bytes: usize,
) -> Result<AllocationBudgetSnapshot, EnableBudgetError> {
    ALLOCATOR.accounting.enable(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_budget_tracks_baseline_exact_boundary_deallocation_and_reuse() {
        let state = Accounting::new();
        state.reserve(10).unwrap();
        assert_eq!(
            state.enable(30).unwrap(),
            AllocationBudgetSnapshot {
                limit_bytes: 30,
                live_bytes_at_enable: 10
            }
        );
        state.reserve(20).unwrap();
        assert_eq!(state.reserve(1), Err(BudgetError::Limit));
        state.release(20).unwrap();
        state.reserve(20).unwrap();
        state.release(30).unwrap();
        assert_eq!(state.live.load(Ordering::Acquire), 0);
    }
    #[test]
    fn allocation_budget_rejects_overflow_underflow_and_invalid_repeated_enable() {
        let state = Accounting::new();
        state.reserve(usize::MAX).unwrap();
        assert_eq!(state.reserve(1), Err(BudgetError::Limit));
        assert_eq!(state.enable(100), Err(EnableBudgetError::BaselineTooLarge));
        state.release(usize::MAX).unwrap();
        assert_eq!(state.release(1), Err(BudgetError::Invariant));
        assert_eq!(state.enable(0), Err(EnableBudgetError::Invalid));
        state.enable(100).unwrap();
        assert_eq!(state.enable(200), Err(EnableBudgetError::AlreadyEnabled));
    }
    #[test]
    fn allocation_budget_failed_allocation_rollback_and_realloc_peak_keep_old_bytes() {
        let state = Accounting::new();
        state.enable(30).unwrap();
        state.reserve(20).unwrap();
        assert_eq!(
            state.reserve(21),
            Err(BudgetError::Limit),
            "realloc bounds old+new, not net growth"
        );
        state.reserve(10).unwrap();
        state.release(10).unwrap();
        assert_eq!(
            state.live.load(Ordering::Acquire),
            20,
            "null System allocation rolls back only the new reservation"
        );
        state.reserve(10).unwrap();
        state.release(20).unwrap();
        assert_eq!(state.live.load(Ordering::Acquire), 10);
    }
    #[test]
    fn allocation_budget_system_zeroed_and_realloc_preserve_bytes_and_alignment() {
        let allocator = BudgetedSystem::new();
        let old = Layout::from_size_align(32, 32).unwrap();
        // SAFETY: all pointers below belong to this allocator, are checked for
        // null, and are freed exactly once with their matching current layout.
        unsafe {
            let pointer = allocator.alloc_zeroed(old);
            assert!(!pointer.is_null());
            assert_eq!(pointer as usize % 32, 0);
            assert!(
                std::slice::from_raw_parts(pointer, 32)
                    .iter()
                    .all(|v| *v == 0)
            );
            pointer.write(71);
            allocator.accounting.enable(128).unwrap();
            let grown = allocator.realloc(pointer, old, 64);
            assert!(!grown.is_null());
            assert_eq!(grown as usize % 32, 0);
            assert_eq!(grown.read(), 71);
            assert_eq!(allocator.accounting.live.load(Ordering::Acquire), 64);
            let shrunk = allocator.realloc(grown, Layout::from_size_align(64, 32).unwrap(), 16);
            assert!(!shrunk.is_null());
            assert_eq!(shrunk.read(), 71);
            allocator.dealloc(shrunk, Layout::from_size_align(16, 32).unwrap());
        }
        assert_eq!(allocator.accounting.live.load(Ordering::Acquire), 0);
    }
}
