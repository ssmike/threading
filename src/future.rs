use std::sync::{Arc, Mutex};
use spinlock::Spinlock;
use event::Event;
use std::mem;

use future::FutureValue::*;

enum FutureValue<T> {
    ValEmpty,
    ValSet(T),
    ValMoved,
}

impl<T> FutureValue<T> {
    fn is_empty(&self) -> bool {
        match *self {
            ValEmpty => true,
            _ => false
        }
    }

    fn take(&mut self) -> T {
        let mut new = ValMoved;
        mem::swap(&mut new, self);
        match new {
            ValSet(x) => x,
            _ => {panic!("value has been moved");}
        }
    }

    fn read(&self) -> &T {
        match *self {
            ValSet(ref x) => x,
            _ => {panic!("value has been moved");}
        }
    }

    fn put(&mut self, val: T) {
        match *self {
            ValSet(_) => {panic!("double set on same future state");},
            ValMoved => {panic!("value already moved");},
            _ => {}
        }
        *self = ValSet(val);
    }
}

struct FutureState<'t, T>
    where T: 't
{
    value: FutureValue<T>,
    callbacks: Vec<Box<dyn 't + FnOnce(&StateHolder<'t, T>) -> () + Send>>,
    ready_event: Option<Arc<Event>>
}

// all calbacks will be executed once, so
unsafe impl<'t, T: Sync> Sync for FutureState<'t, T> {}

impl<'t, T> FutureState<'t, T> {
    fn new(value: T) -> FutureState<'t, T> {
        FutureState {
            value: ValSet(value),
            callbacks: Vec::new(),
            ready_event: None
        }
    }
}

impl<'t, T> Default for FutureState<'t, T> {
    fn default() -> FutureState<'t, T> {
        FutureState {
            value: ValEmpty,
            callbacks: Vec::new(),
            ready_event: None
        }
    }
}

#[derive(Default)]
struct StateHolder<'t, T>
    where T: 't
{
    state: Arc<Spinlock<FutureState<'t, T>>>
}

impl<'t, T> Clone for StateHolder<'t, T> {
    fn clone(&self) -> Self {
        StateHolder{state: self.state.clone()}
    }
}

impl<'t, T> StateHolder<'t, T> {
    fn preset(val: T) -> Self {
        StateHolder {
            state: Arc::new(Spinlock::new(FutureState::new(val)))
        }
    }

    fn new() -> Self {
        StateHolder {
            state: Arc::new(Spinlock::new(FutureState::default()))
        }
    }

    fn set(&self, value: T) {
        let callbacks = {
            let mut state = self.state.lock().expect("spinlock poisoned");
            state.value.put(value);
            let mut vec = Vec::new();
            mem::swap(&mut vec, &mut state.callbacks);
            state.ready_event.as_ref().map(|ev| {ev.signal()});
            vec
        };
        callbacks.into_iter().for_each(|f| {
            Box::call_once(f, (self,));
        });
    }

    fn take(&self) -> T {
        self.wait();
        let mut state = self.state.lock();
        state.as_mut().expect("value already shared")
            .value.take()
    }

    fn wait(&self) {
        let to_wait: Option<Arc<Event>> = {
            match self.state.lock() {
                None => {None},
                Some(ref mut locked) => {
                    if locked.ready_event.is_none() && locked.value.is_empty() {
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

    fn subscribe<Func>(&self, f: Func)
        where Func: 't + FnOnce(&StateHolder<'t, T>) -> () + Send
    {
        let boxed = Box::new(f);
        let mut guard = self.state.lock();
        if guard.is_none() || !guard.as_ref().unwrap().value.is_empty() {
            drop(guard);
            Box::call_once(boxed, (self,));
        } else {
            guard.as_mut().unwrap().callbacks.push(boxed);
        }
    }
}

impl<'t, T> StateHolder<'t, T>
    where T: Sync
{
    fn get(&self) -> &T {
        self.wait();
        let state = self.state.share();
        state.value.read()
    }
}

pub struct Promise<'t, T>
    where T: 't
{
    holder: StateHolder<'t, T>
}

impl<'t, T> Promise<'t, T> {
    pub fn new() -> (Promise<'t, T>, Future<'t, T>) {
        let holder = StateHolder::new();
        (Promise{holder:holder.clone()}, Future{holder:holder})
    }

    pub fn set(self: Promise<'t, T>, value: T) {
        self.holder.set(value)
    }
}

pub struct Future<'t, T>
    where T: 't
{
    holder: StateHolder<'t, T>
}

impl<'t, T> Future<'t, T> {
    pub fn new(val: T) -> Future<'t, T> {
        Future {
            holder: StateHolder::preset(val)
        }
    }

    pub fn take(self) -> T {
        self.holder.take()
    }

    pub fn apply<R, Func>(self, f: Func) -> Future<'t, R>
        where R: 't + Send,
              Func: 't + FnOnce(T) -> R + Send
    {
        let (promise, future) = Promise::new();
        self.holder.subscribe(move |holder| {
            promise.set(f(holder.take()));
        });
        future
    }

    pub fn then<R, Func>(self, f: Func) -> Future<'t, R>
        where Func: 't + FnOnce(T) -> Future<'t, R> + Send,
              R: 't + Send
    {
        let (promise, future) = Promise::new();
        self.holder.subscribe(move |holder| {
            f(holder.take()).holder.subscribe(move |holder| {
                promise.set(holder.take());
            });
        });
        future
    }

    pub fn wait(&self) {
        self.holder.wait()
    }
}

impl<'t, T: Sync> Future<'t, T> {
    pub fn share(self) -> SharedFuture<'t, T> {
        SharedFuture {
            holder: self.holder
        }
    }
}

pub struct SharedFuture<'t, T>
    where T: 't + Sync
{
    holder: StateHolder<'t, T>
}

impl<'t, T: Sync> Clone for SharedFuture<'t, T> {
    fn clone(&self) -> Self {
        SharedFuture{holder: self.holder.clone()}
    }
}

impl<'t, T: 't + Sync> SharedFuture<'t, T> {
    pub fn get(&self) -> &T {
        self.holder.get()
    }

    pub fn apply<R, Func>(&self, f: Func) -> Future<'t, R>
        where R: 't + Send,
              Func: 't + FnOnce(&T) -> R + Send
    {
        let (promise, future) = Promise::new();
        self.holder.subscribe(move |holder| {
            promise.set(f(holder.get()));
        });
        future
    }

    pub fn then<R, Func>(&self, f: Func) -> Future<'t, R>
        where Func: 't + FnOnce(&T) -> Future<'t, R> + Send,
              R: 't + Send
    {
        let (promise, future) = Promise::new();
        self.holder.subscribe(move |holder| {
            f(holder.get()).holder.subscribe(move |holder| {
                promise.set(holder.take());
            });
        });
        future
    }

    pub fn wait(&self) {
        self.holder.wait()
    }
}

#[derive(Clone)]
struct Waiter<F>
    where F: FnOnce() -> ()
{
    on_destroy: Option<Box<F>>
}

impl<F> Waiter<F>
    where F: FnOnce() -> ()
{
    fn new(f: F) -> Waiter<F> {
        Waiter {
            on_destroy: Some(Box::new(f))
        }
    }
}

impl<F> Drop for Waiter<F>
    where F: FnOnce() -> ()
{
    fn drop(self: &mut Waiter<F>) {
        Box::call_once(self.on_destroy.take().unwrap(), ())
    }
}

pub fn wait_all<'i, 't, T, I>(i: I) -> Future<'t, ()>
    where I: Iterator<Item = &'i Future<'t, T>>,
          't : 'i,
          T: 't
{
    let (promise, future) = Promise::new();
    let waiter = Arc::new(Waiter::new(
        move || {
            promise.set(());
        }));
    i.for_each(|f| {
        let waiter = waiter.clone();
        f.holder.subscribe(move |_| drop(waiter));
    });
    future
}

pub fn wait_any<'i, 't, T, I>(i: I) -> Future<'t, ()>
    where I: Iterator<Item = &'i Future<'t, T>>,
          't : 'i,
          T: 't
{
    let (promise, future) = Promise::new();
    let promise = Arc::new(Mutex::new(Some(promise)));
    i.for_each(|f| {
        let promise = promise.clone();
        f.holder.subscribe(move |_| {
            promise
                .lock().unwrap()
                .take()
                .map(|promise| promise.set(()));
        });
    });
    future
}
