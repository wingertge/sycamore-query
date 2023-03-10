use crate::{
    as_rc, client::QueryOptions, AsKeys, DataSignal, Fetcher, QueryClient, QueryData, Status,
};
use fluvio_wasm_timer::Delay;
use std::any::Any;
use std::{future::Future, rc::Rc};
use sycamore::{
    futures::spawn_local,
    reactive::{
        create_effect, create_memo, create_rc_signal, create_ref, create_selector, use_context,
        ReadSignal, Scope, Signal,
    },
};

/// The struct representing a query
///
/// # Example
///
/// ```
/// # use sycamore::prelude::*;
/// # use sycamore_query::{*, query::{Query, use_query}};
/// # #[component]
/// # pub fn App<G: Html>(cx: Scope) -> View<G> {
/// #   provide_context(cx, QueryClient::new(ClientOptions::default()));
/// let Query { data, status, refetch } = use_query(
///     cx,
///     ("hello", "World"),
///     || async { Result::<_, ()>::Ok("World".to_string()) }
/// );
///
/// # view! { cx, }
/// # }
/// ```
pub struct Query<'a, T, E, F: Fn()> {
    /// The data returned by the query. See [`QueryData`].
    pub data: &'a ReadSignal<QueryData<Rc<T>, Rc<E>>>,
    /// The status of the query. See [`Status`].
    pub status: Rc<Signal<Status>>,
    /// A function to trigger a refetch of the query and all queries with the
    /// same key.
    pub refetch: &'a F,
}

impl QueryClient {
    pub(crate) fn find_query(
        &self,
        key: &[u64],
        new_hook: bool,
    ) -> Option<(Rc<DataSignal>, Rc<Signal<Status>>, Fetcher)> {
        let data = self.data_signals.read().unwrap().get(key);
        let status = self.status_signals.read().unwrap().get(key);
        let fetcher = self.fetchers.read().unwrap().get(key)?.clone();
        let (data, status) = match (data, status) {
            (None, None) => None,
            (None, Some(status)) => {
                let data = if let Some(data) = self.cache.read().unwrap().get(key) {
                    QueryData::Ok(data)
                } else {
                    QueryData::Loading
                };
                let data = as_rc(create_rc_signal(data));
                if new_hook {
                    self.data_signals
                        .write()
                        .unwrap()
                        .insert(key.to_vec(), data.clone());
                }
                Some((data, status))
            }
            (Some(data), None) => {
                let status = as_rc(create_rc_signal(Status::Success));
                if new_hook {
                    self.status_signals
                        .write()
                        .unwrap()
                        .insert(key.to_vec(), status.clone());
                }
                Some((data, status))
            }
            (Some(data), Some(status)) => Some((data, status)),
        }?;
        Some((data, status, fetcher))
    }

    pub(crate) fn insert_query(
        &self,
        key: Vec<u64>,
        data: Rc<DataSignal>,
        status: Rc<Signal<Status>>,
        fetcher: Fetcher,
    ) {
        self.data_signals.write().unwrap().insert(key.clone(), data);
        self.status_signals
            .write()
            .unwrap()
            .insert(key.clone(), status);
        self.fetchers.write().unwrap().insert(key, fetcher);
    }

    pub(crate) fn run_query(
        self: Rc<Self>,
        key: &[u64],
        data: Rc<DataSignal>,
        status: Rc<Signal<Status>>,
        fetcher: Fetcher,
        options: &QueryOptions,
    ) {
        let options = self.default_options.merge(options);
        if let Some(cached) = {
            let cache = self.cache.read().unwrap();
            cache.get(key)
        } {
            data.set(QueryData::Ok(cached));
            self.clone().invalidate_queries(vec![key.to_vec()]);
        } else if *status.get_untracked() != Status::Fetching {
            status.set(Status::Fetching);
            let key = key.to_vec();
            spawn_local(async move {
                let mut res = fetcher().await;
                let mut retries = 0;
                while res.is_err() && retries < options.retries {
                    Delay::new((options.retry_fn)(retries)).await.unwrap();
                    res = fetcher().await;
                    retries += 1;
                }
                data.set(res.map_or_else(QueryData::Err, QueryData::Ok));
                if let QueryData::Ok(data) = data.get_untracked().as_ref() {
                    self.cache
                        .write()
                        .unwrap()
                        .insert(key, data.clone(), &options);
                }
                status.set(Status::Success);
            });
        }
    }

    pub(crate) fn refetch_query(self: Rc<Self>, key: &[u64]) {
        self.invalidate_queries(vec![key.to_vec()]);
    }
}

/// Use a query to load remote data and keep it up to date.
///
/// # Parameters
///
/// * `cx` - The Scope of the containing component
/// * `key` - A unique key for this query. Any queries sharing this key will
/// have the same data and status signals. If your query takes arguments, it's
/// expected to add them to the key tuple. Keys in your key tuple only need to
/// implement `Hash`. Using a key tuple is preferrable to using a formatted
/// string because the tuple allows for invalidating groups of queries that share
/// the same top level key. Why is this a closure instead of a value? Because I need to track the
/// signals used in it. There is a more ergonomic implementation but it requires specialization or
/// a change in sycamore's `Hash` implementation.
/// * `fetcher` - The asynchronous function used to fetch the data. This needs
/// to be static because it's stored and automatically rerun if the data in the
/// cache is stale or the query is invalidated.
///
/// # Signals in Keys
///
/// Currently, Sycamore uses the `untracked_get` function in its [`Hash`](std::hash::Hash)
/// implementation for signals. This means changes won't be tracked by default. If you want the
/// query to refetch every time the signal in the key changes, use `signal.key()`/`signal.rc_key()`
/// from the [`AsKeySignal`](crate::AsKeySignal) and [`AsRcKeySignal`](crate::AsRcKeySignal) traits
/// respectively.
///
/// # Example
///
/// ```
/// # use sycamore::prelude::*;
/// # use sycamore_query::{*, query::{Query, use_query}};
/// # #[component]
/// # pub fn App<G: Html>(cx: Scope) -> View<G> {
/// #   provide_context(cx, QueryClient::new(ClientOptions::default()));
/// let Query { data, status, refetch } = use_query(
///     cx,
///     ("hello", "World"),
///     || async { Result::<_, ()>::Ok("World".to_string()) }
/// );
///
/// # view! { cx, }
/// # }
/// ```
///
/// # Notes
///
/// This will crash your application if two queries with the same key but different
/// types are used. Data is stored as `Rc<dyn Any>` internally and downcast for
/// each `use_query` invocation. If the type doesn't match, it will panic. This
/// shouldn't be a problem because different queries should never have exactly
/// the same key, but it's worth noting.
///
pub fn use_query<'a, K, T, E, F, R>(
    cx: Scope<'a>,
    key: K,
    fetcher: F,
) -> Query<'a, T, E, impl Fn() + 'a>
where
    K: AsKeys + 'a,
    F: Fn() -> R + 'static,
    R: Future<Output = Result<T, E>> + 'static,
    T: 'static,
    E: 'static,
{
    use_query_with_options(cx, key, fetcher, QueryOptions::default())
}

/// Use a query to fetch remote data with extra options.
/// For more information see [`use_query`] and [`QueryOptions`].
pub fn use_query_with_options<'a, K, T, E, F, R>(
    cx: Scope<'a>,
    key: K,
    fetcher: F,
    options: QueryOptions,
) -> Query<'a, T, E, impl Fn() + 'a>
where
    K: AsKeys + 'a,
    F: Fn() -> R + 'static,
    R: Future<Output = Result<T, E>> + 'static,
    T: 'static,
    E: 'static,
{
    let id = create_selector(cx, move || key.as_keys());

    let client = use_context::<Rc<QueryClient>>(cx).clone();
    let (data, status, fetcher) = if let Some(query) = client.find_query(&id.get(), true) {
        query
    } else {
        let data: Rc<DataSignal> = as_rc(create_rc_signal(QueryData::Loading));
        let status = as_rc(create_rc_signal(Status::Idle));
        let fetcher: Fetcher = Rc::new(move || {
            let fut = fetcher();
            Box::pin(async move {
                fut.await
                    .map(|data| -> Rc<dyn Any> { Rc::new(data) })
                    .map_err(|err| -> Rc<dyn Any> { Rc::new(err) })
            })
        });
        client.insert_query(
            id.get().as_ref().clone(),
            data.clone(),
            status.clone(),
            fetcher.clone(),
        );
        (data, status, fetcher)
    };

    {
        let client = client.clone();
        let data = data.clone();
        let status = status.clone();
        create_effect(cx, move || {
            log::info!("Key changed. New key: {:?}", id.get());
            client.clone().run_query(
                &id.get(),
                data.clone(),
                status.clone(),
                fetcher.clone(),
                &options,
            );
        });
    }

    let refetch = create_ref(cx, move || {
        client.clone().refetch_query(&id.get());
    });
    let data = create_memo(cx, move || match data.get().as_ref() {
        QueryData::Loading => QueryData::Loading,
        QueryData::Ok(data) => QueryData::Ok(data.clone().downcast().unwrap()),
        QueryData::Err(err) => QueryData::Err(err.clone().downcast().unwrap()),
    });

    Query {
        data,
        status,
        refetch,
    }
}
