use std::{future::Future, rc::Rc};

use sycamore::{
    futures::spawn_local_scoped,
    reactive::{create_ref, create_signal, use_context, ReadSignal, Scope, Signal},
};

use crate::{client::QueryOptions, QueryClient, QueryData, Status};

/// The struct representing a mutation
///
/// # Example
///
/// ```
/// # use sycamore::prelude::*;
/// # use sycamore_query::{*, mutation::{Mutation, use_mutation}};
/// # #[component]
/// # pub fn App<G: Html>(cx: Scope) -> View<G> {
/// #   provide_context(cx, QueryClient::new(ClientOptions::default()));
/// let Mutation { data, status, mutate } = use_mutation(
///     cx,
///     |name: String| async { Result::<_, ()>::Ok(name) },
///     |client, data| client.set_query_data("name", data)
/// );
///
/// mutate("World".to_string());
/// # view! { cx, }
/// # }
/// ```
pub struct Mutation<'a, T, E, Args> {
    /// The data returned by the mutation, if any
    pub data: &'a ReadSignal<QueryData<Rc<T>, Rc<E>>>,
    /// The status of the mutation
    pub status: &'a ReadSignal<Status>,
    /// The mutation function. This takes in the arguments for the mutator
    /// function and tries to execute the mutation.
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
        Success: Fn(Rc<QueryClient>, Rc<T>),
        Args: 'a,
    {
        let ctx = cx.clone();
        status.set(Status::Fetching);
        spawn_local_scoped(cx, async move {
            let res = mutator(args).await;
            data.set(res.map_or_else(
                |err| QueryData::Err(Rc::new(err)),
                |data| QueryData::Ok(Rc::new(data)),
            ));
            if let QueryData::Ok(ok) = data.get().as_ref() {
                let client = use_context::<Rc<QueryClient>>(ctx);
                on_success(client.clone(), ok.clone());
            }
            status.set(Status::Success);
        });
    }
}

/// Use a mutation that updates data on the server.
///
/// # Parameters
///
/// * `cx` - The scope for the component the mutation is in.
/// * `mutator` - The function that actually executes the mutation on the server.
/// This can take in any type of arguments.
/// * `on_success` - Function to execute when the mutation is successful. Used to
/// invalidate queries or update queries with data returned by the mutation.
///
/// # Returns
///
/// A [`Mutation`] struct.
///
/// # Example
///
/// ```
/// # use sycamore::prelude::*;
/// # use sycamore_query::{*, mutation::{Mutation, use_mutation}};
/// # #[component]
/// # pub fn App<G: Html>(cx: Scope) -> View<G> {
/// #   provide_context(cx, QueryClient::new(ClientOptions::default()));
/// let Mutation { data, status, mutate } = use_mutation(
///     cx,
///     |name: String| async { Result::<_, ()>::Ok(name) },
///     |client, data| client.set_query_data("name", data)
/// );
/// # view! { cx, }
/// # }
pub fn use_mutation<'a, Args, T, E, F, R, Success>(
    cx: Scope<'a>,
    mutator: F,
    on_success: Success,
) -> Mutation<'a, T, E, Args>
where
    F: Fn(Args) -> R + 'a,
    R: Future<Output = Result<T, E>>,
    Success: Fn(Rc<QueryClient>, Rc<T>) + 'a,
{
    use_mutation_with_options(cx, mutator, on_success, QueryOptions::default())
}

/// Use a mutation with additional query options. For more information, see
/// [`use_mutation`] and [`QueryOptions`]
pub fn use_mutation_with_options<'a, Args, T, E, F, R, Success>(
    cx: Scope<'a>,
    mutator: F,
    on_success: Success,
    _options: QueryOptions,
) -> Mutation<'a, T, E, Args>
where
    F: Fn(Args) -> R + 'a,
    R: Future<Output = Result<T, E>>,
    Success: Fn(Rc<QueryClient>, Rc<T>) + 'a,
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
