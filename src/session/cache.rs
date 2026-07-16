use std::collections::HashMap;

pub struct Cache<V> {
    entries: HashMap<String, V>,
    order: Vec<String>,
    max_size: usize,
}

impl<V> Cache<V> {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            max_size,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&V> {
        if self.entries.contains_key(key) {
            self.touch(key);
            self.entries.get(key)
        } else {
            None
        }
    }

    pub fn put(&mut self, key: &str, val: V) {
        if !self.entries.contains_key(key) {
            while self.entries.len() >= self.max_size && !self.order.is_empty() {
                let evict = self.order.remove(0);
                self.entries.remove(&evict);
            }
            self.entries.insert(key.to_string(), val);
        }
        self.touch(key);
    }

    pub fn remove(&mut self, key: &str) {
        self.entries.remove(key);
        self.order.retain(|k| k != key);
    }

    fn touch(&mut self, key: &str) {
        self.order.retain(|k| k != key);
        self.order.push(key.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_eviction() {
        let mut c = Cache::new(3);
        c.put("a", 1);
        c.put("b", 2);
        c.put("c", 3);
        c.put("d", 4);
        assert!(c.get("a").is_none());
        assert_eq!(*c.get("b").unwrap(), 2);
    }

    #[test]
    fn test_touch_on_read() {
        let mut c = Cache::new(2);
        c.put("a", 1);
        c.put("b", 2);
        assert_eq!(*c.get("a").unwrap(), 1);
        c.put("c", 3);
        assert_eq!(*c.get("a").unwrap(), 1);
        assert!(c.get("b").is_none());
    }

    #[test]
    fn test_put_noop_on_existing_key() {
        let mut c = Cache::new(2);
        c.put("a", 1);
        c.put("b", 2);
        c.put("a", 99);
        assert_eq!(*c.get("a").unwrap(), 1);
        c.put("c", 3);
        assert!(c.get("b").is_none());
        assert_eq!(*c.get("a").unwrap(), 1);
    }

    #[test]
    fn test_remove() {
        let mut c = Cache::new(3);
        c.put("a", 1);
        c.put("b", 2);
        c.remove("a");
        assert!(c.get("a").is_none());
        assert_eq!(*c.get("b").unwrap(), 2);
    }

    #[test]
    fn test_get_miss() {
        let mut c = Cache::<i32>::new(1);
        assert!(c.get("nonexistent").is_none());
    }

    #[test]
    fn test_eviction_order_with_touch() {
        let mut c = Cache::new(2);
        c.put("a", 1);
        c.put("b", 2);
        c.get("a");
        c.put("c", 3);
        assert!(c.get("b").is_none());
        assert_eq!(*c.get("a").unwrap(), 1);
        assert_eq!(*c.get("c").unwrap(), 3);
    }
}
