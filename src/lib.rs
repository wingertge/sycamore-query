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
pub mod mutation;
pub mod query;

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
pub trait AsKey {
    fn as_key(&self) -> Vec<u64>;
}

impl AsKey for str {
    fn as_key(&self) -> Vec<u64> {
        let mut hash = FnvHasher::default();
        self.hash(&mut hash);
        vec![hash.finish()]
    }
}

impl AsKey for &str {
    fn as_key(&self) -> Vec<u64> {
        let mut hash = FnvHasher::default();
        self.hash(&mut hash);
        vec![hash.finish()]
    }
}

impl AsKey for String {
    fn as_key(&self) -> Vec<u64> {
        self.as_str().as_key()
    }
}

macro_rules! impl_as_key_tuple {
    ($($ty:ident),*) => {
        impl<$($ty: Hash),*> AsKey for ($($ty),*) {
            fn as_key(&self) -> Vec<u64> {
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
    Loading,
    Ok(T),
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
    Fetching,
    Success,
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
            use $crate::AsKey;
            let mut res = ::std::vec::Vec::with_capacity(keys!(@count $($key),*));
            $(
                res.push($key.as_key());
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
