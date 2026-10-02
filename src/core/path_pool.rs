use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Thread-safe parent directory pool that deduplicates `Arc<Path>` allocations.
#[derive(Default, Debug)]
pub struct PathPool {
    cache: Mutex<HashMap<PathBuf, Arc<Path>>>,
}

impl PathPool {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Retrieves an existing `Arc<Path>` for the given parent path, or inserts a new one.
    pub fn get_or_insert(&self, parent: &Path) -> Arc<Path> {
        let mut guard = self.cache.lock().unwrap();
        if let Some(arc) = guard.get(parent) {
            Arc::clone(arc)
        } else {
            let arc: Arc<Path> = Arc::from(parent.to_path_buf());
            guard.insert(parent.to_path_buf(), Arc::clone(&arc));
            arc
        }
    }

    /// Returns the number of unique parent directories stored in the pool.
    pub fn unique_parents_count(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_pool_deduplication() {
        let pool = PathPool::new();

        let path1 = pool.get_or_insert(Path::new("/home/user/downloads"));
        let path2 = pool.get_or_insert(Path::new("/home/user/downloads"));

        assert!(Arc::ptr_eq(&path1, &path2));
        assert_eq!(pool.unique_parents_count(), 1);
    }
}
