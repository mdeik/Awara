//! Bounded memo caches shared by the filesystem probes.
//!
//! Both the case-sensitivity probe (`case`) and the attribute-capability probe
//! (`attrs`) memoize a pure, per-directory result that is cheap to recompute and
//! safe to drop. A recursive scan or a long GUI session can touch a very large
//! number of directories, so these memos are bounded and evict **eldest first**
//! (FIFO) instead of growing for the life of the process.
//!
//! FIFO rather than LRU on purpose: promoting a hit to most-recently-used is
//! `O(capacity)` with std collections (no intrusive linked map), and these
//! memos are hit in short bursts where insertion order already tracks recency.
//! An evicted entry simply costs one recomputation on its next use.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Entries retained per memo before the eldest is evicted. At roughly 100–150
/// bytes per entry this caps each memo in the low single-digit MB.
pub(crate) const MEMO_CAP: usize = 16_384;

/// A process-wide, mutex-guarded map with a fixed capacity. Eviction is FIFO.
pub(crate) struct BoundedMemo<K, V, const CAP: usize> {
    cell: OnceLock<Mutex<Inner<K, V, CAP>>>,
}

/// The guarded contents of a [`BoundedMemo`]. Public within the crate so
/// callers can hold the guard across a probe.
pub(crate) struct Inner<K, V, const CAP: usize> {
    map: HashMap<K, V>,
    /// Insertion order of the keys in `map`, eldest at the front.
    order: VecDeque<K>,
}

impl<K, V, const CAP: usize> BoundedMemo<K, V, CAP> {
    pub(crate) const fn new() -> Self {
        Self {
            cell: OnceLock::new(),
        }
    }

    /// Lock the memo. Callers that must keep a probe serialized may hold the
    /// guard across it (the case-sensitivity probe probes under the lock).
    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner<K, V, CAP>> {
        self.cell
            .get_or_init(|| {
                Mutex::new(Inner {
                    map: HashMap::new(),
                    order: VecDeque::new(),
                })
            })
            .lock()
            .unwrap()
    }
}

impl<K: Eq + Hash + Clone, V: Copy, const CAP: usize> Inner<K, V, CAP> {
    pub(crate) fn get(&self, key: &K) -> Option<V> {
        self.map.get(key).copied()
    }

    /// Insert or overwrite. A new key past the capacity evicts the eldest;
    /// overwriting an existing key keeps its original position.
    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.map.insert(key.clone(), value).is_some() {
            return;
        }
        self.order.push_back(key);
        while self.order.len() > CAP {
            if let Some(eldest) = self.order.pop_front() {
                self.map.remove(&eldest);
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_eldest_beyond_capacity() {
        let memo: BoundedMemo<u32, u32, 2> = BoundedMemo::new();
        {
            let mut m = memo.lock();
            m.insert(1, 10);
            m.insert(2, 20);
        }
        assert_eq!(memo.lock().get(&1), Some(10));

        memo.lock().insert(3, 30);
        assert_eq!(memo.lock().get(&1), None, "eldest evicted");
        assert_eq!(memo.lock().get(&2), Some(20));
        assert_eq!(memo.lock().get(&3), Some(30));
    }

    #[test]
    fn overwriting_does_not_add_a_second_order_slot() {
        let memo: BoundedMemo<u32, u32, 3> = BoundedMemo::new();
        {
            let mut m = memo.lock();
            m.insert(1, 1);
            m.insert(1, 11); // overwrite must not consume an order slot
            m.insert(2, 2);
            m.insert(3, 3);
        }
        // Only three slots were used, so nothing was evicted and the value is
        // the overwritten one. A duplicate slot would have evicted key 1 here.
        assert_eq!(memo.lock().get(&1), Some(11));
        assert_eq!(memo.lock().get(&2), Some(2));
        assert_eq!(memo.lock().get(&3), Some(3));

        memo.lock().insert(4, 4); // now evicts the eldest, key 1
        assert_eq!(memo.lock().get(&1), None);
        assert_eq!(memo.lock().get(&4), Some(4));
    }

    #[test]
    fn clear_empties_the_memo() {
        let memo: BoundedMemo<u32, u32, 4> = BoundedMemo::new();
        memo.lock().insert(7, 7);
        memo.lock().clear();
        assert_eq!(memo.lock().get(&7), None);
    }
}
