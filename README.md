### Future

This library is my attempt to implement [arcadia](https://yandex.com/company/) futures in Rust.

Mapping from future.rs entities and methods to arcadia Nthreading.

future | Arcadia
--------|-------
Future | TFuture
Future::new | MakeFuture
Future::apply/map/then/chain | TFuture::Apply
Future::get | TFuture::GetValueSync
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

TODO: refine apply methods family
