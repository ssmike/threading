use std::option::Option;
use std::boxed::{Box, FnBox};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, Condvar};
use std::mem;
use std::cell::UnsafeCell;
use std::ops::{DerefMut, Deref};
use std::thread;
use std::marker::PhantomData;

#[derive(Default)]
struct Spinlock<T> {
    cnt: AtomicI64,
    data: UnsafeCell<T>
}

struct SpinlockGuard<'t, T: 't> {
    parent: &'t Spinlock<T>,
}

impl<'t, T: 't> Drop for SpinlockGuard<'t, T> {
    fn drop(self: &mut SpinlockGuard<'t, T>) {
        self.parent.cnt.fetch_sub(1, Ordering::SeqCst);
    }
}

impl<'t, T: 't> Deref for SpinlockGuard<'t, T> {
    type Target = T;

    fn deref(self: &SpinlockGuard<'t, T>) -> &T {
        unsafe {mem::transmute(self.parent.data.get())}
    }
}

impl<'t, T: 't> DerefMut for SpinlockGuard<'t, T> {
    fn deref_mut(self: &mut SpinlockGuard<'t, T>) -> &mut T {
        unsafe {mem::transmute(self.parent.data.get())}
    }
}

impl<T> Spinlock<T> {
    fn new(value: T) -> Spinlock<T> {
        Spinlock {
            cnt: AtomicI64::new(0),
            data: UnsafeCell::from(value)
        }
    }

    fn lock<'t>(self: &'t Spinlock<T>) -> (SpinlockGuard<'t, T>) {
        while self.cnt.fetch_add(1, Ordering::SeqCst) != 0 {
            self.cnt.fetch_sub(1, Ordering::SeqCst);
        }
        SpinlockGuard{parent: self}
    }

    unsafe fn get<'t>(self: &'t Spinlock<T>) -> &'t mut T {
        mem::transmute(self.data.get())
    }
}

struct Event {
    var: Condvar,
    set: Mutex<bool>
}

impl Event {
    fn new() -> Event {
        Event {
            set: Mutex::new(false),
            var: Condvar::new()
        }
    }

    fn reset(self: &Event) {
        *(self.set.lock().unwrap()) = false;
    }

    fn wait(self: &Event) {
        loop {
            let lock = self.set.lock().unwrap();
            if *lock {
                break;
            } else {
                self.var.wait(lock);
            }
        }
    }

    fn signal(self: &Event) {
        let mut lock = self.set.lock().unwrap();
        *lock = true;
        self.var.notify_all();
    }
}

struct FutureState<'t, T>
    where T: 't
{
    value: Option<T>,
    callbacks: Vec<Box<'t + FnBox(&T) -> ()>>,
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

#[derive(Clone, Default)]
pub struct Future<'t, T>
    where T: 't
{
    state: Arc<Spinlock<FutureState<'t, T>>>
}

unsafe impl<'t, T> Send for Future<'t, T> {}

#[derive(Default)]
pub struct Promise<'t, T>
    where T: 't
{
    state: Arc<Spinlock<FutureState<'t, T>>>
}

unsafe impl<'t, T> Send for Promise<'t, T> {}

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
            let mut vec = Vec::<Box<FnBox(&T) -> ()>>::new();
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

    pub fn then<R: 't, Func: 't + FnOnce(&T) -> R>(self: &Future<'t, T>, f: Func) -> Future<'t, R> {
        let (promise, future) = Promise::new();
        self.subscribe(move |x| {promise.set(f(x));});
        future
    }

    fn subscribe<Func: 't + FnOnce(&T) -> ()>(self: &Future<'t, T>, f: Func) {
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

    pub fn wait(self: &Future<'t, T>) -> &'t T {
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

pub struct DeferScope<'t> {
    to_run: Mutex<Vec<Box<'t + FnBox() -> ()>>>,
    _marker: PhantomData<&'t ()>
}

impl<'t> DeferScope<'t> {
    fn defer<Func: 't + FnOnce() -> ()>(self: &DeferScope<'t>, f: Func) {
        self.to_run.lock().unwrap().push(Box::new(f));
    }

    pub fn spawn<Func>(self: &DeferScope<'t>, f: Func)
        where Func: 't + Send + FnOnce() -> ()
    {
        let to_send: Box<'t + FnBox() -> () + Send> = Box::new(f);
        let to_send: Box<'static + FnBox() -> () + Send> = unsafe{mem::transmute(to_send)};
        let to_join = thread::spawn(move || {
            FnBox::call_box(to_send, ());
        });
        self.defer(move || {
            to_join.join().unwrap();
        });
    }

    pub fn async<Func, R>(self: &DeferScope<'t>, f: Func) -> Future<'t, R>
        where Func: 't + Send + FnOnce() -> R
    {
        let (promise, future) = Promise::new();
        self.spawn(move || {
            promise.set(f());
        });
        future
    }
}

impl<'t> Drop for DeferScope<'t> {
    fn drop(self: &mut DeferScope<'t>) {
        let mut callbacks = Vec::new();
        mem::swap(&mut callbacks, &mut self.to_run.lock().unwrap());
        callbacks.into_iter().for_each(|x| {
            FnBox::call_box(x, ());
        });
    }
}

pub fn enter<'t, Func, R>(f: Func) -> R
    where Func: 't + FnOnce(&DeferScope<'t>) -> R
{
    let mut scope = DeferScope {
        to_run: Mutex::new(Vec::new()),
        _marker: PhantomData
    };
    f(&mut scope)
}

pub fn async<Func, R>(f: Func) -> Future<'static, R>
    where Func: 'static + Send + FnOnce() -> R,
          R: 'static
{
    let (promise, future) = Promise::new();
    thread::spawn(move || {
        promise.set(f());
    });
    future
}
