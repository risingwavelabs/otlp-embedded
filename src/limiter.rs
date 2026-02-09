use get_size2::GetSize;
use schnellru::Limiter;

/// Limit both the number of elements and the memory usage of the map.
pub(crate) struct MyLimiter {
    current_mem: usize,
    max_mem: usize,

    max_length: u32,
}

impl MyLimiter {
    pub fn new(max_mem: usize, max_length: u32) -> Self {
        Self {
            current_mem: 0,
            max_mem,
            max_length,
        }
    }

    pub fn estimated_memory_usage(&self) -> usize {
        self.current_mem
    }
}

impl<K, V> Limiter<K, V> for MyLimiter
where
    K: GetSize,
    V: GetSize,
{
    type KeyToInsert<'a> = K;
    type LinkType = u32;

    fn is_over_the_limit(&self, length: usize) -> bool {
        length > self.max_length as usize || self.current_mem > self.max_mem
    }

    fn on_insert(
        &mut self,
        _length: usize,
        key: Self::KeyToInsert<'_>,
        value: V,
    ) -> Option<(K, V)> {
        if self.max_length > 0 {
            // Do not reject new inserts due to memory usage.
            // Instead, evict the oldest entry by telling `is_over_the_limit`.
            let mem = key.get_heap_size() + value.get_heap_size();
            self.current_mem += mem;

            Some((key, value))
        } else {
            None
        }
    }

    fn on_replace(
        &mut self,
        _length: usize,
        _old_key: &mut K,
        _new_key: Self::KeyToInsert<'_>,
        _old_value: &mut V,
        _new_value: &mut V,
    ) -> bool {
        // We never call this.
        unreachable!()
    }

    fn on_removed(&mut self, key: &mut K, value: &mut V) {
        let mem = key.get_heap_size() + value.get_heap_size();
        self.current_mem -= mem;
    }

    fn on_cleared(&mut self) {
        self.current_mem = 0;
    }

    fn on_grow(&mut self, _new_memory_usage: usize) -> bool {
        // We don't care about the memory overhead of the map itself.
        true
    }
}
