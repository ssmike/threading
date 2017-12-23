#![feature(integer_atomics)]
#![feature(fnbox)]

pub mod future;
pub mod async;
pub mod event;

mod spinlock;

#[cfg(test)]
mod tests;

