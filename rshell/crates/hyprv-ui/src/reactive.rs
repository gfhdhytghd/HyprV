//! Single-thread reactive properties with automatic dependency tracking and RAII effects.
//! Effects run synchronously only on unequal writes. UI scene mutations still go through Scene::edit.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
thread_local! { static ACTIVE: RefCell<Option<Weak<EffectInner>>> = const { RefCell::new(None) }; }
struct EffectInner {
    callback: RefCell<Box<dyn FnMut()>>,
    running: Cell<bool>,
    dependencies: RefCell<Vec<Rc<Signal>>>,
}
#[derive(Default)]
struct Signal {
    listeners: RefCell<Vec<Weak<EffectInner>>>,
}
impl Signal {
    fn track(self: &Rc<Self>) {
        ACTIVE.with(|a| {
            if let Some(w) = a.borrow().as_ref() {
                if let Some(e) = w.upgrade() {
                    if !e.dependencies.borrow().iter().any(|s| Rc::ptr_eq(s, self)) {
                        self.listeners.borrow_mut().push(w.clone());
                        e.dependencies.borrow_mut().push(self.clone());
                    }
                }
            }
        });
    }
    fn notify(&self) {
        let listeners = self.listeners.borrow().clone();
        self.listeners.borrow_mut().retain(|w| w.strong_count() > 0);
        for w in listeners {
            if let Some(e) = w.upgrade() {
                EffectInner::run(&e);
            }
        }
    }
}
impl Drop for EffectInner {
    fn drop(&mut self) {
        let identity = self as *const Self;
        for dependency in self.dependencies.get_mut().drain(..) {
            dependency
                .listeners
                .borrow_mut()
                .retain(|w| w.as_ptr() != identity);
        }
    }
}
impl EffectInner {
    fn run(this: &Rc<Self>) {
        assert!(!this.running.replace(true), "reactive binding cycle");
        struct Guard<'a>(&'a EffectInner, Option<Weak<EffectInner>>);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                ACTIVE.with(|a| *a.borrow_mut() = self.1.take());
                self.0.running.set(false);
            }
        }
        for dep in this.dependencies.borrow_mut().drain(..) {
            dep.listeners
                .borrow_mut()
                .retain(|w| w.as_ptr() != Rc::as_ptr(this));
        }
        let previous = ACTIVE.with(|a| a.replace(Some(Rc::downgrade(this))));
        let _guard = Guard(this, previous);
        (this.callback.borrow_mut())();
    }
}
#[derive(Clone)]
pub struct Property<T> {
    value: Rc<RefCell<T>>,
    signal: Rc<Signal>,
}
impl<T: Clone + PartialEq + 'static> Property<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
            signal: Rc::new(Signal::default()),
        }
    }
    pub fn get(&self) -> T {
        self.signal.track();
        self.value.borrow().clone()
    }
    pub fn set(&self, value: T) -> bool {
        if *self.value.borrow() == value {
            return false;
        }
        *self.value.borrow_mut() = value;
        self.signal.notify();
        true
    }
    /// The returned handle must be retained for the lifetime of the binding.
    pub fn bind(&self, mut compute: impl FnMut() -> T + 'static) -> Effect {
        let target = self.clone();
        Effect::new(move || {
            target.set(compute());
        })
    }
}
/// Dropping the final handle disconnects the effect without a global scheduler.
#[derive(Clone)]
pub struct Effect(Rc<EffectInner>);
impl Effect {
    pub fn new(callback: impl FnMut() + 'static) -> Self {
        let effect = Self(Rc::new(EffectInner {
            callback: RefCell::new(Box::new(callback)),
            running: Cell::new(false),
            dependencies: RefCell::new(Vec::new()),
        }));
        EffectInner::run(&effect.0);
        effect
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_changes_propagate_and_drop_disconnects() {
        let source = Property::new(2);
        let result = Property::new(0);
        let binding = result.bind({
            let s = source.clone();
            move || s.get() * 2
        });
        assert_eq!(result.get(), 4);
        assert!(!source.set(2));
        source.set(3);
        assert_eq!(result.get(), 6);
        drop(binding);
        source.set(4);
        assert_eq!(result.get(), 6);
    }
    #[test]
    fn final_drop_releases_unchanged_source_subscriptions() {
        let source = Property::new(1);
        for _ in 0..1000 {
            let source = source.clone();
            let effect = Effect::new(move || {
                source.get();
            });
            let other = effect.clone();
            drop(effect);
            assert_eq!(other.0.dependencies.borrow().len(), 1);
            drop(other);
        }
        assert!(source.signal.listeners.borrow().is_empty());
    }
    #[test]
    fn dependencies_can_change() {
        let use_a = Property::new(true);
        let a = Property::new(1);
        let b = Property::new(2);
        let output = Property::new(0);
        let _e = output.bind({
            let use_a = use_a.clone();
            let a = a.clone();
            let b = b.clone();
            move || if use_a.get() { a.get() } else { b.get() }
        });
        use_a.set(false);
        assert!(a.signal.listeners.borrow().is_empty());
        assert_eq!(b.signal.listeners.borrow().len(), 1);
        a.set(9);
        assert_eq!(output.get(), 2);
        b.set(7);
        assert_eq!(output.get(), 7);
    }
}
