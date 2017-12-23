use std::sync::{Mutex, Arc};
use std::marker::PhantomData;
use std::boxed::{Box, FnBox};
use future::{Future, Promise};
use std::thread;
use std::mem;

pub struct DeferScope<'t> {
    to_run: Mutex<Vec<Box<'t + FnBox() -> ()>>>,
    _marker: PhantomData<&'t ()>
}

impl<'t> DeferScope<'t> {
    pub fn defer<Func: 't + FnOnce() -> ()>(self: &DeferScope<'t>, f: Func) {
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
        where Func: 't + Send + FnOnce() -> R,
              R: Sync + Send
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
          R: 'static + Send + Sync
{
    let (promise, future) = Promise::new();
    thread::spawn(move || {
        promise.set(f());
    });
    future
}
