//! Allocation-free, component-local reactive state.
//!
//! [`Local`] is intended for state owned directly by a GPUI entity. Unlike a
//! shared [`crate::Ref`], it stores its value inline and exposes mutation only
//! through `&mut self`. A caller supplies a [`ChangeNotifier`] at mutation
//! time, which keeps the state independent from any particular GPUI context.

use std::fmt;

use crate::ChangeNotifier;

/// A lightweight version token for component-local state.
///
/// Revisions use wrapping arithmetic deliberately. Advancing
/// [`Revision::MAX`] produces [`Revision::ZERO`] without panicking in debug or
/// release builds, while every pair of adjacent revisions remains distinct.
/// A cache would need to miss more than `u64::MAX` consecutive changes to
/// observe the same token again.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Revision(
    /// The wrapping revision counter.
    u64,
);

impl Revision {
    /// The initial revision assigned to a new [`Local`].
    pub const ZERO: Self = Self(0);

    /// The greatest revision before the counter wraps to [`Revision::ZERO`].
    pub const MAX: Self = Self(u64::MAX);

    /// Creates a revision from its raw counter value.
    ///
    /// This is primarily useful for serialization, diagnostics, and testing
    /// revision-aware infrastructure.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw counter value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next revision, wrapping safely at [`Revision::MAX`].
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// Inline reactive state owned by one component or GPUI entity.
///
/// `Local<T>` performs no allocation and has no shared ownership or interior
/// mutability. Mutations therefore require `&mut self`, making aliasing rules
/// visible to Rust and avoiding the runtime borrow checks used by [`crate::Ref`].
pub struct Local<T> {
    /// The inline state value.
    value: T,
    /// The version changed after each effective mutation.
    revision: Revision,
}

impl<T> Local<T> {
    /// Creates local state at [`Revision::ZERO`].
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self {
            value,
            revision: Revision::ZERO,
        }
    }

    /// Clones and returns the current value.
    ///
    /// Prefer [`Local::as_ref`] or [`Local::read`] when cloning `T` would be
    /// unnecessary or expensive.
    #[must_use]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.clone()
    }

    /// Reads the current value through a callback without cloning it.
    pub fn read<R>(&self, reader: impl FnOnce(&T) -> R) -> R {
        reader(&self.value)
    }

    /// Borrows the current value.
    #[must_use]
    pub const fn as_ref(&self) -> &T {
        &self.value
    }

    /// Returns the current version token.
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// Consumes the state container and returns its value.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Replaces the value when it differs, then advances and notifies.
    ///
    /// The operation neither clones the old value nor allocates. It returns
    /// `true` exactly when `next` was installed. Notification happens after
    /// the value and revision have both been updated.
    pub fn set<N>(&mut self, next: T, notifier: &mut N) -> bool
    where
        T: PartialEq,
        N: ChangeNotifier + ?Sized,
    {
        if self.value == next {
            return false;
        }

        self.value = next;
        self.revision = self.revision.next();
        notifier.notify();
        true
    }

    /// Derives a candidate value and installs it only when it differs.
    ///
    /// The callback receives the current value by shared reference and returns
    /// the desired next value. This form permits exact equality suppression
    /// without cloning the previous value. Use `|count| count + 1` for scalar
    /// state and construct or move a replacement for owned collections.
    pub fn update<N>(&mut self, update: impl FnOnce(&T) -> T, notifier: &mut N) -> bool
    where
        T: PartialEq,
        N: ChangeNotifier + ?Sized,
    {
        let next = update(&self.value);
        self.set(next, notifier)
    }
}

impl<T> AsRef<T> for Local<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> Default for Local<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Local<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> fmt::Debug for Local<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Local")
            .field("value", &self.value)
            .field("revision", &self.revision)
            .finish()
    }
}

/// A lazily recomputed value keyed by typed dependency state.
///
/// Use a [`Revision`] for one [`Local`] dependency, or a tuple of revisions
/// when a computation depends on several values. `Memo<T, D>` owns only its
/// last dependency key and result; it does not allocate unless `T` or `D` do.
pub struct Memo<T, D = Revision> {
    /// The most recently computed dependency key and value.
    entry: Option<MemoEntry<T, D>>,
}

/// One populated memo cache entry.
struct MemoEntry<T, D> {
    /// The dependency key used to compute `value`.
    dependencies: D,
    /// The cached computed value.
    value: T,
}

impl<T, D> Memo<T, D> {
    /// Creates an empty memo cache.
    #[must_use]
    pub const fn new() -> Self {
        Self { entry: None }
    }

    /// Returns the cached value, if it has been computed.
    #[must_use]
    pub fn get(&self) -> Option<&T> {
        self.entry.as_ref().map(|entry| &entry.value)
    }

    /// Returns the cached dependency key, if it has been computed.
    #[must_use]
    pub fn dependencies(&self) -> Option<&D> {
        self.entry.as_ref().map(|entry| &entry.dependencies)
    }

    /// Drops the cached dependency key and value.
    pub fn invalidate(&mut self) {
        self.entry = None;
    }

    /// Returns the cached value or recomputes it for a changed dependency key.
    ///
    /// `compute` is called once on a cold cache and once after each unequal
    /// dependency key. A cache hit does not invoke the callback.
    ///
    /// # Panics
    ///
    /// Propagates a panic from `compute`. The cache remains unchanged if a
    /// recomputation panics.
    pub fn get_or_update(&mut self, dependencies: D, compute: impl FnOnce() -> T) -> &T
    where
        D: PartialEq,
    {
        let cache_hit = self
            .entry
            .as_ref()
            .is_some_and(|entry| entry.dependencies == dependencies);

        if !cache_hit {
            self.entry = Some(MemoEntry {
                dependencies,
                value: compute(),
            });
        }

        &self
            .entry
            .as_ref()
            .expect("memo entry is populated before it is returned")
            .value
    }
}

impl<T, D> Default for Memo<T, D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, D> fmt::Debug for Memo<T, D>
where
    T: fmt::Debug,
    D: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Memo")
            .field("entry", &self.entry)
            .finish()
    }
}

impl<T, D> fmt::Debug for MemoEntry<T, D>
where
    T: fmt::Debug,
    D: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoEntry")
            .field("dependencies", &self.dependencies)
            .field("value", &self.value)
            .finish()
    }
}
