use future::Promise;
use std::sync::mpsc::channel;

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
    promise.set_value(5);
    assert_eq!(rx.recv().unwrap(), 5);
}
