//! A small scoped thread pool for git calls that do not depend on each other
//! (`worktree list` per repository, `status` per worktree). No dependencies beyond std.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

/// Upper bound of threads: each runs one git process, more mostly adds disk contention.
const MAX_THREADS: usize = 8;

/// Threads used for `count` items: the CPU count, capped, never more than the items.
pub fn threads_for(count: usize) -> usize {
    let cpus = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    cpus.clamp(1, MAX_THREADS).min(count.max(1))
}

/// `f` applied to every item on a few threads. Results keep the order of `items`.
pub fn parallel_map<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let threads = threads_for(items.len());
    if threads <= 1 {
        return items.iter().map(&f).collect();
    }
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    break;
                };
                let result = f(item);
                // A poisoned lock only means another worker panicked; keep its partial results.
                let mut guard = done.lock().unwrap_or_else(|e| e.into_inner());
                guard.push((index, result));
            });
        }
    });
    let mut results = done.into_inner().unwrap_or_else(|e| e.into_inner());
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn keeps_the_order_of_items() {
        let items: Vec<u32> = (0..100).collect();
        let doubled = parallel_map(&items, |n| n * 2);
        assert_eq!(doubled, items.iter().map(|n| n * 2).collect::<Vec<_>>());
        assert!(parallel_map(&Vec::<u32>::new(), |n| *n).is_empty());
    }

    /// Counts how many items run at once instead of timing them: a wall-clock bound failed on
    /// loaded CI runners (x86_64 macOS) although the items did overlap.
    #[test]
    fn runs_items_at_the_same_time() {
        if threads_for(8) < 2 {
            return; // One core runs the items one after another by design.
        }
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let items = [0u8; 8];
        parallel_map(&items, |_| {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            // Wait for a second item to start; the deadline only keeps a broken pool from
            // hanging the test, it is not what the assertion measures.
            let deadline = Instant::now() + Duration::from_secs(10);
            while peak.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(1));
            }
            active.fetch_sub(1, Ordering::SeqCst);
        });
        assert!(peak.load(Ordering::SeqCst) >= 2, "items never overlapped");
    }

    #[test]
    fn thread_count_is_bounded() {
        assert_eq!(threads_for(0), 1);
        assert_eq!(threads_for(1), 1);
        assert!(threads_for(1000) <= MAX_THREADS);
    }
}
