use std::option::Option;
use std::boxed::{Box, FnBox};
use std::sync::{Arc, Mutex};
use std::iter::Iterator;
use spinlock::{Spinlock, SpinlockGuard};
use event::Event;
use std::mem;

struct FutureState<'t, T>
    where T: 't
{
    value: Option<T>,
    callbacks: Vec<Box<'t + FnBox(&T) -> () + Send>>,
    ready_event: Option<Box<Event>>
}

impl<'t, T> FutureState<'t, T> {
    fn new(value: T) -> FutureState<'t, T> {
        FutureState {
            value: Option::Some(value),
            callbacks: Vec::new(),
            ready_event: None
        }
    }

    unsafe fn get(self: &FutureState<'t, T>) -> &'t T {
        mem::transmute(self.value.as_ref().unwrap())
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

impl<'t, T> Clone for Future<'t, T> {
    fn clone(self: &Future<'t, T>) -> Future<'t, T> {
        Future {
            state: self.state.clone()
        }
    }
}

#[derive(Default)]
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
            let mut guard = self.state.lock();
            let mut state = guard;
            state.value = Option::Some(value);
            let mut vec = Vec::new();
            mem::swap(&mut vec, &mut state.callbacks);
            vec
        };
        let state = unsafe { self.state.get() };
        state.ready_event.as_ref().map(|ev| {ev.signal()});
        let value = state.value.as_ref().unwrap();
        callbacks.into_iter().for_each(|f| {
            FnBox::call_box(f, (value,));
        });
    }
}

impl<'t, T> Future<'t, T> {
    pub fn new(val: T) -> Future<'t, T> {
        Future {
            state: Arc::new(Spinlock::new(FutureState::new(val)))
        }
    }

    unsafe fn take(self: &Future<'t, T>) -> Option<T> {
        self.state.lock().value.take()
    }

    pub fn map<R, Func>(self: &Future<'t, T>, f: Func) -> Future<'t, R>
        where R: 't,
              Func: 't + FnOnce(&T) -> R + Send
    {
        let (promise, future) = Promise::new();
        self.subscribe(move |x| {promise.set(f(x));});
        future
    }

    pub fn then<R, Func>(self: &Future<'t, T>, f: Func) -> Future<'t, R>
        where Func: 't + FnOnce(&T) -> Future<'t, R> + Send,
              R: 't
    {
        let (promise, future) = Promise::new();
        self.subscribe(move |x| {
            let sub = f(x);
            let borrow = sub.clone();
            sub.subscribe(move |_| {
                // here we take value out of state, which executes this callback
                // so _ is dangling :)
                promise.set(unsafe {borrow.take().unwrap()});
            });
        });
        future
    }


    fn subscribe<Func>(self: &Future<'t, T>, f: Func)
        where Func: 't + FnOnce(&T) -> () + Send
    {
        let mut state = self.state.lock();
        match state.value {
            None => {
                state.callbacks.push(Box::new(f));
            },
            Some(_) => {
                mem::drop(state);
                let state = unsafe { self.state.get() };
                f(state.value.as_ref().unwrap());
            }
        }
    }

    pub fn wait(self: &Future<'t, T>) -> &T {
        {
            let mut locked = self.state.lock();
            if locked.ready_event.is_none() && locked.value.is_none() {
                locked.ready_event = Option::Some(Box::new(Event::new()));
            }
        }
        let state = unsafe { self.state.get() };
        state.ready_event.as_ref().map(|ev| {ev.wait()});
        unsafe{state.get()}
    }
}

#[derive(Clone)]
struct Waiter<F>
    where F: Send + FnBox() -> ()
{
    on_destroy: Option<Box<F>>
}

impl<F> Waiter<F>
    where F: Send + FnBox() -> ()
{
    fn new(f: F) -> Waiter<F> {
        Waiter {
            on_destroy: Some(Box::new(f))
        }
    }
}

impl<F> Drop for Waiter<F>
    where F: Send + FnBox() -> ()
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


