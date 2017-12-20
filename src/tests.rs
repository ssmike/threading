use future::{Promise, Future};
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
