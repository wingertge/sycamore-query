use std::{any::Any, future::Future, pin::Pin, rc::Rc};

use sycamore::reactive::{RcSignal, ReadSignal, Signal};

mod cache;
mod client;
pub mod mutation;
pub mod query;
pub mod util;

pub use client::QueryClient;

pub(crate) type Fetcher =
    Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<Rc<dyn Any>, Rc<dyn Any>>>>>>;
pub(crate) type DataSignal = Signal<QueryData<Rc<dyn Any>, Rc<dyn Any>>>;
pub(crate) type DynQueryData = QueryData<Rc<dyn Any>, Rc<dyn Any>>;

#[derive(Clone)]
pub enum QueryData<T, E> {
    Loading,
    Ok(T),
    Err(E),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    Fetching,
    Success,
    Idle,
}

#[macro_export]
macro_rules! keys {
    (@to_unit $($_:tt)*) => (());
    (@count $($tail:expr),*) => (
        <[()]>::len(&[$(keys!(@to_unit $tail)),*])
      );

    [$($key: expr),* $(,)?] => {
        {
            let mut res = ::std::vec::Vec::with_capacity(keys!(@count $($key),*));
            $(
                res.push($crate::util::hash_key($key));
            )*
            res
        }
    };
}

pub trait QuerySignalExt<T, E> {
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
