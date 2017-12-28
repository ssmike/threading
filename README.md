### Threading [![Build Status](https://travis-ci.org/ssmike/threading.svg?branch=master)](https://travis-ci.org/ssmike/threading)

This library is my attempt to implement [arcadia](https://yandex.com/company/) threading primitives in Rust.

Mapping from threading entities and methods to arcadia Nthreading.

threading | Arcadia
--------|-------
Future | TFuture
Future::new | MakeFuture
Future/SharedFuture::apply/then | TFuture::Apply
SharedFuture::get | TFuture::GetValueSync
Future::take | TFuture::ExtractValueSync
Promise | TPromise
Promise::new | NewPromise + TPromise::GetFuture
Promise::set | TPromise::SetValue
Event | TManualEvent
Event::reset | TManualEvent::Reset
Event::signal | TManualEvent::Signal
async/DeferScope::async | TLegacyFuture
wait_all | WaitAll
wait_any | WaitAny


Mimicking ugly enterprise interfaces was not priority, so there are no GetValue and Promises could be set only once.
