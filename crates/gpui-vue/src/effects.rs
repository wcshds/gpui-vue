//! Explicitly owned observers, event subscriptions, and scheduled effects.
//!
//! Native GPUI entities already provide notification and event streams. This
//! module gives those streams a small `gpui-vue` vocabulary without building a
//! second dependency graph or scheduler.

use gpui::{App, Context, Entity, EventEmitter, Subscription, Task, Window};

/// Async application context passed to work scheduled by [`spawn`].
///
/// The alias keeps component code on the `gpui-vue` surface while preserving
/// GPUI's fallible, weak application access across `.await` points.
pub type AsyncContext = gpui::AsyncApp;

/// Async application-and-window context passed to work scheduled by
/// [`spawn_in`].
///
/// Use this context when a task must update the originating window after an
/// `.await` point.
pub type AsyncWindowContext = gpui::AsyncWindowContext;

/// Weak handle to the entity that owns a scheduled effect.
///
/// A weak owner never keeps the component alive by itself. Both [`spawn`] and
/// [`spawn_in`] provide this handle instead of capturing a strong [`Entity`].
pub type WeakOwner<Owner> = gpui::WeakEntity<Owner>;

/// An owned collection of cancellable native subscriptions.
///
/// Dropping or clearing the scope drops every retained [`Subscription`] and
/// cancels its callback. Store a scope in component state when several
/// observers share the same lifetime.
#[derive(Debug, Default)]
pub struct EffectScope {
    /// Subscriptions retained by this scope.
    subscriptions: Vec<Subscription>,
}

impl EffectScope {
    /// Creates an empty effect scope.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            subscriptions: Vec::new(),
        }
    }

    /// Retains one subscription until the scope is cleared or dropped.
    pub fn track(&mut self, subscription: Subscription) {
        self.subscriptions.push(subscription);
    }

    /// Returns the number of retained subscriptions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.subscriptions.len()
    }

    /// Returns whether the scope currently owns no subscriptions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.subscriptions.is_empty()
    }

    /// Cancels every retained subscription immediately.
    pub fn clear(&mut self) {
        self.subscriptions.clear();
    }

    /// Detaches every retained subscription and consumes the scope.
    ///
    /// Detached callbacks continue until the entities involved are released.
    /// Prefer ordinary ownership unless the callback intentionally outlives
    /// the component that created it.
    pub fn detach(mut self) {
        for subscription in self.subscriptions.drain(..) {
            subscription.detach();
        }
    }
}

/// Spawns owner-safe asynchronous work on GPUI's foreground executor.
///
/// The callback receives a [`WeakOwner`] and an [`AsyncContext`]. Holding the
/// returned [`Task`] keeps the work alive; dropping it cancels the work. The
/// weak handle makes a late completion harmless after the owning component is
/// released.
///
/// Prefer storing the task in component state, or use
/// [`crate::async_state::AsyncResource`] when the work produces UI-facing
/// loading, ready, and error states.
#[track_caller]
pub fn spawn<Owner, AsyncFn, Output>(cx: &Context<'_, Owner>, operation: AsyncFn) -> Task<Output>
where
    Owner: 'static,
    AsyncFn: AsyncFnOnce(WeakOwner<Owner>, &mut AsyncContext) -> Output + 'static,
    Output: 'static,
{
    cx.spawn(operation)
}

/// Spawns owner-safe asynchronous work associated with a native window.
///
/// This is the window-aware counterpart of [`spawn`]. The callback receives a
/// [`WeakOwner`] and an [`AsyncWindowContext`], so it can safely update both
/// entity and window state after `.await` points. Dropping the returned
/// [`Task`] cancels the work.
#[track_caller]
pub fn spawn_in<Owner, AsyncFn, Output>(
    cx: &Context<'_, Owner>,
    window: &Window,
    operation: AsyncFn,
) -> Task<Output>
where
    Owner: 'static,
    AsyncFn: AsyncFnOnce(WeakOwner<Owner>, &mut AsyncWindowContext) -> Output + 'static,
    Output: 'static,
{
    cx.spawn_in(window, operation)
}

/// Observes notifications from another native entity.
///
/// The returned subscription must be retained for as long as updates are
/// wanted. Dropping it cancels the observer.
pub fn watch_entity<Owner, Watched>(
    cx: &mut Context<'_, Owner>,
    watched: &Entity<Watched>,
    on_change: impl FnMut(&mut Owner, Entity<Watched>, &mut Context<'_, Owner>) + 'static,
) -> Subscription
where
    Owner: 'static,
    Watched: 'static,
{
    cx.observe(watched, on_change)
}

/// Observes notifications from another entity with access to its window.
pub fn watch_entity_in<Owner, Watched>(
    cx: &mut Context<'_, Owner>,
    watched: &Entity<Watched>,
    window: &mut Window,
    on_change: impl FnMut(&mut Owner, Entity<Watched>, &mut Window, &mut Context<'_, Owner>) + 'static,
) -> Subscription
where
    Owner: 'static,
    Watched: 'static,
{
    cx.observe_in(watched, window, on_change)
}

/// Subscribes to typed events emitted by another native entity.
pub fn watch_event<Owner, Emitter, Event>(
    cx: &mut Context<'_, Owner>,
    emitter: &Entity<Emitter>,
    on_event: impl FnMut(&mut Owner, Entity<Emitter>, &Event, &mut Context<'_, Owner>) + 'static,
) -> Subscription
where
    Owner: 'static,
    Emitter: EventEmitter<Event> + 'static,
    Event: 'static,
{
    cx.subscribe(emitter, on_event)
}

/// Subscribes to typed events with access to the emitter's window.
pub fn watch_event_in<Owner, Emitter, Event>(
    cx: &mut Context<'_, Owner>,
    emitter: &Entity<Emitter>,
    window: &Window,
    on_event: impl FnMut(&mut Owner, &Entity<Emitter>, &Event, &mut Window, &mut Context<'_, Owner>)
    + 'static,
) -> Subscription
where
    Owner: 'static,
    Emitter: EventEmitter<Event>,
    Event: 'static,
{
    cx.subscribe_in(emitter, window, on_event)
}

/// Runs one component callback on the next native frame.
pub fn next_frame<Owner>(
    cx: &Context<'_, Owner>,
    window: &mut Window,
    callback: impl FnOnce(&mut Owner, &mut Window, &mut Context<'_, Owner>) + 'static,
) where
    Owner: 'static,
{
    cx.on_next_frame(window, callback);
}

/// Runs one component callback at the end of the current native effect cycle.
pub fn defer<Owner>(
    cx: &mut Context<'_, Owner>,
    window: &Window,
    callback: impl FnOnce(&mut Owner, &mut Window, &mut Context<'_, Owner>) + 'static,
) where
    Owner: 'static,
{
    cx.defer_in(window, callback);
}

/// Registers cleanup that runs when GPUI releases the owning entity.
pub fn on_release<Owner>(
    cx: &Context<'_, Owner>,
    cleanup: impl FnOnce(&mut Owner, &mut App) + 'static,
) -> Subscription
where
    Owner: 'static,
{
    cx.on_release(cleanup)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn scope_cancels_every_owned_subscription() {
        let cancellations = Rc::new(Cell::new(0));
        let mut scope = EffectScope::new();

        for _ in 0..3 {
            let cancellations = Rc::clone(&cancellations);
            scope.track(Subscription::new(move || {
                cancellations.set(cancellations.get() + 1);
            }));
        }

        assert_eq!(scope.len(), 3);
        scope.clear();
        assert!(scope.is_empty());
        assert_eq!(cancellations.get(), 3);
    }

    #[test]
    fn detached_scope_leaves_subscriptions_active() {
        let cancellations = Rc::new(Cell::new(0));
        let mut scope = EffectScope::new();
        let observed = Rc::clone(&cancellations);
        scope.track(Subscription::new(move || {
            observed.set(observed.get() + 1);
        }));

        scope.detach();
        assert_eq!(cancellations.get(), 0);
    }

    /// Compile-only fixture for the owner-safe foreground task wrapper.
    #[allow(dead_code, reason = "compile-time API fixture")]
    fn typed_spawn_fixture(cx: &Context<'_, AsyncOwner>) {
        let _task: Task<()> = spawn(cx, async |owner, async_cx| {
            owner.update(async_cx, |_owner, cx| cx.notify()).ok();
        });
    }

    /// Compile-only fixture for the window-aware owner-safe task wrapper.
    #[allow(dead_code, reason = "compile-time API fixture")]
    fn typed_spawn_in_fixture(cx: &Context<'_, AsyncOwner>, window: &Window) {
        let _task: Task<()> = spawn_in(cx, window, async |owner, async_cx| {
            owner
                .update_in(async_cx, |_owner, _window, cx| cx.notify())
                .ok();
        });
    }

    /// Owner used by the typed asynchronous wrapper fixtures.
    struct AsyncOwner;
}
