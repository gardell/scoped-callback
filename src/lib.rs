//! Allows registering scoped functions with local borrows with code that expect
//! functions taking `'static` lifetimes.

unsafe fn transmute_lifetime<'a, A: 'static, R: 'static>(
    value: Box<dyn FnMut(A) -> R + 'a>,
) -> Box<dyn FnMut(A) -> R + 'static> {
    std::mem::transmute(value)
}

struct Deregister<'a>(std::cell::RefCell<Option<Box<dyn FnOnce() + 'a>>>);

impl<'a> Deregister<'a> {
    fn new(f: Box<dyn FnOnce() + 'a>) -> Self {
        Self(std::cell::RefCell::new(Some(f)))
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
    deregister: std::rc::Rc<Deregister<'env>>,
    marker: std::marker::PhantomData<&'scope ()>,
}

impl<'env, 'scope> Drop for Registered<'env, 'scope> {
    fn drop(&mut self) {
        self.deregister.force()
    }
}

/// A `Scope` is used to register callbacks.
/// See [Scope::register](struct.Scope.html#method.register).
pub struct Scope<'env> {
    callbacks: std::cell::RefCell<Vec<std::rc::Rc<Deregister<'env>>>>,
    marker: std::marker::PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'env> {
    fn new() -> Self {
        Self {
            callbacks: std::cell::RefCell::new(Vec::new()),
            marker: std::marker::PhantomData,
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
        let c = std::rc::Rc::new(std::cell::RefCell::new(Some(c)));
        let handle = {
            let c = c.clone();
            register(Box::new(move |arg| {
                (c.as_ref()
                    .borrow_mut()
                    .as_mut()
                    .expect("Callback used after scope is unsafe"))(arg)
            }))
        };
        let deregister = std::rc::Rc::new(Deregister::new(Box::new(move || {
            deregister(handle);
            c.as_ref().borrow_mut().take();
        })));
        self.callbacks.borrow_mut().push(deregister.clone());
        Registered {
            deregister,
            marker: std::marker::PhantomData,
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
pub async fn scope_async<'env, R>(
    f: impl for<'r> FnOnce(&'r Scope<'env>) -> futures_util::future::BoxFuture<'r, R>
) -> R {
    f(&Scope::<'env>::new()).await
}

/// Same as [scope_async](fn.scope_async.html) but here `f` returns a `LocalBoxFuture` instead.
pub async fn scope_async_local<'env, R>(
    f: impl for<'r> FnOnce(&'r Scope<'env>) -> futures_util::future::LocalBoxFuture<'r, R>
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
                    let b = a * a;
                    println!("{}", b);
                },
                register,
                deregister,
            );

            std::mem::drop(registered);
        });
    }

    #[test]
    fn calling() {
        let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
        scope(|scope| {
            let registered = scope.register(
                |a| 2 * a,
                |callback| {
                    stored.as_ref().borrow_mut().replace(callback);
                },
                |_| {},
            );

            assert_eq!((stored.as_ref().borrow_mut().as_mut().unwrap())(42), 2 * 42);

            std::mem::drop(registered);
        });
    }

    #[test]
    fn drop_registered_causes_deregister() {
        let dropped = std::rc::Rc::new(std::cell::Cell::new(false));
        scope(|scope| {
            let registered = scope.register(|_| {}, register, {
                let dropped = dropped.clone();
                move |_| dropped.as_ref().set(true)
            });

            std::mem::drop(registered);
            assert!(dropped.as_ref().get());
        });
    }

    #[test]
    fn leaving_scope_causes_deregister() {
        let dropped = std::rc::Rc::new(std::cell::Cell::new(false));
        scope(|scope| {
            let registered = scope.register(|_| {}, register, {
                let dropped = dropped.clone();
                move |_| dropped.as_ref().set(true)
            });

            std::mem::forget(registered);
            assert!(!dropped.as_ref().get());
        });
        assert!(dropped.as_ref().get());
    }

    #[test]
    fn calling_static_callback_after_drop_panics() {
        let res = std::panic::catch_unwind(|| {
            let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
            scope(|scope| {
                let registered = scope.register(
                    |_| {},
                    |callback| {
                        stored.as_ref().borrow_mut().replace(callback);
                    },
                    |_| {},
                );

                std::mem::drop(registered);
                (stored.as_ref().borrow_mut().as_mut().unwrap())(42);
            });
        });
        assert!(res.is_err());
    }

    #[test]
    fn calling_static_callback_after_scope_panics() {
        let res = std::panic::catch_unwind(|| {
            let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
            scope(|scope| {
                let registered = scope.register(
                    |_| {},
                    |callback| {
                        stored.as_ref().borrow_mut().replace(callback);
                    },
                    |_| {},
                );

                std::mem::forget(registered);
            });
            (stored.as_ref().borrow_mut().as_mut().unwrap())(42);
        });
        assert!(res.is_err());
    }

    #[test]
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

                std::mem::forget(registered);
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
