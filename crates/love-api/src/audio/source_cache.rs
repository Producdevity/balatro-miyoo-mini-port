use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

#[derive(Default)]
pub(super) struct SourceCache(HashMap<String, Weak<[u8]>>);

impl SourceCache {
    pub(super) fn get_or_load(
        &mut self,
        path: &str,
        load: impl FnOnce() -> Result<Vec<u8>>,
    ) -> Result<Arc<[u8]>> {
        if let Some(data) = self.0.get(path).and_then(Weak::upgrade) {
            return Ok(data);
        }
        let data: Arc<[u8]> = load()?.into();
        // Sources, not this lookup, own the compressed bytes.
        self.0.retain(|_, data| data.strong_count() > 0);
        self.0.insert(path.to_owned(), Arc::downgrade(&data));
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_sources_share_bytes_without_retaining_finished_assets() {
        let mut cache = SourceCache::default();
        let first = cache
            .get_or_load("music.ogg", || Ok(vec![1, 2, 3]))
            .unwrap();
        let again = cache
            .get_or_load("music.ogg", || panic!("asset was read twice"))
            .unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        let weak = Arc::downgrade(&first);
        drop(first);
        drop(again);
        assert!(weak.upgrade().is_none());
        let reloaded = cache.get_or_load("music.ogg", || Ok(vec![4, 5])).unwrap();
        assert_eq!(&*reloaded, &[4, 5]);
    }
}
