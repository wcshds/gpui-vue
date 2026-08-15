//! Application-wide native state built on GPUI globals.
//!
//! This module is the shared-state boundary for values whose lifetime belongs
//! to the whole application. Component-local state should still use
//! [`crate::Local`], while independently retained models usually belong in a
//! native entity observed through [`crate::effects`].

use gpui::{App, Subscription};
pub use gpui::{Global, ReadGlobal, UpdateGlobal};

/// Stores or replaces one application-wide value.
pub fn provide_global<Value: Global>(app: &mut App, value: Value) {
    app.set_global(value);
}

/// Returns whether an application-wide value has been installed.
#[must_use]
pub fn has_global<Value: Global>(app: &App) -> bool {
    app.has_global::<Value>()
}

/// Reads an installed application-wide value.
///
/// # Panics
///
/// Panics when no value of `Value` has been provided.
#[must_use]
pub fn global<Value: Global>(app: &App) -> &Value {
    app.global::<Value>()
}

/// Tries to read an installed application-wide value.
#[must_use]
pub fn try_global<Value: Global>(app: &App) -> Option<&Value> {
    app.try_global::<Value>()
}

/// Mutably accesses an installed value and notifies its observers.
///
/// # Panics
///
/// Panics when no value of `Value` has been provided.
#[must_use]
pub fn global_mut<Value: Global>(app: &mut App) -> &mut Value {
    app.global_mut::<Value>()
}

/// Returns a global value, installing its default first when necessary.
#[must_use]
pub fn default_global<Value: Global + Default>(app: &mut App) -> &mut Value {
    app.default_global::<Value>()
}

/// Observes replacements and mutable access to one global value type.
///
/// GPUI schedules the callback after the current effect cycle. Retain the
/// returned subscription for as long as updates are wanted.
pub fn watch_global<Value: Global>(
    app: &mut App,
    callback: impl FnMut(&mut App) + 'static,
) -> Subscription {
    app.observe_global::<Value>(callback)
}

/// Removes and returns one installed application-wide value.
///
/// # Panics
///
/// Panics when no value of `Value` has been provided.
pub fn remove_global<Value: Global>(app: &mut App) -> Value {
    app.remove_global::<Value>()
}
