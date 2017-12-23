use std::sync::atomic::{AtomicI64, Ordering};
use std::ops::{DerefMut, Deref};
use std::cell::UnsafeCell;
use std::mem;

#[derive(Default)]
pub struct Spinlock<T> {
    cnt: AtomicI64,
    data: UnsafeCell<T>
}

unsafe impl<T> Sync for Spinlock<T> {}
unsafe impl<T> Send for Spinlock<T> {}

pub struct SpinlockGuard<'t, T: 't> {
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
    pub fn new(value: T) -> Spinlock<T> {
        Spinlock {
            cnt: AtomicI64::new(0),
            data: UnsafeCell::from(value)
        }
    }

    pub fn lock<'t>(self: &'t Spinlock<T>) -> (SpinlockGuard<'t, T>) {
        while self.cnt.fetch_add(1, Ordering::SeqCst) != 0 {
            self.cnt.fetch_sub(1, Ordering::SeqCst);
        }
        SpinlockGuard{parent: self}
    }

    pub unsafe fn get<'t>(self: &'t Spinlock<T>) -> &'t mut T {
        mem::transmute(self.data.get())
    }

    pub fn unwrap(self: Spinlock<T>) -> T {
        while self.cnt.fetch_add(1, Ordering::SeqCst) != 0 {
            self.cnt.fetch_sub(1, Ordering::SeqCst);
        }
        unsafe {self.data.into_inner()}
    }
}
