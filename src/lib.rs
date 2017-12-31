#![feature(integer_atomics)]
#![feature(fnbox)]

pub mod future;
pub mod async;
pub mod event;
pub mod atom;
pub mod spinlock;

#[cfg(test)]
mod tests;

