# scoped-callback

Allows registering scoped functions with local borrows with code that expect
functions taking `'static` lifetimes.

Motivating example:

```rust
/// Function for registering a callback with a `'static` lifetime.
fn register(callback: Box<dyn FnMut(i32)>) -> Box<dyn FnMut(i32)> {
  callback
}
/// Function for de-registering the handle returned by `register`,
/// in this case the callback itself.
fn deregister(_callback: Box<dyn FnMut(i32)>) {}

/// Variable that can be borrowed from inside the callback closure
let a = 42;

/// After returning from the closure, `scope` guarantees that any callbacks
/// that have not yet been de-registered are de-registered.
scope(|scope| {

  /// Register the given closure, which can borrow from the stack outside `scope`
  /// using the `register` and `deregister` functions declared above.
  /// The returned handle will cause a de-register when dropped.
  let _registered = scope.register(
    |_| {
      let b = a * a;
      println!("{}", b);
    },
    register,
    deregister,
  );
});
```
See [scope_async](https://docs.rs/scoped-callback/fn.scope_async.html) and [scope_async_local](fn.scope_async_local.html)
as well for versions that work with `async` scopes.

## How is this safe?
There are three important concepts in this implementation:
* [register](https://docs.rs/scoped-callback/struct.Scope.html#method.register) returns a [Registered](struct.Registered.html)
  instance, which when [Drop](https://docs.rs/scoped-callback/struct.Registered.html#impl-Drop)-ed causes the callback to be
  de-registered using the provided function.
* In case the [Registered](https://docs.rs/scoped-callback/struct.Registered.html) instance is not
  [Drop](https://docs.rs/scoped-callback/struct.Registered.html#impl-Drop)-ed, for example by calling
  [std::mem::forget](https://docs.rs/scoped-callback/https://doc.rust-lang.org/std/mem/fn.forget.html) (which is *not* `unsafe`!)
  the de-registering using the provided function will instead happen after leaving the closure
  passed to [scope](https://docs.rs/scoped-callback/fn.scope.html).
* In case the given de-register function doesn't actually de-register the callback,
  and for some reason the callback given to the [register](https://docs.rs/scoped-callback/struct.Scope.html#method.register)
  function is called after the closure passed to [scope](https://docs.rs/scoped-callback/fn.scope.html), the call will cause a
  `panic!`.

License: Apache-2.0
