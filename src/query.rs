use crate::{
    as_rc, client::QueryOptions, AsKey, DataSignal, Fetcher, QueryClient, QueryData, Status,
};
use fluvio_wasm_timer::Delay;
use std::any::Any;
use std::{future::Future, rc::Rc};
use sycamore::{
    futures::spawn_local,
    reactive::{
        create_memo, create_rc_signal, create_ref, use_context, RcSignal, ReadSignal, Scope, Signal,
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
    pub debug: RcSignal<String>,
}

impl QueryClient {
    pub(crate) fn find_query(
        &self,
        key: &[u64],
    ) -> Option<(Rc<DataSignal>, Rc<Signal<Status>>, Fetcher)> {
        let data = self.data_signals.read().unwrap().get(key)?;
        let status = self.status_signals.read().unwrap().get(key)?;
        let fetcher = self.fetchers.read().unwrap().get(key)?.clone();
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
        } else if *status.get() != Status::Fetching {
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
                if let QueryData::Ok(data) = data.get().as_ref() {
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
/// the same top level key.
/// * `fetcher` - The asynchronous function used to fetch the data. This needs
/// to be static because it's stored and automatically rerun if the data in the
/// cache is stale or the query is invalidated.
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
pub fn use_query<K, T, E, F, R>(
    cx: Scope<'_>,
    key: K,
    fetcher: F,
) -> Query<'_, T, E, impl Fn() + '_>
where
    K: AsKey,
    F: Fn() -> R + 'static,
    R: Future<Output = Result<T, E>> + 'static,
    T: 'static,
    E: 'static,
{
    use_query_with_options(cx, key, fetcher, QueryOptions::default())
}

/// Use a query to fetch remote data with extra options.
/// For more information see [`use_query`] and [`QueryOptions`].
pub fn use_query_with_options<K, T, E, F, R>(
    cx: Scope<'_>,
    key: K,
    fetcher: F,
    options: QueryOptions,
) -> Query<'_, T, E, impl Fn() + '_>
where
    K: AsKey,
    F: Fn() -> R + 'static,
    R: Future<Output = Result<T, E>> + 'static,
    T: 'static,
    E: 'static,
{
    let key = key.as_key();

    let client = use_context::<Rc<QueryClient>>(cx).clone();
    let debug = create_rc_signal("".to_string());
    let (data, status, fetcher) = if let Some(query) = client.find_query(&key) {
        debug.modify().push_str("\nQuery already exists");
        query
    } else {
        debug.modify().push_str("\nCreating query");
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
        client.insert_query(key.clone(), data.clone(), status.clone(), fetcher.clone());
        (data, status, fetcher)
    };

    client.clone().run_query(
        &key,
        data.clone(),
        status.clone(),
        fetcher.clone(),
        &options,
    );

    let refetch = create_ref(cx, move || {
        client.clone().refetch_query(&key);
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
        debug,
    }
}
