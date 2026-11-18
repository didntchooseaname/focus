pub struct Cache<K, V> {
    entries: Vec<Entry<K, V>>,
    bytes: usize,
    budget: usize,
    clock: u64,
}

struct Entry<K, V> {
    key: K,
    value: V,
    bytes: usize,
    used: u64,
}

impl<K: PartialEq, V: Clone> Cache<K, V> {
    pub fn new(budget: usize) -> Self {
        Cache {
            entries: Vec::new(),
            bytes: 0,
            budget,
            clock: 0,
        }
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.entries.iter().any(|e| e.key == *key)
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        self.clock += 1;
        let clock = self.clock;
        self.entries.iter_mut().find(|e| e.key == *key).map(|e| {
            e.used = clock;
            e.value.clone()
        })
    }

    pub fn find(&self, keep: impl Fn(&K) -> bool) -> Vec<(&K, V)> {
        self.entries
            .iter()
            .filter(|e| keep(&e.key))
            .map(|e| (&e.key, e.value.clone()))
            .collect()
    }

    pub fn insert(&mut self, key: K, value: V, bytes: usize) -> Vec<V> {
        let mut out = self.remove(|k| *k == key);
        self.clock += 1;
        self.entries.push(Entry {
            key,
            value,
            bytes,
            used: self.clock,
        });
        self.bytes += bytes;
        while self.bytes > self.budget && self.entries.len() > 1 {
            let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.used)
                .map(|(i, _)| i)
            else {
                break;
            };
            let entry = self.entries.swap_remove(oldest);
            self.bytes -= entry.bytes;
            out.push(entry.value);
        }
        out
    }

    pub fn remove(&mut self, matches: impl Fn(&K) -> bool) -> Vec<V> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.entries.len() {
            if matches(&self.entries[i].key) {
                let entry = self.entries.swap_remove(i);
                self.bytes -= entry.bytes;
                out.push(entry.value);
            } else {
                i += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_stays_under_its_bound_and_drops_the_oldest_first() {
        let mut cache: Cache<u32, u32> = Cache::new(100);
        assert!(cache.insert(1, 1, 40).is_empty());
        assert!(cache.insert(2, 2, 40).is_empty());
        assert_eq!(cache.get(&1), Some(1));
        assert_eq!(cache.insert(3, 3, 40), vec![2]);
        assert!(cache.bytes() <= 100);
        assert!(cache.contains(&1) && cache.contains(&3) && !cache.contains(&2));
        for page in 4..50 {
            cache.insert(page, page, 30);
            assert!(cache.bytes() <= 100, "{}", cache.bytes());
        }
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn an_entry_larger_than_the_bound_replaces_everything_and_stays() {
        let mut cache: Cache<u32, u32> = Cache::new(100);
        cache.insert(1, 1, 60);
        let mut out = cache.insert(2, 2, 500);
        out.sort();
        assert_eq!(out, vec![1]);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.insert(2, 20, 10), vec![2]);
        assert_eq!(cache.bytes(), 10);
        assert_eq!(cache.remove(|_| true), vec![20]);
        assert_eq!(cache.bytes(), 0);
    }
}
