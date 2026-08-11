//! Small, Vue-inspired reactive value primitives.
//!
//! [`Ref`] itself contains no GPUI state. Mutations accept a short-lived
//! [`ChangeNotifier`], and `gpui::Context` implements that trait directly.
//! This keeps a borrowed UI context out of application state while making
//! `count.update(|n| *n += 1, cx)` invalidate the GPUI entity represented by
//! the explicitly supplied context.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Receives notification after a [`Ref`] has actually changed.
///
/// The notification is always delivered after the value's mutable borrow has
/// been released, so an implementation may immediately schedule a render that
/// reads the value again.
pub trait ChangeNotifier {
    /// Marks the associated GPUI entity or external observer as changed.
    fn notify(&mut self);
}

impl<F> ChangeNotifier for F
where
    F: FnMut(),
{
    fn notify(&mut self) {
        self();
    }
}

/// A convenient no-op notifier for mutations that do not need to invalidate a
/// view.
impl ChangeNotifier for () {
    fn notify(&mut self) {}
}

impl<V: 'static> ChangeNotifier for gpui::Context<'_, V> {
    fn notify(&mut self) {
        gpui::Context::notify(self);
    }
}

/// A Vue-like reactive reference with shared clone semantics.
///
/// Cloning a `Ref<T>` clones its handle, not `T`; every clone observes the same
/// value. The implementation is intentionally single-threaded, matching the
/// way GPUI entities and their contexts are normally used.
///
/// This is an entity-local notifying cell, not an automatic dependency graph.
/// Clones share data, but a mutation only notifies the [`ChangeNotifier`] that
/// is passed to that mutation.
pub struct Ref<T> {
    /// Shared single-threaded storage used by every cloned handle.
    value: Rc<RefCell<T>>,
}

impl<T> Ref<T> {
    /// Creates a reactive reference.
    pub fn new(value: T) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
        }
    }

    /// Clones and returns the current value.
    #[must_use]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.borrow().clone()
    }

    /// Reads the current value without requiring `T: Clone`.
    ///
    /// The borrowed value must not escape the callback. Mutating this same
    /// `Ref` from inside `read` will panic, just like overlapping `RefCell`
    /// borrows normally do.
    pub fn read<R>(&self, reader: impl FnOnce(&T) -> R) -> R {
        reader(&self.value.borrow())
    }

    /// Replaces the value and notifies when it is different from the old one.
    ///
    /// Returns `true` when a replacement and notification occurred. Equality
    /// suppression is useful for render invalidation: assigning the same value
    /// is a no-op.
    pub fn set<N>(&self, next: T, notifier: &mut N) -> bool
    where
        T: PartialEq,
        N: ChangeNotifier + ?Sized,
    {
        let changed = {
            let mut current = self.value.borrow_mut();

            if current.eq(&next) {
                false
            } else {
                *current = next;
                true
            }
        };

        if changed {
            notifier.notify();
        }

        changed
    }

    /// Mutates the value in place and notifies when its final value changed.
    ///
    /// `T: Clone` is required so this method can compare the value before and
    /// after the callback. This makes the mutation O(size of `T`); prefer
    /// smaller refs for large state. Notification happens after releasing the
    /// mutable borrow.
    pub fn update<N>(&self, update: impl FnOnce(&mut T), notifier: &mut N) -> bool
    where
        T: Clone + PartialEq,
        N: ChangeNotifier + ?Sized,
    {
        let changed = {
            let mut current = self.value.borrow_mut();
            let previous = current.clone();
            update(&mut current);
            current.ne(&previous)
        };

        if changed {
            notifier.notify();
        }

        changed
    }

    /// Returns whether two handles point at the same reactive value.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.value, &other.value)
    }
}

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self {
            value: Rc::clone(&self.value),
        }
    }
}

impl<T> fmt::Debug for Ref<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Ref")
            .field(&self.value.borrow())
            .finish()
    }
}

/// Creates a Vue-like reactive [`Ref`].
///
/// The trailing underscore avoids Rust's `ref` keyword while keeping call
/// sites familiar: `let count = ref_(0);`.
pub fn ref_<T>(value: T) -> Ref<T> {
    Ref::new(value)
}

/// More explicit alias for [`ref_`].
pub fn reactive_ref<T>(value: T) -> Ref<T> {
    Ref::new(value)
}
