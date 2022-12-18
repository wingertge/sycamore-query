use fnv::{FnvBuildHasher, FnvHashMap};
use std::{
    rc::{Rc, Weak},
    sync::RwLock,
    time::Duration,
};
use sycamore::reactive::Signal;
use weak_table::WeakValueHashMap;

use crate::{cache::QueryCache, AsKey, DataSignal, Fetcher, Status};

#[derive(Clone)]
pub struct ClientOptions {
    pub cache_expiration: Duration,
    pub retries: u32,
    pub retry_fn: Rc<dyn Fn(u32) -> Duration>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            cache_expiration: Duration::from_secs(5 * 60),
            retries: 3,
            retry_fn: Rc::new(|retries| {
                Duration::from_secs((1 ^ (2 * retries)).clamp(0, 30) as u64)
            }),
        }
    }
}

impl ClientOptions {
    pub(crate) fn merge(&self, query_options: &QueryOptions) -> ClientOptions {
        Self {
            cache_expiration: query_options
                .cache_expiration
                .clone()
                .unwrap_or_else(|| self.cache_expiration.clone()),
            retries: query_options.retries.unwrap_or(self.retries),
            retry_fn: query_options
                .retry_fn
                .clone()
                .unwrap_or_else(|| self.retry_fn.clone()),
        }
    }
}

#[derive(Default)]
pub struct QueryOptions {
    pub cache_expiration: Option<Duration>,
    pub retries: Option<u32>,
    pub retry_fn: Option<Rc<dyn Fn(u32) -> Duration>>,
}

type WeakFnvMap<T> = WeakValueHashMap<Vec<u64>, Weak<T>, FnvBuildHasher>;

#[derive(Default)]
pub struct QueryClient {
    pub default_options: ClientOptions,
    pub(crate) cache: RwLock<QueryCache>,
    pub(crate) data_signals: RwLock<WeakFnvMap<DataSignal>>,
    pub(crate) status_signals: RwLock<WeakFnvMap<Signal<Status>>>,
    pub(crate) fetchers: RwLock<FnvHashMap<Vec<u64>, Fetcher>>,
}

impl QueryClient {
    pub fn new(default_options: ClientOptions) -> Self {
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
                    .run_query(query, data, status, fetcher, &QueryOptions::default());
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

    pub fn query_data<K: AsKey, T: 'static>(&self, key: K) -> Option<Rc<T>> {
        let data = self.cache.read().unwrap().get(&key.as_key())?;
        Some(data.clone().downcast().unwrap())
    }

    pub fn set_query_data<K: AsKey, T: 'static>(&self, key: K, value: T) {
        self.cache
            .write()
            .unwrap()
            .insert(key.as_key(), Rc::new(value), &self.default_options);
    }
}
