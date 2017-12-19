use future::Promise;
use std::sync::mpsc::channel;
use std::thread;

#[test]
fn check_work() {
    let (tx, rx) = channel();
    let promise = {
        let a = Promise::new();
        let tx = tx.clone();
        a.future().then(move |x| {
            tx.send(*x).unwrap();
        });
        a
    };
    thread::spawn(move || {
        promise.set_value(5);
    });
    assert_eq!(rx.recv().unwrap(), 5);
}
