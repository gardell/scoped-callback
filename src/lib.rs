//! Allows registering scoped functions with local borrows with code that expect
//! functions taking `'static` lifetimes.
//!
//! Motivating example:
//!
//! ```rust
//! # use scoped_callback::scope;
//! /// Function for registering a callback with a `'static` lifetime.
//! fn register(callback: Box<dyn FnMut(i32)>) -> Box<dyn FnMut(i32)> {
//!   callback
//! }
//! /// Function for de-registering the handle returned by `register`,
//! /// in this case the callback itself.
//! fn deregister(_callback: Box<dyn FnMut(i32)>) {}
//!
//! /// Variable that can be borrowed from inside the callback closure
//! let a = 42;
//!
//! /// After returning from the closure, `scope` guarantees that any callbacks
//! /// that have not yet been de-registered are de-registered.
//! scope(|scope| {
//!
//!   /// Register the given closure, which can borrow from the stack outside `scope`
//!   /// using the `register` and `deregister` functions declared above.
//!   /// The returned handle will cause a de-register when dropped.
//!   let _registered = scope.register(
//!     |_| {
//!       let b = a * a;
//!       println!("{}", b);
//!     },
//!     register,
//!     deregister,
//!   );
//! });
//! ```
//! See [scope_async](fn.scope_async.html) and [scope_async_local](fn.scope_async_local.html)
//! as well for versions that work with `async` scopes.
//!
//! # How is this safe?
//! There are three important concepts in this implementation:
//! * [register](struct.Scope.html#method.register) returns a [Registered](struct.Registered.html)
//!   instance, which when [Drop](struct.Registered.html#impl-Drop)-ed causes the callback to be
//!   de-registered using the provided function.
//! * In case the [Registered](struct.Registered.html) instance is not
//!   [Drop](struct.Registered.html#impl-Drop)-ed, for example by calling `std::mem::forget`
//!   (which is *not* `unsafe`!)
//!   the de-registering using the provided function will instead happen after leaving the closure
//!   passed to [scope](fn.scope.html).
//! * In case the given de-register function doesn't actually de-register the callback,
//!   and for some reason the callback given to the [register](struct.Scope.html#method.register)
//!   function is called after the closure passed to [scope](fn.scope.html), the call will cause a
//!   `panic!`.
//!
//! # `no_std`
//! This crate supports `no_std` by disabling its `std` feature.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, rc::Rc, vec::Vec};
#[cfg(feature = "std")]
use std::rc::Rc;

unsafe fn transmute_lifetime<'a, A: 'static, R: 'static>(
    value: Box<dyn FnMut(A) -> R + 'a>,
) -> Box<dyn FnMut(A) -> R + 'static> {
    core::mem::transmute(value)
}

struct Deregister<'a>(core::cell::RefCell<Option<Box<dyn FnOnce() + 'a>>>);

impl<'a> Deregister<'a> {
    fn new(f: Box<dyn FnOnce() + 'a>) -> Self {
        Self(core::cell::RefCell::new(Some(f)))
    }

    fn force(&self) {
        if let Some(f) = self.0.borrow_mut().take() {
            f();
        }
    }
}

impl<'a> Drop for Deregister<'a> {
    fn drop(&mut self) {
        self.force();
    }
}

/// A handle returned by [Scope::register](struct.Scope.html#method.register).
/// When this handle is dropped, the callback is de-registered.
pub struct Registered<'env, 'scope> {
    deregister: Rc<Deregister<'env>>,
    marker: core::marker::PhantomData<&'scope ()>,
}

impl<'env, 'scope> Drop for Registered<'env, 'scope> {
    fn drop(&mut self) {
        self.deregister.force()
    }
}

/// A `Scope` is used to register callbacks.
/// See [Scope::register](struct.Scope.html#method.register).
pub struct Scope<'env> {
    callbacks: core::cell::RefCell<Vec<Rc<Deregister<'env>>>>,
    marker: core::marker::PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'env> {
    fn new() -> Self {
        Self {
            callbacks: core::cell::RefCell::new(Vec::new()),
            marker: core::marker::PhantomData,
        }
    }

    /// Register the function `c` with local lifetime `'env` using the `register` and `deregister`
    /// functions that handle only `'static` lifetime functions.
    /// The returned `Registered` object will, when dropped, invoke the `deregister` function.
    ///
    /// If the `Registered` object is `std::mem::forget`-ed, `Scope::drop` will
    /// perform the de-registration.
    ///
    /// *Note*: If the callback passed to the `register` function is invoked after `deregister`
    /// has been invoked, the callback will `panic!`.
    pub fn register<'scope, A: 'static, R: 'static, H: 'static>(
        &'scope self,
        c: impl (FnMut(A) -> R) + 'env,
        register: impl FnOnce(Box<dyn FnMut(A) -> R>) -> H + 'env,
        deregister: impl FnOnce(H) + 'env,
    ) -> Registered<'env, 'scope> {
        let c = unsafe { transmute_lifetime(Box::new(c)) };
        let c = Rc::new(core::cell::RefCell::new(Some(c)));
        let handle = {
            let c = c.clone();
            register(Box::new(move |arg| {
                (c.as_ref()
                    .borrow_mut()
                    .as_mut()
                    .expect("Callback used after scope is unsafe"))(arg)
            }))
        };
        let deregister = Rc::new(Deregister::new(Box::new(move || {
            deregister(handle);
            c.as_ref().borrow_mut().take();
        })));
        self.callbacks.borrow_mut().push(deregister.clone());
        Registered {
            deregister,
            marker: core::marker::PhantomData,
        }
    }
}

impl<'env> Drop for Scope<'env> {
    fn drop(&mut self) {
        self.callbacks
            .borrow()
            .iter()
            .for_each(|deregister| deregister.force());
    }
}

/// Call `scope` to receive a `Scope` instance that can be used to register functions.
/// See [Scope::register](struct.Scope.html#method.register).
pub fn scope<'env, R>(f: impl FnOnce(&Scope<'env>) -> R) -> R {
    f(&Scope::<'env>::new())
}

/// Same as [scope](fn.scope.html) but also allow `async` borrows.
///
/// The `Scope` instance passed to `f` can not outlive the call of this function.
/// However, for async functions, this would be useless as the function returns a `Future`
/// that is yet to complete, and may contain references to the given `Scope`.
/// In order to remedy this, `scope_async` explicitly makes sure `Scope` lives throughout
/// the lifetime of the future returned by `f`.
#[cfg(feature = "async")]
pub async fn scope_async<'env, R>(
    f: impl for<'r> FnOnce(&'r Scope<'env>) -> futures_util::future::BoxFuture<'r, R>,
) -> R {
    f(&Scope::<'env>::new()).await
}

/// Same as [scope_async](fn.scope_async.html) but here `f` returns a `LocalBoxFuture` instead.
#[cfg(feature = "async")]
pub async fn scope_async_local<'env, R>(
    f: impl for<'r> FnOnce(&'r Scope<'env>) -> futures_util::future::LocalBoxFuture<'r, R>,
) -> R {
    f(&Scope::<'env>::new()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register(callback: Box<dyn FnMut(i32)>) -> Box<dyn FnMut(i32)> {
        callback
    }

    fn deregister(_callback: Box<dyn FnMut(i32)>) {}

    #[test]
    fn it_works() {
        let a = 42;
        scope(|scope| {
            let registered = scope.register(
                |_| {
                    let _b = a * a;
                },
                register,
                deregister,
            );

            core::mem::drop(registered);
        });
    }

    #[test]
    fn calling() {
        let stored = Rc::new(core::cell::RefCell::new(None));
        scope(|scope| {
            let registered = scope.register(
                |a| 2 * a,
                |callback| {
                    stored.as_ref().borrow_mut().replace(callback);
                },
                |_| {},
            );

            assert_eq!((stored.as_ref().borrow_mut().as_mut().unwrap())(42), 2 * 42);

            core::mem::drop(registered);
        });
    }

    #[test]
    fn drop_registered_causes_deregister() {
        let dropped = Rc::new(core::cell::Cell::new(false));
        scope(|scope| {
            let registered = scope.register(|_| {}, register, {
                let dropped = dropped.clone();
                move |_| dropped.as_ref().set(true)
            });

            core::mem::drop(registered);
            assert!(dropped.as_ref().get());
        });
    }

    #[test]
    fn leaving_scope_causes_deregister() {
        let dropped = Rc::new(core::cell::Cell::new(false));
        scope(|scope| {
            let registered = scope.register(|_| {}, register, {
                let dropped = dropped.clone();
                move |_| dropped.as_ref().set(true)
            });

            core::mem::forget(registered);
            assert!(!dropped.as_ref().get());
        });
        assert!(dropped.as_ref().get());
    }

    #[test]
    /// Note: catch_unwind not available with `no_std`,
    /// See https://github.com/rust-lang/rfcs/issues/2810
    #[cfg(feature = "std")]
    fn calling_static_callback_after_drop_panics() {
        let res = std::panic::catch_unwind(|| {
            let stored = Rc::new(core::cell::RefCell::new(None));
            scope(|scope| {
                let registered = scope.register(
                    |_| {},
                    |callback| {
                        stored.as_ref().borrow_mut().replace(callback);
                    },
                    |_| {},
                );

                core::mem::drop(registered);
                (stored.as_ref().borrow_mut().as_mut().unwrap())(42);
            });
        });
        assert!(res.is_err());
    }

    #[test]
    /// Note: catch_unwind not available with `no_std`,
    /// See https://github.com/rust-lang/rfcs/issues/2810
    #[cfg(feature = "std")]
    fn calling_static_callback_after_scope_panics() {
        let res = std::panic::catch_unwind(|| {
            let stored = Rc::new(core::cell::RefCell::new(None));
            scope(|scope| {
                let registered = scope.register(
                    |_| {},
                    |callback| {
                        stored.as_ref().borrow_mut().replace(callback);
                    },
                    |_| {},
                );

                core::mem::forget(registered);
            });
            (stored.as_ref().borrow_mut().as_mut().unwrap())(42);
        });
        assert!(res.is_err());
    }

    #[test]
    /// Note: catch_unwind not available with `no_std`,
    /// See https://github.com/rust-lang/rfcs/issues/2810
    #[cfg(feature = "std")]
    fn panic_in_scoped_is_safe() {
        let stored = std::sync::Mutex::new(None);
        let res = std::panic::catch_unwind(|| {
            scope(|scope| {
                let registered = scope.register(
                    |_| {},
                    |callback| {
                        stored.lock().unwrap().replace(callback);
                    },
                    |_| {},
                );

                core::mem::forget(registered);
                panic!()
            });
        });
        assert!(res.is_err());
        let res = std::panic::catch_unwind(|| {
            (stored.lock().unwrap().as_mut().take().unwrap())(42);
        });
        assert!(res.is_err());
    }
}
