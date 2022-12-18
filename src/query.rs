use crate::{
    as_rc, client::QueryOptions, AsKey, DataSignal, Fetcher, QueryClient, QueryData, Status,
};
use futures_timer::Delay;
use std::any::Any;
use std::{future::Future, rc::Rc};
use sycamore::{
    futures::spawn_local,
    reactive::{create_memo, create_rc_signal, create_ref, use_context, ReadSignal, Scope, Signal},
};

pub struct Query<'a, T, E, F: Fn()> {
    pub data: &'a ReadSignal<QueryData<Rc<T>, Rc<E>>>,
    pub status: Rc<Signal<Status>>,
    pub refetch: &'a F,
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

    pub fn run_query(
        self: Rc<Self>,
        key: &[u64],
        data: Rc<Signal<QueryData<Rc<dyn Any>, Rc<dyn Any>>>>,
        status: Rc<Signal<Status>>,
        fetcher: Fetcher,
        options: &QueryOptions,
    ) {
        let options = self.default_options.merge(options);
        if let Some(cached) = {
            let cache = self.cache.read().unwrap();
            cache.get(&key)
        } {
            data.set(QueryData::Ok(cached));
            self.clone().invalidate_queries(&vec![key]);
        } else if *status.get() != Status::Fetching {
            status.set(Status::Fetching);
            let options = options.clone();
            let key = key.to_vec();
            spawn_local(async move {
                let mut res = fetcher().await;
                let mut retries = 0;
                while res.is_err() && retries < options.retries {
                    Delay::new((options.retry_fn)(retries)).await;
                    res = fetcher().await;
                    retries += 1;
                }
                data.set(res.map_or_else(|err| QueryData::Err(err), |data| QueryData::Ok(data)));
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

    pub fn refetch_query<'a>(self: Rc<Self>, key: &[u64]) {
        self.invalidate_queries(&vec![key]);
    }
}

pub fn use_query<'a, K, T, E, F, R>(
    cx: Scope<'a>,
    key: K,
    fetcher: F,
) -> Query<'a, T, E, impl Fn() + 'a>
where
    K: AsKey,
    F: Fn() -> R + 'static,
    R: Future<Output = Result<T, E>> + 'static,
    T: 'static,
    E: 'static,
{
    use_query_with_options(cx, key, fetcher, QueryOptions::default())
}

pub fn use_query_with_options<'a, K, T, E, F, R>(
    cx: Scope<'a>,
    key: K,
    fetcher: F,
    options: QueryOptions,
) -> Query<'a, T, E, impl Fn() + 'a>
where
    K: AsKey,
    F: Fn() -> R + 'static,
    R: Future<Output = Result<T, E>> + 'static,
    T: 'static,
    E: 'static,
{
    let key = key.as_key();

    let client = use_context::<Rc<QueryClient>>(cx).clone();
    let (data, status, fetcher) = if let Some(query) = client.find_query(&key) {
        query
    } else {
        let data: Rc<Signal<QueryData<Rc<dyn Any>, Rc<dyn Any>>>> =
            as_rc(create_rc_signal(QueryData::Loading));
        let status = as_rc(create_rc_signal(Status::Fetching));
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
    }
}
