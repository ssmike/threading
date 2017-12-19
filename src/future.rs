use std::option::Option;
use std::boxed::{Box, FnBox};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc};
use std::mem;
use std::cell::UnsafeCell;

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

impl<T> Spinlock<T> {
    fn new(value: T) -> Spinlock<T> {
        Spinlock {
            cnt: AtomicI64::new(0),
            data: UnsafeCell::from(value)
        }
    }

    fn lock<'t>(self: &'t Spinlock<T>) -> (&'t mut T, SpinlockGuard<'t, T>) {
        while self.cnt.fetch_add(1, Ordering::SeqCst) != 0 {
            self.cnt.fetch_sub(1, Ordering::SeqCst);
        }
        (unsafe {self.get()}, SpinlockGuard{parent: self})
    }

    unsafe fn get<'t>(self: &'t Spinlock<T>) -> &'t mut T {
        mem::transmute(self.data.get())
    }
}

struct FutureState<'t, T> {
    value: Option<T>,
    callbacks: Vec<Box<'t + FnBox(&T) -> ()>>
}

impl<'t, T> FutureState<'t, T> {
    fn new(value: T) -> FutureState<'t, T> {
        FutureState {
            value: Option::Some(value),
            callbacks: Vec::new()
        }
    }
}

impl<'t, T> Default for FutureState<'t, T> {
    fn default() -> FutureState<'t, T> {
        FutureState {
            value: Option::None,
            callbacks: Vec::new()
        }
    }
}

#[derive(Clone, Default)]
pub struct Future<'t, T> {
    state: Arc<Spinlock<FutureState<'t, T>>>
}

unsafe impl<'t, T> Send for Future<'t, T> {}

#[derive(Default)]
pub struct Promise<'t, T> {
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
            let (ref mut state, _) = self.state.lock();
            state.value = Option::Some(value);
            let mut vec = Vec::<Box<FnBox(&T) -> ()>>::new();
            mem::swap(&mut vec, &mut state.callbacks);
            vec
        };
        let state = unsafe { self.state.get() };
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
        let (state, guard) = self.state.lock();
        match state.value {
            None => {
                state.callbacks.push(Box::new(f));
            },
            Some(_) => {
                mem::drop(guard);
                let state = unsafe { self.state.get() };
                f(state.value.as_ref().unwrap());
            }
        }
    }

}
