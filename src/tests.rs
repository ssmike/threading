use future::Promise;

#[test]
fn check_work() {
    let a = Promise::new();
    a.future().then(|x| {
        assert_eq!(*x, 4);
        println!("value is {}", x);
    });
    a.set_value(4);
}
