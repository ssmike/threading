use std::option::Option;
use std::boxed::{Box, FnBox};
use std::sync::{Arc, Mutex};
use std::iter::Iterator;
use spinlock::Spinlock;
use std::ops::Deref;
use event::Event;
use std::mem;

struct FutureState<'t, T>
    where T: 't
{
    value: Option<T>,
    callbacks: Vec<Box<'t + FnBox(Future<'t, T>) -> () + Send>>,
    ready_event: Option<Arc<Event>>
}

// all calbacks will be executed once, so
unsafe impl<'t, T: Sync> Sync for FutureState<'t, T> {}

impl<'t, T> FutureState<'t, T> {
    fn new(value: T) -> FutureState<'t, T> {
        FutureState {
            value: Option::Some(value),
            callbacks: Vec::new(),
            ready_event: None
        }
    }
}

impl<'t, T> Default for FutureState<'t, T> {
    fn default() -> FutureState<'t, T> {
        FutureState {
            value: Option::None,
            callbacks: Vec::new(),
            ready_event: None
        }
    }
}

#[derive(Default)]
pub struct Future<'t, T>
    where T: 't
{
    state: Arc<Spinlock<FutureState<'t, T>>>
}

pub struct Promise<'t, T>
    where T: 't
{
    state: Arc<Spinlock<FutureState<'t, T>>>
}

impl<'t, T> Promise<'t, T> {
    pub fn new() -> (Promise<'t, T>, Future<'t, T>) {
        let state = Arc::new(Spinlock::new(FutureState::default()));
        (Promise{state:state.clone()}, Future{state:state})
    }

    pub fn set(self: Promise<'t, T>, value: T) {
        let callbacks = {
            let mut state = self.state.lock().expect("spinlock poisoned");
            state.value = Option::Some(value);
            let mut vec = Vec::new();
            mem::swap(&mut vec, &mut state.callbacks);
            state.ready_event.as_ref().map(|ev| {ev.signal()});
            vec
        };
        let future = Future{state: self.state.clone()};
        callbacks.into_iter().for_each(|f| {
            FnBox::call_box(f, (future.clone(),));
        });
    }
}

impl<'t, T> Deref for Future<'t, T> {
    type Target = T;

    fn deref(self: &Future<'t, T>) -> &T {
        self.wait();
        let state = self.state.share();
        state.value.as_ref().expect("value was moved")
    }
}

impl<'t, T> Clone for Future<'t, T> {
    fn clone(self: &Future<'t, T>) -> Future<'t, T> {
        Future{state: self.state.clone()}
    }
}

impl<'t, T> Future<'t, T> {
    pub fn new(val: T) -> Future<'t, T> {
        Future {
            state: Arc::new(Spinlock::new(FutureState::new(val)))
        }
    }

    pub fn take(self: Future<'t, T>) -> T {
        self.wait();
        let mut state = self.state.lock();
        let val = state.as_mut().and_then(|state| state.value.take());
        val.expect("value was moved")
    }

    pub fn apply<R, Func>(self: &Future<'t, T>, f: Func) -> Future<'t, R>
        where R: 't + Send + Sync,
              Func: 't + FnOnce(Future<'t, T>) -> R + Send
    {
        let (promise, future) = Promise::new();
        self.subscribe(move |future| {
            promise.set(f(future.clone()));
        });
        future
    }

    pub fn then<R, Func>(self: &Future<'t, T>, f: Func) -> Future<'t, R>
        where Func: 't + FnOnce(Future<'t, T>) -> Future<'t, R> + Send,
              R: 't + Send + Sync
    {
        let (promise, future) = Promise::new();
        self.subscribe(move |future| {
            f(future.clone()).subscribe(move |future| {
                promise.set(future.take());
            });
        });
        future
    }

    fn subscribe<Func>(self: &Future<'t, T>, f: Func)
        where Func: 't + FnOnce(Future<'t, T>) -> () + Send
    {
        let mut guard = self.state.lock();
        if guard.is_none() || guard.as_ref().unwrap().value.is_some() {
            drop(guard);
            f(self.clone());
        } else {
            guard.as_mut().unwrap().callbacks.push(Box::new(f));
        }
    }

    fn wait(self: &Future<'t, T>) {
        let to_wait: Option<Arc<Event>> = {
            match self.state.lock() {
                None => {None},
                Some(ref mut locked) => {
                    if locked.ready_event.is_none() && locked.value.is_none() {
                        let event = Arc::new(Event::new());
                        locked.ready_event = Option::Some(event.clone());
                        Some(event)
                    } else {
                        None
                    }
                }
            }
        };
        to_wait.map(|ev| {ev.wait()});
    }
}

#[derive(Clone)]
struct Waiter<F>
    where F: FnBox() -> ()
{
    on_destroy: Option<Box<F>>
}

impl<F> Waiter<F>
    where F: FnBox() -> ()
{
    fn new(f: F) -> Waiter<F> {
        Waiter {
            on_destroy: Some(Box::new(f))
        }
    }
}

impl<F> Drop for Waiter<F>
    where F: FnBox() -> ()
{
    fn drop(self: &mut Waiter<F>) {
        FnBox::call_box(self.on_destroy.take().unwrap(), ())
    }
}

pub fn wait_all<'t, T, I>(i: I) -> Future<'t, ()>
    where I: Iterator<Item = Future<'t, T>>,
          T: 't
{
    let (promise, future) = Promise::new();
    let waiter = Arc::new(Waiter::new(
        move || {
            promise.set(());
        }));
    i.for_each(|f| {
        let waiter = waiter.clone();
        f.subscribe(move |_| drop(waiter));
    });
    future
}

pub fn wait_any<'t, T, I>(i: I) -> Future<'t, ()>
    where I: Iterator<Item = Future<'t, T>>,
          T: 't
{
    let (promise, future) = Promise::new();
    let promise = Arc::new(Mutex::new(Some(promise)));
    i.for_each(|f| {
        let promise = promise.clone();
        f.subscribe(move |_| {
            promise
                .lock().unwrap()
                .take()
                .map(|promise| promise.set(()));
        });
    });
    future
}


