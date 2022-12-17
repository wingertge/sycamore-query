use std::{future::Future, rc::Rc};

use sycamore::{
    futures::spawn_local_scoped,
    reactive::{create_ref, create_signal, use_context, ReadSignal, Scope, Signal},
};

use crate::{client::QueryOptions, QueryClient, QueryData, Status};

pub struct Mutation<'a, T, E, Args> {
    pub data: &'a ReadSignal<QueryData<Rc<T>, Rc<E>>>,
    pub status: &'a ReadSignal<Status>,
    pub mutate: &'a dyn Fn(Args),
}

impl QueryClient {
    pub fn run_mutation<'a, T, E, Mutate, R, Args, Success>(
        &self,
        cx: Scope<'a>,
        data: &'a Signal<QueryData<Rc<T>, Rc<E>>>,
        status: &'a Signal<Status>,
        mutator: &'a Mutate,
        args: Args,
        on_success: &'a Success,
    ) where
        Mutate: Fn(Args) -> R,
        R: Future<Output = Result<T, E>>,
        Success: Fn(Rc<QueryClient>),
        Args: 'a,
    {
        let ctx = cx.clone();
        status.set(Status::Fetching);
        spawn_local_scoped(cx, async move {
            let res = mutator(args).await;
            if res.is_ok() {
                let client = use_context::<Rc<QueryClient>>(ctx);
                on_success(client.clone());
            }
            data.set(res.map_or_else(
                |err| QueryData::Err(Rc::new(err)),
                |data| QueryData::Ok(Rc::new(data)),
            ));
            status.set(Status::Success);
        });
    }
}

pub fn use_mutation<'a, Args, T, E, F, R, Success>(
    cx: Scope<'a>,
    mutator: F,
    on_success: Success,
) -> Mutation<'a, T, E, Args>
where
    F: Fn(Args) -> R + 'a,
    R: Future<Output = Result<T, E>>,
    Success: Fn(Rc<QueryClient>) + 'a,
{
    use_mutation_with_options(cx, mutator, on_success, QueryOptions::default())
}

pub fn use_mutation_with_options<'a, Args, T, E, F, R, Success>(
    cx: Scope<'a>,
    mutator: F,
    on_success: Success,
    _options: QueryOptions,
) -> Mutation<'a, T, E, Args>
where
    F: Fn(Args) -> R + 'a,
    R: Future<Output = Result<T, E>>,
    Success: Fn(Rc<QueryClient>) + 'a,
{
    let client = use_context::<Rc<QueryClient>>(cx).clone();
    let data: &Signal<QueryData<Rc<T>, Rc<E>>> = create_signal(cx, QueryData::Loading);
    let status = create_signal(cx, Status::Fetching);
    let mutator = create_ref(cx, mutator);
    let on_success = create_ref(cx, on_success);

    let mutate = create_ref(cx, move |args: Args| {
        client.run_mutation(cx, data, status, mutator, args, on_success)
    });

    Mutation {
        data,
        mutate,
        status,
    }
}
