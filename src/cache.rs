use prehash::{Prehashed, PrehashedMap};
use std::{
    any::Any,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::{client::QueryOptions, QueryData};

type Cache = PrehashedMap<(), CacheEntry>;

pub struct CacheEntry {
    created_at: Instant,
    lifetime: Duration,
    value: Rc<QueryData<Rc<dyn Any>, Rc<dyn Any>>>,
}

#[derive(Default)]
pub struct QueryCache {
    inner: Cache,
}

impl QueryCache {
    pub fn get(
        &self,
        id: u64,
        options: &QueryOptions,
    ) -> Option<Rc<QueryData<Rc<dyn Any>, Rc<dyn Any>>>> {
        let key = Prehashed::new((), id);
        let entry = self.inner.get(&key)?;
        let age = Instant::now().duration_since(entry.created_at);
        if age > options.cache_expiration {
            None
        } else {
            Some(entry.value.clone())
        }
    }

    pub fn insert(
        &mut self,
        id: u64,
        value: Rc<QueryData<Rc<dyn Any>, Rc<dyn Any>>>,
        options: &QueryOptions,
    ) -> Rc<QueryData<Rc<dyn Any>, Rc<dyn Any>>> {
        self.inner.insert(
            Prehashed::new((), id),
            CacheEntry {
                created_at: Instant::now(),
                lifetime: options.cache_expiration,
                value: value.clone(),
            },
        );
        value
    }

    pub fn invalidate_keys(&mut self, keys: &[u64]) {
        self.inner
            .retain(|key, _| keys.contains(Prehashed::<()>::as_hash(key)))
    }

    pub fn collect_garbage(&mut self) {
        self.inner
            .retain(|_, entry| Instant::now().duration_since(entry.created_at) < entry.lifetime);
    }
}
