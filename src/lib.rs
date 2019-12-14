//! Allows registering scoped functions with local borrows with code that expect
//! functions taking `'static` lifetimes.

unsafe fn transmute_lifetime<'a, A: 'static, R: 'static>(
    value: Box<dyn FnMut(A) -> R + 'a>,
) -> Box<dyn FnMut(A) -> R + 'static> {
    std::mem::transmute(value)
}

struct Deregister(std::cell::RefCell<Option<Box<dyn FnOnce()>>>);

impl Deregister {
    fn new(f: Box<dyn FnOnce()>) -> Self {
        Self(std::cell::RefCell::new(Some(f)))
    }

    fn force(&self) {
        if let Some(f) = self.0.borrow_mut().take() {
            f();
        }
    }
}

impl Drop for Deregister {
    fn drop(&mut self) {
        self.force();
    }
}

/// A handle returned by [Scope::register](struct.Scope.html#method.register).
/// When this handle is dropped, the callback is de-registered.
pub struct Registered<'scope> {
    deregister: std::rc::Rc<Deregister>,
    marker: std::marker::PhantomData<&'scope ()>,
}

impl<'a> Drop for Registered<'a> {
    fn drop(&mut self) {
        self.deregister.force()
    }
}

/// A `Scope` is used to register callbacks.
/// See [Scope::register](struct.Scope.html#method.register).
pub struct Scope<'env> {
    callbacks: std::cell::RefCell<Vec<std::rc::Rc<Deregister>>>,
    marker: std::marker::PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'env> {
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
        register: impl FnOnce(Box<dyn FnMut(A) -> R>) -> H,
        deregister: impl FnOnce(H) + 'static,
    ) -> Registered<'scope> {
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
    f(&Scope::<'env> {
        callbacks: std::cell::RefCell::new(Vec::new()),
        marker: std::marker::PhantomData,
    })
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
                }, register, deregister);

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
}
