use fnv::{FnvBuildHasher, FnvHashMap};
use std::{
    rc::{Rc, Weak},
    sync::RwLock,
    time::Duration,
};
use sycamore::reactive::Signal;
use weak_table::WeakValueHashMap;

use crate::{cache::QueryCache, AsKeys, DataSignal, Fetcher, QueryData, Status};

/// Global query options.
/// These can be overridden on a per query basis with [`QueryOptions`].
///
/// # Options
///
/// * `cache_expiration` - The time before a cached query result expires.
/// Default: 5 minutes
/// * `retries` - The number of times to retry a query if it fails. Default: 3
/// * `retry_fn` - The function for the timeout between retries. Defaults to
/// exponential delay starting with 1 second, but not going over 30 seconds.
///
#[derive(Clone)]
pub struct ClientOptions {
    /// The time before a cached query result expires. Default: 5 minutes
    pub cache_expiration: Duration,
    /// The number of times to retry a query if it fails. Default: 3
    pub retries: u32,
    /// The function for the timeout between retries. Defaults to
    /// exponential delay starting with 1 second, but not going over 30 seconds.
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
                .unwrap_or(self.cache_expiration),
            retries: query_options.retries.unwrap_or(self.retries),
            retry_fn: query_options
                .retry_fn
                .clone()
                .unwrap_or_else(|| self.retry_fn.clone()),
        }
    }
}

/// Query-specific options that override the global [`ClientOptions`].
/// Any fields that are not set are defaulted to the [`QueryClient`]'s settings.
///
/// # Options
///
/// * `cache_expiration` - The time before a cached query result expires.
/// * `retries` - The number of times to retry a query if it fails. Default: 3
/// * `retry_fn` - The function for the timeout between retries. Defaults to
/// exponential delay starting with 1 second, but not going over 30 seconds.
///
#[derive(Default)]
pub struct QueryOptions {
    /// The time before a cached query result expires. Default: 5 minutes
    pub cache_expiration: Option<Duration>,
    /// The number of times to retry a query if it fails. Default: 3
    pub retries: Option<u32>,
    /// The function for the timeout between retries. Defaults to
    /// exponential delay starting with 1 second, but not going over 30 seconds.
    pub retry_fn: Option<Rc<dyn Fn(u32) -> Duration>>,
}

type WeakFnvMap<T> = WeakValueHashMap<Vec<u64>, Weak<T>, FnvBuildHasher>;

/// The query client for `sycamore-query`. This stores your default settings,
/// the cache and all queries that need to be updated when a query is refetched
/// or updated. The client needs to be provided as a Context object in your top
/// level component (`sycamore`) or index view (`perseus`).
/// # Example
///
/// ```
/// # use sycamore::prelude::*;
/// # use sycamore_query::*;
///
/// #[component]
/// pub fn App<G: Html>(cx: Scope) -> View<G> {
///     let client = QueryClient::new(ClientOptions::default());
///     provide_context(cx, client);
///
///     // You can now use the sycamore-query hooks
///     view! { cx, }
/// }
/// ```
///
#[derive(Default)]
pub struct QueryClient {
    pub(crate) default_options: ClientOptions,
    pub(crate) cache: RwLock<QueryCache>,
    pub(crate) data_signals: RwLock<WeakFnvMap<DataSignal>>,
    pub(crate) status_signals: RwLock<WeakFnvMap<Signal<Status>>>,
    pub(crate) fetchers: RwLock<FnvHashMap<Vec<u64>, Fetcher>>,
}

impl QueryClient {
    /// Creates a new QueryClient.
    ///
    /// # Arguments
    /// * `default_options` - The global query options.
    ///
    /// # Example
    ///
    /// ```
    /// # use sycamore_query::*;
    /// let client = QueryClient::new(ClientOptions::default());
    /// ```
    pub fn new(default_options: ClientOptions) -> Rc<Self> {
        Rc::new(Self {
            default_options,
            ..QueryClient::default()
        })
    }

    /// Invalidate all queries whose keys start with any of the keys passed in.
    /// For example, passing a top level query ID will invalidate all queries
    /// with that top level ID, regardless of their arguments.
    /// For passing multiple keys with tuple types, see [`keys!`](crate::keys).
    ///
    /// # Example
    ///
    /// ```
    /// # use sycamore_query::*;
    /// # let client = QueryClient::new(ClientOptions::default());
    /// // This will invalidate all queries whose keys start with `"hello"`,
    /// // or where the first key is `"user"` and the first argument `3`
    /// client.invalidate_queries(keys!["hello", ("user", 3)]);
    /// ```
    ///
    pub fn invalidate_queries(self: Rc<Self>, queries: Vec<Vec<u64>>) {
        let queries = queries
            .iter()
            .map(|query| query.as_slice())
            .collect::<Vec<_>>();
        self.cache.write().unwrap().invalidate_keys(&queries);
        log::info!(
            "Invalidating queries: {queries:?}. Queries in cache: {:?}",
            self.data_signals.read().unwrap().keys().collect::<Vec<_>>()
        );
        for query in self
            .data_signals
            .read()
            .unwrap()
            .keys()
            .filter(|k| queries.iter().any(|key| k.starts_with(key)))
        {
            log::info!("Updating query {query:?}");
            if let Some((data, status, fetcher)) = self.find_query(query, false) {
                log::info!("Query present. Running fetch.");
                self.clone()
                    .run_query(query, data, status, fetcher, &QueryOptions::default());
            }
        }
    }

    /// Collect garbage from the client cache
    /// Call this whenever a lot of queries have been removed (i.e. on going to
    /// a different page) to keep memory usage low.
    /// Alternatively you could call this on a timer with the same length as your
    /// cache expiration time.
    ///
    /// This will iterate through the entire cache sequentially, so don't use
    /// on every frame.
    pub fn collect_garbage(&self) {
        self.cache.write().unwrap().collect_garbage();
        // Queries get collected automatically, make sure to also collect fetchers
        let queries = self.status_signals.read().unwrap();
        self.fetchers
            .write()
            .unwrap()
            .retain(|k, _| queries.contains_key(k));
    }

    /// Fetch query data from the cache if it exists. If it doesn't or the data
    /// is expired, this will return `None`.
    pub fn query_data<K: AsKeys, T: 'static>(&self, key: K) -> Option<Rc<T>> {
        let data = self.cache.read().unwrap().get(&key.as_keys())?;
        Some(data.clone().downcast().unwrap())
    }

    /// Override the query data in the cache for a given key. This will update
    /// all queries with the same key automatically to reflect the new data.
    pub fn set_query_data<K: AsKeys, T: 'static>(&self, key: K, value: T) {
        let key = key.as_keys();
        let value = Rc::new(value);
        if let Some(data) = self.data_signals.read().unwrap().get(&key) {
            data.set(QueryData::Ok(value.clone()))
        }
        self.cache
            .write()
            .unwrap()
            .insert(key, Rc::new(value), &self.default_options);
    }
}
