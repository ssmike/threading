use std::sync::atomic::{Ordering, AtomicUsize};
use spinlock::{SpinRWLock, Spinlock};
use std::sync::Arc;
use std::mem;

pub struct Atom<T> {
    data: [SpinRWLock<Option<Arc<T>>>; 2],
    current: AtomicUsize,
    write_guard: Spinlock<()>
}

impl<T> Atom<T> {
    pub fn new(val: T) -> Self {
        let ptr = Arc::new(val);
        Atom {
            data: [SpinRWLock::new(Some(ptr)), SpinRWLock::new(None)],
            current: AtomicUsize::new(0),
            write_guard: Spinlock::new(())
        }
    }

    pub fn load(&self) -> Arc<T> {
        let guard = self.data[self.get_idx()].read();
        guard.as_ref().unwrap().clone()
    }

    pub fn store_val(&self, val: T) {
        self.store(Arc::new(val))
    }
    
    pub fn store(&self, val: Arc<T>) {
        let _ = self.write_guard.lock();
        self.switch();
        let mut guard = self.data[self.get_idx()].write();
        let mut wrapped = Some(val);
        mem::swap(&mut wrapped, &mut *guard);
    }

    fn get_idx(&self) -> usize {
        self.current.load(Ordering::SeqCst) % 2
    }

    fn switch(&self) {
        self.current.fetch_add(1, Ordering::SeqCst);
    }
}
