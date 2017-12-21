use future::{Promise, Future, enter, spawn};
use std::sync::mpsc::channel;
use std::thread;

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
        spawn(scope, || {
            //wouldn't compile, should outlive scope
            //assert_eq!(y, 2);
            x += 5
        });
        //wouldn't compile, borrowed mutably
        //assert_eq!(x, 5 + 5);
    });
    assert_eq!(x, 5 + 5);
}
