//! Explicit state for native asynchronous resources.
//!
//! GPUI owns task scheduling and cancellation through [`Task`]. This module
//! supplies UI-facing state machines and explicit resource ownership; it does
//! not introduce another executor.

use crate::effects::{AsyncContext, AsyncWindowContext, spawn, spawn_in};
pub use gpui::Task;
use gpui::{Context, Window};

/// Current state of an asynchronously produced value.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum AsyncState<Value, Error = String> {
    /// No operation has started yet.
    #[default]
    Idle,
    /// The current operation is in flight.
    Loading,
    /// The operation produced a value.
    Ready(Value),
    /// The operation failed.
    Error(Error),
}

impl<Value, Error> AsyncState<Value, Error> {
    /// Returns whether an operation is currently in flight.
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// Returns the ready value, if available.
    #[must_use]
    pub const fn value(&self) -> Option<&Value> {
        match self {
            Self::Ready(value) => Some(value),
            Self::Idle | Self::Loading | Self::Error(_) => None,
        }
    }

    /// Returns the current error, if available.
    #[must_use]
    pub const fn error(&self) -> Option<&Error> {
        match self {
            Self::Error(error) => Some(error),
            Self::Idle | Self::Loading | Self::Ready(_) => None,
        }
    }

    /// Maps a ready value while preserving idle, loading, and error states.
    pub fn map<Mapped>(self, map: impl FnOnce(Value) -> Mapped) -> AsyncState<Mapped, Error> {
        match self {
            Self::Idle => AsyncState::Idle,
            Self::Loading => AsyncState::Loading,
            Self::Ready(value) => AsyncState::Ready(map(value)),
            Self::Error(error) => AsyncState::Error(error),
        }
    }
}

/// One owner-held asynchronous resource with cancellation and stale-result
/// protection.
///
/// `AsyncResource` combines an [`AsyncState`] with the [`Task`] that produces
/// it. Starting a replacement request drops the previous task, and dropping
/// the resource cancels any task it still owns. Each request also receives a
/// monotonically increasing generation: a completion from an older request is
/// ignored even if it races with cancellation.
///
/// The resource must be stored in the same GPUI entity selected by the
/// `locate` callback passed to [`load`](Self::load) or
/// [`reload`](Self::reload). That callback is how the weak owner supplied by
/// GPUI finds the resource after an `.await` point.
///
/// ```no_run
/// use gpui_vue::{
///     async_state::AsyncResource,
///     ui::Context,
/// };
///
/// struct SearchView {
///     results: AsyncResource<Vec<String>>,
/// }
///
/// fn begin_initial_load(view: &mut SearchView, cx: &mut Context<'_, SearchView>) {
///     let started = view.results.load(
///         cx,
///         |owner| &mut owner.results,
///         async |_cx| Ok::<_, String>(vec!["永".to_owned()]),
///     );
///     assert!(started);
/// }
/// ```
#[derive(Debug)]
pub struct AsyncResource<Value, Error = String> {
    /// UI-visible state of the latest request.
    state: AsyncState<Value, Error>,
    /// Generation assigned to the latest request or invalidation.
    generation: u64,
    /// Native task whose drop cancels the currently owned request.
    task: Option<Task<()>>,
}

impl<Value, Error> AsyncResource<Value, Error> {
    /// Creates an idle resource without an owned task.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: AsyncState::Idle,
            generation: 0,
            task: None,
        }
    }

    /// Creates a resource that already contains a ready value.
    #[must_use]
    pub const fn ready(value: Value) -> Self {
        Self {
            state: AsyncState::Ready(value),
            generation: 0,
            task: None,
        }
    }

    /// Returns the complete UI-facing state machine.
    #[must_use]
    pub const fn state(&self) -> &AsyncState<Value, Error> {
        &self.state
    }

    /// Returns the ready value, if the latest request succeeded.
    #[must_use]
    pub const fn value(&self) -> Option<&Value> {
        self.state.value()
    }

    /// Returns the error, if the latest request failed.
    #[must_use]
    pub const fn error(&self) -> Option<&Error> {
        self.state.error()
    }

    /// Returns whether the latest request is still loading.
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        self.state.is_loading()
    }

    /// Returns the latest request or invalidation generation.
    ///
    /// Applications normally render [`state`](Self::state) instead. The
    /// generation is exposed for diagnostics and deterministic tests.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Starts the initial request when this resource is idle.
    ///
    /// Returns `false` without invoking `load` if the resource is already
    /// loading, ready, or failed. Use [`reload`](Self::reload) when an existing
    /// state should be replaced.
    ///
    /// `locate` must return this same resource from the owning entity. The
    /// asynchronous callback receives an [`AsyncContext`] and must return a
    /// `Result<Value, Error>`; its result is committed only while its request
    /// generation is still current.
    #[must_use]
    pub fn load<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        locate: Locate,
        load: Load,
    ) -> bool
    where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncContext) -> Result<Value, Error> + 'static,
    {
        if !matches!(self.state, AsyncState::Idle) {
            return false;
        }

        self.reload(cx, locate, load);
        true
    }

    /// Starts a request, cancelling and superseding any request already owned.
    ///
    /// The resource enters [`AsyncState::Loading`] synchronously. Completion
    /// updates the weakly owned entity and calls `notify`; if the entity has
    /// already been released, the result is simply discarded.
    pub fn reload<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        locate: Locate,
        load: Load,
    ) where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncContext) -> Result<Value, Error> + 'static,
    {
        let generation = self.begin_request();
        let task = spawn(cx, async move |owner, async_cx| {
            let result = load(async_cx).await;
            owner
                .update(async_cx, move |owner, cx| {
                    if locate(owner).finish_request(generation, result) {
                        cx.notify();
                    }
                })
                .ok();
        });
        self.task = Some(task);
        cx.notify();
    }

    /// Starts the initial request when idle, with access to the current window.
    ///
    /// This is the window-aware counterpart of [`load`](Self::load). It returns
    /// `false` without invoking `load` unless the state is [`AsyncState::Idle`].
    #[must_use]
    pub fn load_in<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        window: &Window,
        locate: Locate,
        load: Load,
    ) -> bool
    where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncWindowContext) -> Result<Value, Error> + 'static,
    {
        if !matches!(self.state, AsyncState::Idle) {
            return false;
        }

        self.reload_in(cx, window, locate, load);
        true
    }

    /// Starts a window-aware request, cancelling and superseding the previous
    /// request.
    ///
    /// Use this variant for native prompts or updates that require an
    /// [`AsyncWindowContext`]. A completion updates the owner in its originating
    /// window and notifies that owner only if its generation is still current.
    pub fn reload_in<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        window: &Window,
        locate: Locate,
        load: Load,
    ) where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncWindowContext) -> Result<Value, Error> + 'static,
    {
        let generation = self.begin_request();
        let task = spawn_in(cx, window, async move |owner, async_cx| {
            let result = load(async_cx).await;
            owner
                .update_in(async_cx, move |owner, _window, cx| {
                    if locate(owner).finish_request(generation, result) {
                        cx.notify();
                    }
                })
                .ok();
        });
        self.task = Some(task);
        cx.notify();
    }

    /// Cancels the owned request, returns the resource to idle, and notifies
    /// its owner.
    ///
    /// Returns whether the resource was loading. Cancellation always advances
    /// the generation, so an already-queued completion cannot overwrite the
    /// idle state.
    pub fn cancel<Owner>(&mut self, cx: &mut Context<'_, Owner>) -> bool
    where
        Owner: 'static,
    {
        let was_loading = self.cancel_request();
        cx.notify();
        was_loading
    }

    /// Cancels the current request and invalidates queued completion without
    /// requiring an entity context.
    fn cancel_request(&mut self) -> bool {
        let was_loading = self.is_loading();
        self.advance_generation();
        self.task = None;
        self.state = AsyncState::Idle;
        was_loading
    }

    /// Advances the request generation and prepares the loading state.
    fn begin_request(&mut self) -> u64 {
        self.advance_generation();
        self.task = None;
        self.state = AsyncState::Loading;
        self.generation
    }

    /// Advances the generation, failing loudly on an impossible lifetime-wide
    /// overflow instead of allowing an old request id to become current again.
    fn advance_generation(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("AsyncResource request generation overflowed");
    }

    /// Commits a request result only if its generation is still current.
    fn finish_request(&mut self, generation: u64, result: Result<Value, Error>) -> bool {
        if self.generation != generation || !self.is_loading() {
            return false;
        }

        self.state = match result {
            Ok(value) => AsyncState::Ready(value),
            Err(error) => AsyncState::Error(error),
        };
        true
    }
}

impl<Value, Error> Default for AsyncResource<Value, Error> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Value, Error> Drop for AsyncResource<Value, Error> {
    fn drop(&mut self) {
        // GPUI tasks cancel on drop. Taking explicitly documents that this
        // resource, rather than an executor-global detach, owns the request.
        self.task.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_only_ready_values() {
        assert_eq!(
            AsyncState::<usize>::Ready(2).map(|value| value * 3),
            AsyncState::Ready(6)
        );
        assert_eq!(
            AsyncState::<usize>::Error("offline".into()).map(|value| value * 3),
            AsyncState::Error("offline".into())
        );
    }

    #[test]
    fn resource_defaults_to_idle() {
        let resource = AsyncResource::<usize>::default();

        assert_eq!(resource.state(), &AsyncState::Idle);
        assert_eq!(resource.generation(), 0);
        assert!(!resource.is_loading());
        assert_eq!(resource.value(), None);
        assert_eq!(resource.error(), None);
    }

    #[test]
    fn newer_generation_rejects_stale_completion() {
        let mut resource = AsyncResource::<usize, &'static str>::new();
        let stale = resource.begin_request();
        let current = resource.begin_request();

        assert!(!resource.finish_request(stale, Ok(1)));
        assert_eq!(resource.state(), &AsyncState::Loading);
        assert!(resource.finish_request(current, Ok(2)));
        assert_eq!(resource.state(), &AsyncState::Ready(2));
    }

    #[test]
    fn current_error_is_committed() {
        let mut resource = AsyncResource::<usize, &'static str>::new();
        let generation = resource.begin_request();

        assert!(resource.finish_request(generation, Err("offline")));
        assert_eq!(resource.error(), Some(&"offline"));
    }

    #[test]
    fn cancel_drops_owned_task_and_invalidates_completion() {
        let mut resource = AsyncResource::<usize, &'static str>::new();
        let generation = resource.begin_request();
        resource.task = Some(Task::ready(()));

        assert!(resource.cancel_request());
        assert!(resource.task.is_none());
        assert_eq!(resource.state(), &AsyncState::Idle);
        assert!(!resource.finish_request(generation, Ok(1)));
    }

    #[test]
    fn ready_constructor_exposes_value_without_loading() {
        let resource = AsyncResource::<_, String>::ready(42);

        assert_eq!(resource.value(), Some(&42));
        assert!(!resource.is_loading());
    }

    /// Compile-only fixture proving that loaders and owner selectors remain
    /// fully typed without mentioning GPUI's crate path at call sites.
    #[allow(dead_code, reason = "compile-time API fixture")]
    fn typed_loader_fixture(owner: &mut TypedOwner, cx: &mut Context<'_, TypedOwner>) {
        let _ = owner.resource.load(
            cx,
            |owner| &mut owner.resource,
            async |_cx| Ok::<_, &'static str>(7),
        );
    }

    /// Owner used by [`typed_loader_fixture`].
    struct TypedOwner {
        /// Resource selected after the typed asynchronous operation completes.
        resource: AsyncResource<usize, &'static str>,
    }
}
