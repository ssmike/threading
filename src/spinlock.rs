use std::sync::atomic::{Ordering, AtomicBool, AtomicI16};
use std::ops::{DerefMut, Deref};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem;

#[derive(Default)]
pub struct Spinlock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
    read_only: AtomicBool
}

unsafe impl<T: Send> Sync for Spinlock<T> {} //we don't allow to share() !Sync values
unsafe impl<T: Send> Send for Spinlock<T> {}

pub struct SpinlockGuard<'t, T: 't> {
    parent: &'t Spinlock<T>,
    _marker: PhantomData<&'t mut T>
}

impl<'t, T: 't> Drop for SpinlockGuard<'t, T> {
    fn drop(self: &mut SpinlockGuard<'t, T>) {
        self.parent.locked.store(false, Ordering::Release);
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
            locked: AtomicBool::new(false),
            read_only: AtomicBool::new(false),
            data: UnsafeCell::from(value)
        }
    }

    fn read_only(self: &Spinlock<T>) -> bool {
        self.read_only.load(Ordering::Acquire)
    }

    fn take(self: &Spinlock<T>) -> bool {
        while !self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            if self.read_only() {
                return false;
            }
        }
        true
    }

    pub fn lock<'t>(self: &'t Spinlock<T>) -> Option<SpinlockGuard<'t, T>> {
        if self.take() {
            Some(SpinlockGuard{parent: self, _marker: PhantomData})
        } else {
            None
        }
    }
}

impl<T: Sync> Spinlock<T> {
    pub fn share(self: &Spinlock<T>) -> &T {
        if !self.read_only() {
            self.take();
            self.read_only.store(true, Ordering::Release)
        }
        unsafe {mem::transmute(self.data.get())}
    }
}

pub struct SpinRWLock<T> {
    data: UnsafeCell<T>,
    readers: AtomicI16,
    write: AtomicBool
}

unsafe impl<T: Send + Sync> Sync for SpinRWLock<T> {}
unsafe impl<T: Send> Send for SpinRWLock<T> {}

pub struct SpinReadGuard<'t, T: 't> {
    parent: &'t SpinRWLock<T>,
    _marker: PhantomData<&'t T>
}

impl<'t, T: 't> Deref for SpinReadGuard<'t, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe {mem::transmute(self.parent.data.get())}
    }
}

pub struct SpinWriteGuard<'t, T: 't> {
    parent: &'t SpinRWLock<T>,
    _marker: PhantomData<&'t mut T>
}

impl<'t, T: 't> Deref for SpinWriteGuard<'t, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe {mem::transmute(self.parent.data.get())}
    }
}

impl<'t, T: 't> DerefMut for SpinWriteGuard<'t, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe {mem::transmute(self.parent.data.get())}
    }
}

impl<T> SpinRWLock<T> {
    pub fn new(val: T) -> Self {
        SpinRWLock {
            data: UnsafeCell::new(val),
            readers: AtomicI16::new(0),
            write: AtomicBool::new(false)
        }
    }

    pub fn read<'t>(&'t self) -> SpinReadGuard<'t, T> {
        loop {
            self.readers.fetch_add(1, Ordering::SeqCst);
            if !self.write.load(Ordering::SeqCst) { break; }
            self.readers.fetch_sub(1, Ordering::SeqCst);
        }
        SpinReadGuard {
            parent: self,
            _marker: PhantomData
        }
    }

    pub fn write<'t>(&'t self) -> SpinWriteGuard<'t, T> {
        while !self.write.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {}
        while self.readers.load(Ordering::Acquire) != 0 {}
        SpinWriteGuard {
            parent: self,
            _marker: PhantomData
        }
    }
}

impl<'t, T: 't> Drop for SpinWriteGuard<'t, T> {
    fn drop(&mut self) {
        self.parent.write.store(false, Ordering::Release);
    }
}

impl<'t, T: 't> Drop for SpinReadGuard<'t, T> {
    fn drop(&mut self) {
        self.parent.readers.fetch_sub(1, Ordering::Release);
    }
}

