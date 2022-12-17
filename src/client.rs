use std::{
    hash::Hasher,
    rc::{Rc, Weak},
    sync::RwLock,
    time::Duration,
};

use fnv::FnvHasher;
use prehash::{Prehashed, PrehashedMap};
use std::hash::Hash;
use sycamore::reactive::Signal;
use weak_table::WeakValueHashMap;

use crate::{cache::QueryCache, DataSignal, DynQueryData, Fetcher, QueryData, Status};

#[derive(Clone)]
pub struct QueryOptions {
    pub cache_expiration: Duration,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            cache_expiration: Duration::from_secs(5 * 60),
        }
    }
}

#[derive(Default)]
pub struct QueryClient {
    pub default_options: QueryOptions,
    pub(crate) cache: RwLock<QueryCache>,
    pub(crate) data_signals: RwLock<WeakValueHashMap<Prehashed<()>, Weak<DataSignal>>>,
    pub(crate) status_signals: RwLock<WeakValueHashMap<Prehashed<()>, Weak<Signal<Status>>>>,
    pub(crate) fetchers: RwLock<PrehashedMap<(), Fetcher>>,
}

impl QueryClient {
    pub fn new(default_options: QueryOptions) -> Self {
        Self {
            default_options,
            cache: Default::default(),
            data_signals: Default::default(),
            status_signals: Default::default(),
            fetchers: Default::default(),
        }
    }

    pub fn invalidate_queries(self: Rc<Self>, queries: Vec<u64>) {
        self.cache.write().unwrap().invalidate_keys(&queries);
        for query in queries {
            if let Some((data, status, fetcher)) = self.find_query(query) {
                self.clone()
                    .run_query(query, data, status, fetcher, &self.default_options);
            }
        }
    }

    pub fn collect_garbage(&self) {
        self.cache.write().unwrap().collect_garbage();
        // Queries get collected automatically, make sure to also collect fetchers
        let queries = self.status_signals.read().unwrap();
        self.fetchers
            .write()
            .unwrap()
            .retain(|k, _| queries.contains_key(k));
    }

    pub fn query_data<K: Hash, T: 'static, E: 'static>(
        &self,
        key: K,
    ) -> Option<QueryData<Rc<T>, Rc<E>>> {
        let mut hash = FnvHasher::default();
        key.hash(&mut hash);
        let key = hash.finish();
        let data = self
            .cache
            .read()
            .unwrap()
            .get(key, &QueryOptions::default())?;
        Some(match data.as_ref() {
            QueryData::Loading => QueryData::Loading,
            QueryData::Ok(ok) => QueryData::Ok(ok.clone().downcast().unwrap()),
            QueryData::Err(err) => QueryData::Err(err.clone().downcast().unwrap()),
        })
    }

    pub fn set_query_data<K: Hash, T: 'static, E: 'static>(&self, key: K, value: QueryData<T, E>) {
        let mut hash = FnvHasher::default();
        key.hash(&mut hash);
        let key = hash.finish();
        let value: Rc<DynQueryData> = Rc::new(match value {
            QueryData::Loading => QueryData::Loading,
            QueryData::Ok(ok) => QueryData::Ok(Rc::new(ok)),
            QueryData::Err(err) => QueryData::Err(Rc::new(err)),
        });
        self.cache
            .write()
            .unwrap()
            .insert(key, value, &QueryOptions::default());
    }
}
