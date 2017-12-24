use std::sync::{Mutex, Condvar};

pub struct Event {
    var: Condvar,
    set: Mutex<bool>
}

impl Event {
    pub fn new() -> Event {
        Event {
            set: Mutex::new(false),
            var: Condvar::new()
        }
    }

    pub fn reset(self: &Event) {
        *(self.set.lock().unwrap()) = false;
    }

    pub fn wait(self: &Event) {
        let mut lock = self.set.lock().unwrap();
        loop {
            if *lock {
                break;
            } else {
                lock = self.var.wait(lock).unwrap();
            }
        }
    }

    pub fn signal(self: &Event) {
        let mut lock = self.set.lock().unwrap();
        *lock = true;
        self.var.notify_all();
    }
}
