use crate::{client::QueryOptions, DynQueryData, QueryData};
use fnv::FnvHashMap;
use std::{
    any::Any,
    rc::Rc,
    time::{Duration, Instant},
};

type Cache = FnvHashMap<Vec<u64>, CacheEntry>;

pub struct CacheEntry {
    created_at: Instant,
    lifetime: Duration,
    value: Rc<DynQueryData>,
}

#[derive(Default)]
pub struct QueryCache {
    inner: Cache,
}

impl QueryCache {
    pub fn get(
        &self,
        id: &[u64],
        options: &QueryOptions,
    ) -> Option<Rc<QueryData<Rc<dyn Any>, Rc<dyn Any>>>> {
        let entry = self.inner.get(id)?;
        let age = Instant::now().duration_since(entry.created_at);
        if age > options.cache_expiration {
            None
        } else {
            Some(entry.value.clone())
        }
    }

    pub fn insert(
        &mut self,
        id: Vec<u64>,
        value: Rc<QueryData<Rc<dyn Any>, Rc<dyn Any>>>,
        options: &QueryOptions,
    ) -> Rc<QueryData<Rc<dyn Any>, Rc<dyn Any>>> {
        self.inner.insert(
            id,
            CacheEntry {
                created_at: Instant::now(),
                lifetime: options.cache_expiration,
                value: value.clone(),
            },
        );
        value
    }

    pub fn invalidate_keys(&mut self, keys: &[&[u64]]) {
        self.inner
            .retain(|key, _| keys.iter().any(|&prefix| key.starts_with(prefix)));
    }

    pub fn collect_garbage(&mut self) {
        self.inner
            .retain(|_, entry| Instant::now().duration_since(entry.created_at) < entry.lifetime);
    }
}
