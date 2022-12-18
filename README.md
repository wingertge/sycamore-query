# sycamore-query

Provides `react-query`/`tanstack-query` style hooks for `sycamore`.
I aim to eventually have mostly feature parity, but the project is currently
in an MVP (minimum viable product) state. This means the basic functionality
works (caching, background fetching, invalidations, mutations, refetching),
but most of the configurability and automatic refetching on window events
is missing. If you need a specific feature or configuration option, feel
free to open an issue or even a PR and I'll know to prioritise it.

# Usage

To use the library you need to provide it with a `QueryClient` as a context.
This is ideally done in your top level component or index view so your cache
is global. If you want to have separate caches for different parts of your
app it could make sense to set multiple `QueryClient`s.

```rust
use sycamore_query::{QueryClient, ClientOptions};

#[component]
pub fn App<G: Html>(cx: Scope) -> View<G> {
    provide_context(cx, QueryClient::new(ClientOptions::default()));
    
    view! { cx, }
}
```

Now you can use `use_query` and `use_mutation` from any of your components.

```rust
use sycamore_query::{QuerySignalExt, QueryData, query::{use_query, Query}};

#[component]
pub fn Hello<G: Html>(cx: Scope) -> View<G> {
    let name = create_rc_signal("World".to_string());
    let Query { data, status, refetch } = use_query(
        cx,
        ("hello", name.get()),
        move || api::hello(name.get())
    );

    match data.get_data() {
        QueryData::Loading => view! { cx, p { "Loading..." } },
        QueryData::Ok(message) => view! { cx, p { (message) } },
        QueryData::Err(err) => view! { cx, p { "An error has occured: " } p { (err) } }
    }
}
```

This will fetch the data in the background and handle all sorts of things
for you: retrying on error (up to 3 times by default), caching, updating when
a mutation invalidates the query or another query with the same key fetches
the data, etc.

# More information

I don't have the time to write an entire book on this library right now, so just
check out the `react-query` docs and the type level docs for Rust-specific
details, keeping in mind only a subset of `react-query` is currently implemented.