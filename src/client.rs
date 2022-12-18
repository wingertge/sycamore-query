use fnv::{FnvBuildHasher, FnvHashMap};
use std::{
    rc::{Rc, Weak},
    sync::RwLock,
    time::Duration,
};
use sycamore::reactive::Signal;
use weak_table::WeakValueHashMap;

use crate::{cache::QueryCache, AsKey, DataSignal, DynQueryData, Fetcher, QueryData, Status};

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

type WeakFnvMap<T> = WeakValueHashMap<Vec<u64>, Weak<T>, FnvBuildHasher>;

#[derive(Default)]
pub struct QueryClient {
    pub default_options: QueryOptions,
    pub(crate) cache: RwLock<QueryCache>,
    pub(crate) data_signals: RwLock<WeakFnvMap<DataSignal>>,
    pub(crate) status_signals: RwLock<WeakFnvMap<Signal<Status>>>,
    pub(crate) fetchers: RwLock<FnvHashMap<Vec<u64>, Fetcher>>,
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

    pub fn invalidate_queries(self: Rc<Self>, queries: &[&[u64]]) {
        self.cache.write().unwrap().invalidate_keys(queries);
        for &query in queries {
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

    pub fn query_data<K: AsKey, T: 'static, E: 'static>(
        &self,
        key: K,
    ) -> Option<QueryData<Rc<T>, Rc<E>>> {
        let data = self
            .cache
            .read()
            .unwrap()
            .get(&key.as_key(), &QueryOptions::default())?;
        Some(match data.as_ref() {
            QueryData::Loading => QueryData::Loading,
            QueryData::Ok(ok) => QueryData::Ok(ok.clone().downcast().unwrap()),
            QueryData::Err(err) => QueryData::Err(err.clone().downcast().unwrap()),
        })
    }

    pub fn set_query_data<K: AsKey, T: 'static, E: 'static>(&self, key: K, value: QueryData<T, E>) {
        let value: Rc<DynQueryData> = Rc::new(match value {
            QueryData::Loading => QueryData::Loading,
            QueryData::Ok(ok) => QueryData::Ok(Rc::new(ok)),
            QueryData::Err(err) => QueryData::Err(Rc::new(err)),
        });
        self.cache
            .write()
            .unwrap()
            .insert(key.as_key(), value, &QueryOptions::default());
    }
}
