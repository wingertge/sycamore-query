//! Provides `react-query`/`tanstack-query` style hooks for `sycamore`.
//! I aim to eventually have mostly feature parity, but the project is currently
//! in an MVP (minimum viable product) state. This means the basic functionality
//! works (caching, background fetching, invalidations, mutations, refetching),
//! but most of the configurability and automatic refetching on window events
//! is missing. If you need a specific feature or configuration option, feel
//! free to open an issue or even a PR and I'll know to prioritise it.
//!
//! # Usage
//!
//! To use the library you need to provide it with a [`QueryClient`] as a context.
//! This is ideally done in your top level component or index view so your cache
//! is global. If you want to have separate caches for different parts of your
//! app it could make sense to set multiple [`QueryClient`]s.
//!
//! ```
//! # use sycamore::prelude::*;
//! use sycamore_query::{QueryClient, ClientOptions};
//!
//! #[component]
//! pub fn App<G: Html>(cx: Scope) -> View<G> {
//!     provide_context(cx, QueryClient::new(ClientOptions::default()));
//!     
//!     view! { cx, }
//! }
//! ```
//!
//! Now you can use [`use_query`](crate::query::use_query) and
//! [`use_mutation`](crate::mutation::use_mutation) from any of your components.
//!
//! ```
//! # use sycamore::prelude::*;
//! # use sycamore_query::{QueryClient, ClientOptions};
//! use sycamore_query::prelude::*;
//!
//! # mod api {
//! #   use std::rc::Rc;
//! #   pub async fn hello(name: Rc<String>) -> Result<String, String> {
//! #       Ok(name.to_string())
//! #   }
//! # }
//!
//! #[component]
//! pub fn Hello<G: Html>(cx: Scope) -> View<G> {
//! #   provide_context(cx, QueryClient::new(ClientOptions::default()));
//!     let name = create_rc_signal("World".to_string());
//!     let Query { data, status, refetch } = use_query(
//!         cx,
//!         ("hello", name.get()),
//!         move || api::hello(name.get())
//!     );
//!
//!     match data.get_data() {
//!         QueryData::Loading => view! { cx, p { "Loading..." } },
//!         QueryData::Ok(message) => view! { cx, p { (message) } },
//!         QueryData::Err(err) => view! { cx, p { "An error has occured: " } p { (err) } }
//!     }
//! }
//! ```
//!
//! This will fetch the data in the background and handle all sorts of things
//! for you: retrying on error (up to 3 times by default), caching, updating when
//! a mutation invalidates the query or another query with the same key fetches
//! the data, etc.
//!
//! # More information
//!
//! I don't have the time to write an entire book on this library right now, so just
//! check out the `react-query` docs and the type level docs for Rust-specific
//! details, keeping in mind only a subset of `react-query` is currently implemented.

#![warn(missing_docs)]

use std::{
    any::Any,
    future::Future,
    hash::{Hash, Hasher},
    pin::Pin,
    rc::Rc,
};

use fnv::FnvHasher;
use sycamore::reactive::{RcSignal, ReadSignal, Signal};

mod cache;
mod client;
/// Mutation related functions and types
pub mod mutation;
/// Query related functions and types
pub mod query;

/// The sycamore-query prelude.
///
/// In most cases, it is idiomatic to use a glob import (aka wildcard import) at the beginning of
/// your Rust source file.
///
/// ```rust
/// use sycamore_query::prelude::*;
/// ```
pub mod prelude {
    pub use crate::mutation::{use_mutation, Mutation};
    pub use crate::query::{use_query, Query};
    pub use crate::{AsKeySignal, AsRcKeySignal, QueryData, QuerySignalExt, Status};
}

pub use client::*;

pub(crate) type Fetcher =
    Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<Rc<dyn Any>, Rc<dyn Any>>>>>>;
pub(crate) type DataSignal = Signal<QueryData<Rc<dyn Any>, Rc<dyn Any>>>;

/// Trait for anything that can be turned into a key
/// The reason this exists is to allow for prefix invalidation, so lists or
/// tuples should return one hash per element.
/// It's automatically implemented for `String`, `str` and any tuple of size
/// 2 - 12 where each element implements `Hash`.
/// If your keys aren't covered by the default implementation for some reason,
/// you can implement this manually.
///
/// # Example
/// ```
/// # use sycamore_query::AsKey;
/// # use fnv::FnvHasher;
/// # use std::hash::{Hasher, Hash};
/// struct MyType {
///     item1: String,
///     item2: String,
/// }
///
/// impl AsKey for MyType {
///     fn as_key(&self) -> Vec<u64> {
///         let mut hash = FnvHasher::default();
///         self.item1.hash(&mut hash);
///         let hash1 = hash.finish();
///         hash = FnvHasher::default();
///         self.item2.hash(&mut hash);
///         let hash2 = hash.finish();
///         vec![hash1, hash2]
///     }
/// }
/// ```
/// }
pub trait AsKeys {
    /// Internal function to convert the type to a key for use in the query cache
    /// and notifier list.
    fn as_keys(&self) -> Vec<u64>;
}

impl AsKeys for str {
    fn as_keys(&self) -> Vec<u64> {
        let mut hash = FnvHasher::default();
        self.hash(&mut hash);
        vec![hash.finish()]
    }
}

impl AsKeys for &str {
    fn as_keys(&self) -> Vec<u64> {
        let mut hash = FnvHasher::default();
        self.hash(&mut hash);
        vec![hash.finish()]
    }
}

impl AsKeys for String {
    fn as_keys(&self) -> Vec<u64> {
        self.as_str().as_keys()
    }
}

macro_rules! impl_as_key_tuple {
    ($($ty:ident),*) => {
        impl<$($ty: Hash),*> AsKeys for ($($ty),*) {
            fn as_keys(&self) -> Vec<u64> {
                #[allow(non_snake_case)]
                let ($($ty),*) = self;
                vec![$(
                    {
                        let mut hash = FnvHasher::default();
                        $ty.hash(&mut hash);
                        hash.finish()
                    }
                ),*]
            }
        }
    };
}

// Implement for tuples up to 12 long
impl_as_key_tuple!(T1, T2);
impl_as_key_tuple!(T1, T2, T3);
impl_as_key_tuple!(T1, T2, T3, T4);
impl_as_key_tuple!(T1, T2, T3, T4, T5);
impl_as_key_tuple!(T1, T2, T3, T4, T5, T6);
impl_as_key_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_as_key_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_as_key_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_as_key_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_as_key_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_as_key_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

/// The data type of a query.
///
/// # States
///
/// * `Loading` - No query data is available yet
/// * `Ok` - Query data was successfully fetched and is available. Note this
/// might be stale data, check `QueryStatus` if you need to verify whether the
/// query is currently fetching fresh data.
/// * `Err` - Query data still wasn't able to be fetched after the retry strategy
/// was exhausted. This contains the backing error.
///
#[derive(Clone)]
pub enum QueryData<T, E> {
    /// No query data is available yet
    Loading,
    /// Query data was successfully fetched and is available. Note this
    /// might be stale data, check `QueryStatus` if you need to verify whether the
    /// query is currently fetching fresh data.
    Ok(T),
    /// Query data still wasn't able to be fetched after the retry strategy
    /// was exhausted. This contains the backing error.
    Err(E),
}

/// The status of a query.
///
/// # States
///
/// * `Fetching` - Query data is currently being fetched. This might be because
/// no data is available ([`QueryData::Loading`]) or because the data is
/// considered stale.
/// * `Success` - Query data is available and fresh.
/// * `Idle` - Query is disabled from running.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    /// Query data is currently being fetched. This might be because
    /// no data is available ([`QueryData::Loading`]) or because the data is
    Fetching,
    /// Query data is available and fresh.
    Success,
    /// Query is disabled from running.
    Idle,
}

/// A convenience macro for passing a set of keys.
/// Keys don't have the same type, so regular `Vec`s don't work.
///
/// # Example Usage
///
/// ```
/// # use sycamore_query::keys;
/// # use std::rc::Rc;
/// # let client = sycamore_query::QueryClient::new(Default::default());
/// client.invalidate_queries(keys![("hello", "World"), "test", ("user", 3)]);
/// ```
///
#[macro_export]
macro_rules! keys {
    (@to_unit $($_:tt)*) => (());
    (@count $($tail:expr),*) => (
        <[()]>::len(&[$(keys!(@to_unit $tail)),*])
      );

    [$($key: expr),* $(,)?] => {
        {
            use $crate::AsKeys;
            let mut res = ::std::vec::Vec::with_capacity(keys!(@count $($key),*));
            $(
                res.push($key.as_keys());
            )*
            res
        }
    };
}

/// Utility functions for dealing with QueryData in signals.
pub trait QuerySignalExt<T, E> {
    /// Unwraps the outer `Rc` of the signal to provide you with an easier to
    /// match on, unwrapped [`QueryData`].
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use sycamore_query::{QueryData, QuerySignalExt};
    /// # use sycamore::reactive::create_rc_signal;
    /// # use std::rc::Rc;
    /// # let signal = create_rc_signal::<QueryData<_, Rc<String>>>(QueryData::Ok(Rc::new("Hello".to_string())));
    ///
    /// match signal.get_data() {
    ///     QueryData::Ok(message) => println!("{message}"),
    ///     QueryData::Err(err) => eprintln!("{err}"),
    ///     QueryData::Loading => println!("No data yet")
    /// }
    ///
    /// ```
    fn get_data(&self) -> QueryData<Rc<T>, Rc<E>>;
}

impl<T, E> QuerySignalExt<T, E> for ReadSignal<QueryData<Rc<T>, Rc<E>>> {
    fn get_data(&self) -> QueryData<Rc<T>, Rc<E>> {
        match self.get().as_ref() {
            QueryData::Loading => QueryData::Loading,
            QueryData::Ok(data) => QueryData::Ok(data.clone()),
            QueryData::Err(err) => QueryData::Err(err.clone()),
        }
    }
}

struct MyRcSignal<T>(Rc<Signal<T>>);

pub(crate) fn as_rc<T>(signal: RcSignal<T>) -> Rc<Signal<T>> {
    // UNSAFE: This is actually kind of unsafe, but as long as the signature of
    // `RcSignal` doesn't change and the compiler doesn't throw a curveball it
    // should work. This should be replaced with a builtin way to do it.
    let signal: MyRcSignal<T> = unsafe { std::mem::transmute(signal) };
    signal.0
}

/// Internal type for tracking key changes. Only exposed because it's used in a public trait
pub struct KeySignal<'cx, T: Hash>(&'cx ReadSignal<T>);
/// Internal type for tracking key changes. Only exposed because it's used in a public trait
pub struct RcKeySignal<T: Hash>(RcSignal<T>);

/// Extension to allow for tracking key changes. If I can get some changes into sycamore this should
/// become redundant
///
/// # Usage
///
/// ```
/// # use sycamore::prelude::*;
/// use sycamore_query::prelude::*;
/// # #[component]
/// # pub fn App<G: Html>(cx: Scope) -> View<G> {
/// # async fn hello(s: String) -> Result<String, String> {
/// #   Ok(s.to_string())
/// # }
///  let signal = create_signal(cx, "Test");
/// // Updates every time signal changes
/// use_query(cx, ("hello", signal.key()), move || hello(signal.get().to_string());
/// # }
/// ```
pub trait AsKeySignal<T: Hash> {
    /// Creates a reference to the signal that tracks when it's hashed (sycamore uses
    /// [`get_untracked`](sycamore::reactive::ReadSignal) in the [`Hash`](std::hash::Hash)
    /// implementation for signals).
    fn key<'cx>(&'cx self) -> KeySignal<'cx, T>;
}

/// Extension to allow for tracking key changes. If I can get some changes into sycamore this should
/// become redundant
///
/// # Usage
///
/// ```
/// # use sycamore::prelude::*;
/// use sycamore_query::prelude::*;
/// # #[component]
/// # pub fn App<G: Html>(cx: Scope) -> View<G> {
/// # async fn hello(s: String) -> Result<String, String> {
/// #   Ok(s.to_string())
/// # }
/// let signal = create_rc_signal("Test");
/// // Updates every time signal changes
/// use_query(cx, ("hello", signal.clone().rc_key()), move || hello(signal.get().to_string());
/// # }
/// ```
pub trait AsRcKeySignal<T: Hash> {
    /// Creates a copy of the signal that tracks when it's hashed (sycamore uses `get_untracked`
    /// in the `Hash` implementation for signals).
    fn rc_key(self) -> RcKeySignal<T>;
}

impl<T: Hash> AsKeySignal<T> for ReadSignal<T> {
    fn key<'cx>(&'cx self) -> KeySignal<'cx, T> {
        KeySignal(self)
    }
}

impl<T: Hash> AsRcKeySignal<T> for RcSignal<T> {
    fn rc_key(self) -> RcKeySignal<T> {
        RcKeySignal(self)
    }
}

impl<'cx, T: Hash> Hash for KeySignal<'cx, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.track();
        self.0.hash(state);
    }
}

impl<T: Hash> Hash for RcKeySignal<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.track();
        self.0.hash(state);
    }
}
