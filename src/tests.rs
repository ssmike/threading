use future::{Promise, Future, wait_all, wait_any};
use async::{enter, async, DeferScope};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time;
use spinlock::Spinlock;
use std::rc::Rc;
use std::cell::RefCell;

#[test]
fn check_spinlock() {
    let s = Spinlock::new(RefCell::new(5));
    let l = s.lock().unwrap();
    enter(|scope| {
        scope.spawn(move || { //refcell isn't sync, so we can't share reference(but can move)
            *l.borrow_mut() = 6;
        });
    });
}


#[test]
fn check_single() {
    let (promise, future) = Promise::new();
    promise.set(2);
    assert_eq!(*future, 2);
}

#[test]
fn check_rc() {
    let (promise, future) = Promise::new();
    //thread::spawn(move || {
    //    promise.set(Rc::new(5));// such promises aren't send
    //});
    promise.set(Rc::new(5));
    future.apply(|future| {future.take();});
    //thread::spawn(move || {
    //    future.apply(|future| {future.take();})// ... and futures
    //});
}


#[test]
fn check_refcell() {
    let (promise, future) = Promise::new();
    thread::spawn(move || {
        promise.set(RefCell::new(4)); // but for send values futures and promises are send
    });
    //*future; // But we can't dereference such futures.
    assert_eq!(future.take().into_inner(), 4);
}

#[test]
fn check_work() {
    let test_val = 5;
    let (tx, rx)  = channel();
    let promise = {
        let (promise, future) = Promise::<i32>::new();
        let tx = tx.clone();
        future.apply(move |x| {
            tx.send(*x).unwrap();
        });
        promise
    };
    thread::spawn(move || {
        promise.set(test_val);
    });
    assert_eq!(rx.recv().unwrap(), test_val);
}

#[test]
fn check_scoped() {
    let mut x = 5;
    enter(|scope| {
        assert_eq!(x, 5);
        let mut y = 7;
        scope.spawn(|| {
            //wouldn't compile, should outlive scope
            //assert_eq!(y, 2);
            x += 5
        });
        //wouldn't compile, borrowed mutably
        //assert_eq!(x, 5 + 5);
    });
    assert_eq!(x, 5 + 5);
}

#[test]
fn check_get() {
    let (promise, future) = Promise::<i32>::new();
    thread::spawn(move || {
        promise.set(2 + 2);
    });
    assert_eq!(*future, 4);
}

#[test]
fn check_static_async() {
    let r = async(|| {
        thread::sleep(time::Duration::from_millis(4));
        2 + 2
    });
    assert_eq!(*r, 4);
}

#[test]
fn check_asyncs() {
    let arr = [5, 4, 9];
    let mut x = arr[0];
    let sm = arr.iter().sum();
    let res2 = async(move || sm);
    let res1 = enter(|scope| {
        let res1 = scope.async(|| {
            thread::sleep(time::Duration::from_millis(2));
            x += arr[1];
            x
        }).apply(|t| {
            thread::sleep(time::Duration::from_millis(4));
            println!("{}", *t);
            *t + arr[2]
        });
        assert_eq!(*res1, *res2);
        *res1
    });
    assert_eq!(sm, res1);
}

#[test]
fn check_wait_all() {
    let cnt = Arc::new(AtomicI64::new(0));
    let f1 = {
        let cnt = cnt.clone();
        async(move || {
            thread::sleep(time::Duration::from_millis(40));
            cnt.fetch_add(1, Ordering::Relaxed);
        })
    };
    let f2 = {
        let cnt = cnt.clone();
        async(move || {
            thread::sleep(time::Duration::from_millis(2));
            cnt.fetch_add(1, Ordering::Relaxed);
        })
    };
    wait_all(vec![f1, f2].into_iter()).apply(move |_| assert_eq!(cnt.load(Ordering::SeqCst), 2)).take();
}
