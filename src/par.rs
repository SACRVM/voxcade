//! Work spread over the CPU's cores where there is enough of it to pay for
//! the threads, the results always in the order of the input: the
//! simulation stays deterministic however many cores there are.

/// Threads are started for each call: more than this cost more to start
/// than the parts of a tick's work take.
const MAX_THREADS: usize = 8;

/// How many threads the CPU runs at once. Asked once: the standard library
/// reads system files for it (cgroup quotas on Linux), which took longer
/// than the work it was asked for.
pub fn cores() -> usize {
    static CORES: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CORES.get_or_init(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
}

/// `work` on `items` in parts, one part a thread (at least `min_part` items
/// each; fewer items in all stay on this thread), the parts' results joined
/// in the order of the items.
pub fn in_order<T: Sync, R: Send>(items: &[T], min_part: usize, work: impl Fn(&[T]) -> Vec<R> + Sync) -> Vec<R> {
    let threads = cores().min(MAX_THREADS).min(items.len() / min_part.max(1));
    if threads <= 1 {
        return work(items);
    }
    let part = items.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let parts: Vec<_> = items.chunks(part).map(|chunk| scope.spawn(|| work(chunk))).collect();
        parts.into_iter().flat_map(|p| p.join().expect("a worker thread failed")).collect()
    })
}

#[cfg(test)]
mod tests {
    /// However the work is split, the results come in the order of the input.
    #[test]
    fn results_keep_the_order_of_the_input() {
        let items: Vec<u32> = (0..10_000).collect();
        let out = super::in_order(&items, 10, |part| part.iter().map(|i| i * 2).collect());
        assert_eq!(out, items.iter().map(|i| i * 2).collect::<Vec<_>>());
    }
}
