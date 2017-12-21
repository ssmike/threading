use future::{Promise, Future, enter, async};
use std::sync::mpsc::channel;
use std::thread;
use std::time;

#[test]
fn check_work() {
    let test_val = 5;
    let (tx, rx) = channel();
    let promise = {
        let (promise, future) = Promise::new();
        let tx = tx.clone();
        future.then(move |x| {
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
fn check_wait() {
    let (promise, future) = Promise::<i32>::new();
    thread::spawn(move || {
        promise.set(2 + 2);
    });
    assert_eq!(future.wait(), &4);
}

#[test]
fn check_static_async() {
    let r = async(|| {
        thread::sleep(time::Duration::from_millis(4));
        2 + 2
    });
    assert_eq!(r.wait(), &4);
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
        }).then(|t| {
            thread::sleep(time::Duration::from_millis(4));
            println!("{}", t);
            *t + arr[2]
        });
        res1.wait();
        res2.wait();
        assert_eq!(res1.wait(), res2.wait());
        *res1.wait()
    });
    assert_eq!(sm, res1);
}
