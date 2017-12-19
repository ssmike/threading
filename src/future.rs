use std::option::Option;
use std::boxed::Box;
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

struct FutureState<T> {
    value: Option<T>,
    callbacks: Vec<Box<Fn(&T) -> ()>>
}

impl<T> FutureState<T> {
    fn new(value: T) -> FutureState<T> {
        FutureState {
            value: Option::Some(value),
            callbacks: Vec::new()
        }
    }
}

impl<T> Default for FutureState<T> {
    fn default() -> FutureState<T> {
        FutureState {
            value: Option::None,
            callbacks: Vec::new()
        }
    }
}

#[derive(Clone, Default)]
pub struct Future<T> {
    state: Arc<Spinlock<FutureState<T>>>
}

#[derive(Clone, Default)]
pub struct Promise<T> {
    state: Arc<Spinlock<FutureState<T>>>
}

impl<T> Promise<T> {
    pub fn new() -> Promise<T> {
        Promise {
            state: Arc::new(Spinlock::new(FutureState::default()))
        }
    }

    pub fn future(self: &Promise<T>) -> Future<T> {
        Future {
            state: self.state.clone()
        }
    }

    pub fn set_value(self: &Promise<T>, value: T) {
        let callbacks = {
            let (ref mut state, _) = self.state.lock();
            state.value = Option::Some(value);
            let mut vec = Vec::<Box<Fn(&T) -> ()>>::new();
            mem::swap(&mut vec, &mut state.callbacks);
            vec
        };
        let state = unsafe { self.state.get() };
        let value = state.value.as_ref().unwrap();
        callbacks.into_iter().for_each(|f| {
            f(value);
        });
    }
}

impl<T> Future<T> {
    pub fn new(val: T) -> Future<T> {
        Future {
            state: Arc::new(Spinlock::new(FutureState::new(val)))
        }
    }

    pub fn then<R: 'static, Func: 'static + Fn(&T) -> R>(self: &Future<T>, f: Func) -> Future<R> {
        let to_set = Promise::new();
        let res = to_set.future();
        self.subscribe(move |x| {to_set.set_value(f(x));});
        res
    }

    fn subscribe<Func: 'static + Fn(&T) -> ()>(self: &Future<T>, f: Func) {
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
