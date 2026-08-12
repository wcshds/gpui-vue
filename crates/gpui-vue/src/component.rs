//! Persistent native GPUI hosts for generated components.
//!
//! A [`ComponentElement`] is rebuilt with its parent element tree, while its
//! [`ComponentMount`] is retained by GPUI's keyed per-window element state.
//! [`ComponentEventElement`] adds one typed, render-varying event handler while
//! retaining one [`ComponentEventMount`] and one native GPUI subscription.
//! A disappearing slot drops its subscriptions even when another owner keeps
//! the component entity alive.

use gpui::{
    App, AppContext, AsyncApp, Bounds, Component, Context, Element, ElementId, Entity,
    EventEmitter, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Render,
    RenderOnce, Subscription, WeakEntity, Window,
};
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::{Rc, Weak},
};

/// Typestate marker for a required property that has not been supplied.
///
/// This zero-sized type is used only through `PhantomData` in generated props
/// builders and does not occupy runtime storage.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PropMissing;

/// Typestate marker for a required property that has been supplied.
///
/// This zero-sized type is used only through `PhantomData` in generated props
/// builders and does not occupy runtime storage.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PropSet;

/// Inline storage for one required property and its typestate.
///
/// Generated builders use this sealed representation so downstream code cannot
/// construct a `PropSet` value that lacks its property. The `Option<Value>` is
/// stored inline; `State` contributes no runtime size through [`PhantomData`].
#[doc(hidden)]
pub struct RequiredProp<Value, State> {
    /// Inline property storage, absent only in the missing typestate.
    value: Option<Value>,
    /// Zero-sized compile-time state.
    state: PhantomData<State>,
}

impl<Value> RequiredProp<Value, PropMissing> {
    /// Creates inline storage for a required property that is not yet supplied.
    #[doc(hidden)]
    #[must_use]
    pub const fn missing() -> Self {
        Self {
            value: None,
            state: PhantomData,
        }
    }
}

impl<Value, State> RequiredProp<Value, State> {
    /// Stores a value and transitions this property to [`PropSet`].
    #[doc(hidden)]
    #[must_use]
    pub fn set(mut self, value: Value) -> RequiredProp<Value, PropSet> {
        self.value = Some(value);
        RequiredProp {
            value: self.value,
            state: PhantomData,
        }
    }
}

impl<Value> RequiredProp<Value, PropSet> {
    /// Extracts a property whose typestate proves that it was supplied.
    ///
    /// # Panics
    ///
    /// This cannot panic through safe code because the representation is sealed
    /// and only [`RequiredProp::set`] can construct the [`PropSet`] state.
    #[doc(hidden)]
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
            .expect("PropSet is constructed only after storing a required property")
    }
}

/// A generated stateful component that can be mounted by [`ComponentElement`].
///
/// Implementations receive a complete input value on construction and on each
/// subsequent parent render. The input may contain non-comparable values such
/// as slot closures; generated implementations compare ordinary props
/// separately and only notify GPUI when those comparable props change.
pub trait NativeComponent: Render + Sized {
    /// Comparable property value accepted by generated props builders.
    type Props: 'static;

    /// Complete input accepted by this component's persistent host.
    type Input: 'static;

    /// Visual-host state retained while this component identity is mounted.
    ///
    /// Generated components without lifecycle hooks use `()`, which occupies
    /// no storage and performs no registration. Hook-bearing components use a
    /// statically dispatched [`ComponentLifecycleMount`].
    type MountState: NativeComponentMount<Self>;

    /// Attaches visual-host state to a newly constructed component entity.
    ///
    /// The default implementation delegates to the associated mount-state
    /// type. Implementations normally only select [`Self::MountState`].
    fn attach_mount(entity: &Entity<Self>, cx: &mut App) -> Self::MountState {
        Self::MountState::attach(entity, cx)
    }

    /// Constructs the component state for a newly mounted host slot.
    fn construct(input: Self::Input, cx: &mut Context<Self>) -> Self;

    /// Reconciles one new input value into an existing component entity.
    ///
    /// The return value reports whether this frame's input may affect rendered
    /// output. Generated comparable-only inputs return an exact props result;
    /// inputs with opaque slot providers conservatively return `true` whenever
    /// reconciled. Slot-only replacement does not issue an extra notification,
    /// because the persistent host renders the child later in the same frame.
    fn reconcile_input(&mut self, input: Self::Input, cx: &mut Context<Self>) -> bool;
}

/// Lifecycle behavior retained by a native component's visual host.
///
/// This trait is public so `component!` expansions in downstream crates can
/// name it, but it is not intended for direct application use. All operations
/// are monomorphized; no lifecycle callback is stored behind `dyn Fn`.
#[doc(hidden)]
pub trait NativeComponentMount<ComponentType>: 'static
where
    ComponentType: NativeComponent,
{
    /// Per-frame token copied into the transparent hosted element.
    type RenderToken: 'static;

    /// Attaches state exactly once for a newly observed visual identity.
    fn attach(entity: &Entity<ComponentType>, cx: &mut App) -> Self;

    /// Marks a comparable input change before the component renders.
    fn input_changed(&self);

    /// Produces the lightweight token used by [`HostedEntity`].
    fn render_token(&self) -> Self::RenderToken;

    /// Runs after the hosted entity has delegated layout to its rendered tree.
    fn after_render(
        token: &Self::RenderToken,
        entity: &Entity<ComponentType>,
        window: &mut Window,
        cx: &mut App,
    );
}

impl<ComponentType> NativeComponentMount<ComponentType> for ()
where
    ComponentType: NativeComponent<MountState = ()>,
{
    type RenderToken = ();

    fn attach(_entity: &Entity<ComponentType>, _cx: &mut App) -> Self {}

    fn input_changed(&self) {}

    fn render_token(&self) -> Self::RenderToken {}

    fn after_render(
        _token: &Self::RenderToken,
        _entity: &Entity<ComponentType>,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

/// Statically dispatched lifecycle hook bodies generated by `component!`.
///
/// Missing hooks use these empty defaults. A component only implements this
/// trait when at least one lifecycle section was declared, so components with
/// no hooks do not allocate or register lifecycle state.
#[doc(hidden)]
pub trait ComponentLifecycleHooks: NativeComponent {
    /// Whether a first-render `mounted` callback must be queued.
    const HAS_MOUNTED: bool = false;

    /// Whether notifications must be observed for an `updated` hook.
    const TRACK_UPDATES: bool = false;

    /// Whether visual teardown and the entity-release fallback are required.
    const HAS_UNMOUNTED: bool = false;

    /// Runs after the component's first delegated draw returns, at the end of
    /// the current GPUI effect cycle.
    fn mounted(_component: &mut Self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Runs after a later dirty delegated draw returns, at the end of the
    /// current GPUI effect cycle.
    fn updated(_component: &mut Self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Runs once after the visual host state disappears.
    fn unmounted(_component: &mut Self, _cx: &mut App) {}
}

/// Retained state for a component with compile-time lifecycle hooks.
///
/// The host owns this value independently of the component entity. Its drop
/// therefore represents disappearance of the keyed visual identity even when
/// another owner keeps an [`Entity`] clone alive.
#[doc(hidden)]
pub struct ComponentLifecycleMount<ComponentType>
where
    ComponentType: ComponentLifecycleHooks,
{
    /// Notification observer, cancelled before visual teardown is queued.
    observer: Option<Subscription>,
    /// Weak entity identity used to avoid extending its mounted lifetime.
    entity: Option<WeakEntity<ComponentType>>,
    /// Application handle used to queue teardown outside element-state drop.
    app: Option<AsyncApp>,
    /// Liveness, phase, and dirty state shared with per-render weak tokens.
    signals: Rc<LifecycleSignals>,
    /// Component type identity without runtime storage.
    component: PhantomData<fn() -> ComponentType>,
}

/// Per-render weak token for a lifecycle-enabled hosted entity.
///
/// Pending post-commit callbacks also retain only weak state. Dropping the
/// visual mount invalidates them immediately, before queued unmount work runs.
#[doc(hidden)]
pub struct LifecycleRenderToken<ComponentType>
where
    ComponentType: ComponentLifecycleHooks,
{
    /// Weak visual-host liveness and scheduling state.
    signals: Weak<LifecycleSignals>,
    /// Component type identity without runtime storage.
    component: PhantomData<fn() -> ComponentType>,
}

/// Mutable lifecycle phase shared between one mount and its commit callbacks.
struct LifecycleSignals {
    /// Current visual scheduling phase.
    phase: Cell<LifecyclePhase>,
    /// Whether a state notification or comparable input change occurred.
    dirty: Cell<bool>,
    /// Whether the entity completed at least one delegated layout.
    eligible: Cell<bool>,
    /// Whether the unmounted hook has already executed.
    done: Cell<bool>,
}

/// Scheduling phases for one visual host identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecyclePhase {
    /// No delegated layout has completed yet.
    ActiveUnrendered,
    /// The first-render mounted callback is queued.
    MountQueued,
    /// The mounted callback completed and no update callback is queued.
    ActiveRendered,
    /// A coalesced updated callback is queued.
    UpdateQueued,
    /// Visual state disappeared and pending commit callbacks are stale.
    UnmountQueued,
}

/// One generated hook to run after the current draw has been committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleCallback {
    /// The visual identity's first-render hook.
    Mounted,
    /// A coalesced dirty-render hook.
    Updated,
}

impl LifecycleSignals {
    /// Creates scheduling state for a newly attached visual identity.
    const fn new() -> Self {
        Self {
            phase: Cell::new(LifecyclePhase::ActiveUnrendered),
            dirty: Cell::new(false),
            eligible: Cell::new(false),
            done: Cell::new(false),
        }
    }

    /// Records a component notification.
    fn notified(&self) {
        self.dirty.set(true);
    }

    /// Records a comparable host input change.
    fn input_changed(&self) {
        self.dirty.set(true);
    }

    /// Records one completed render and selects any post-commit callback.
    ///
    /// A callback that is already queued covers every later render completed
    /// before it runs. Dirty state incorporated by such a render is therefore
    /// consumed here instead of leaking into a future unrelated render.
    fn rendered(&self, has_mounted: bool) -> Option<LifecycleCallback> {
        self.eligible.set(true);

        match self.phase.get() {
            LifecyclePhase::ActiveUnrendered => {
                self.dirty.set(false);
                if has_mounted {
                    self.phase.set(LifecyclePhase::MountQueued);
                    Some(LifecycleCallback::Mounted)
                } else {
                    self.phase.set(LifecyclePhase::ActiveRendered);
                    None
                }
            }
            LifecyclePhase::ActiveRendered if self.dirty.replace(false) => {
                self.phase.set(LifecyclePhase::UpdateQueued);
                Some(LifecycleCallback::Updated)
            }
            LifecyclePhase::MountQueued | LifecyclePhase::UpdateQueued => {
                self.dirty.set(false);
                None
            }
            LifecyclePhase::ActiveRendered | LifecyclePhase::UnmountQueued => None,
        }
    }

    /// Completes a queued callback unless visual teardown invalidated it.
    fn complete(&self, callback: LifecycleCallback) -> bool {
        let expected = match callback {
            LifecycleCallback::Mounted => LifecyclePhase::MountQueued,
            LifecycleCallback::Updated => LifecyclePhase::UpdateQueued,
        };
        if self.phase.get() != expected {
            return false;
        }
        self.phase.set(LifecyclePhase::ActiveRendered);
        true
    }

    /// Invalidates pending commit callbacks for a disappearing visual host.
    fn queue_unmount(&self) {
        self.phase.set(LifecyclePhase::UnmountQueued);
    }

    /// Claims the eligible unmounted hook exactly once.
    fn claim_unmount(&self) -> bool {
        self.eligible.get() && !self.done.replace(true)
    }

    /// Runs a generated unmounted hook at most once after visual eligibility.
    fn run<ComponentType>(&self, component: &mut ComponentType, cx: &mut App)
    where
        ComponentType: ComponentLifecycleHooks,
    {
        if self.claim_unmount() {
            ComponentType::unmounted(component, cx);
        }
    }
}

impl<ComponentType> NativeComponentMount<ComponentType> for ComponentLifecycleMount<ComponentType>
where
    ComponentType: ComponentLifecycleHooks<MountState = Self>,
{
    type RenderToken = LifecycleRenderToken<ComponentType>;

    fn attach(entity: &Entity<ComponentType>, cx: &mut App) -> Self {
        let signals = Rc::new(LifecycleSignals::new());

        let observer = if ComponentType::TRACK_UPDATES {
            let weak_signals = Rc::downgrade(&signals);
            Some(entity.update(cx, |_component, component_cx| {
                component_cx.observe_self(move |_component, _component_cx| {
                    if let Some(signals) = weak_signals.upgrade() {
                        signals.notified();
                    }
                })
            }))
        } else {
            None
        };

        if ComponentType::HAS_UNMOUNTED {
            let signals_on_release = Rc::clone(&signals);
            entity
                .update(cx, |_component, component_cx| {
                    component_cx.on_release(move |component, cx| {
                        signals_on_release.run(component, cx);
                    })
                })
                .detach();
        }

        Self {
            observer,
            entity: ComponentType::HAS_UNMOUNTED.then(|| entity.downgrade()),
            app: ComponentType::HAS_UNMOUNTED.then(|| cx.to_async()),
            signals,
            component: PhantomData,
        }
    }

    fn input_changed(&self) {
        if ComponentType::TRACK_UPDATES {
            self.signals.input_changed();
        }
    }

    fn render_token(&self) -> Self::RenderToken {
        LifecycleRenderToken {
            signals: Rc::downgrade(&self.signals),
            component: PhantomData,
        }
    }

    fn after_render(
        token: &Self::RenderToken,
        entity: &Entity<ComponentType>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(signals) = token.signals.upgrade() else {
            return;
        };

        // Descendant layout completes before this delegated layout returns, so
        // GPUI's FIFO defer effects preserve child-before-parent hook order.
        match signals.rendered(ComponentType::HAS_MOUNTED) {
            Some(LifecycleCallback::Mounted) => {
                let weak_signals = Rc::downgrade(&signals);
                let weak_entity = entity.downgrade();
                window.defer(cx, move |window, cx| {
                    let Some(signals) = weak_signals.upgrade() else {
                        return;
                    };
                    if !signals.complete(LifecycleCallback::Mounted) {
                        return;
                    }
                    let _ = weak_entity.update(cx, |component, component_cx| {
                        ComponentType::mounted(component, window, component_cx);
                    });
                });
            }
            Some(LifecycleCallback::Updated) => {
                let weak_signals = Rc::downgrade(&signals);
                let weak_entity = entity.downgrade();
                window.defer(cx, move |window, cx| {
                    let Some(signals) = weak_signals.upgrade() else {
                        return;
                    };
                    if !signals.complete(LifecycleCallback::Updated) {
                        return;
                    }
                    let _ = weak_entity.update(cx, |component, component_cx| {
                        ComponentType::updated(component, window, component_cx);
                    });
                });
            }
            None => {}
        }
    }
}

impl<ComponentType> Drop for ComponentLifecycleMount<ComponentType>
where
    ComponentType: ComponentLifecycleHooks,
{
    fn drop(&mut self) {
        drop(self.observer.take());
        self.signals.queue_unmount();

        if !ComponentType::HAS_UNMOUNTED || !self.signals.eligible.get() || self.signals.done.get()
        {
            return;
        }
        // Keep this weak: the enclosing mount drops its strong entity field
        // next, allowing GPUI's release listener to run during shutdown flush.
        let Some(entity) = self.entity.take() else {
            return;
        };
        let signals = Rc::clone(&self.signals);
        let app = self
            .app
            .take()
            .expect("unmounted hooks retain an async app");
        let executor = app.foreground_executor().clone();
        executor
            .spawn(async move {
                let _ = app.update(|cx| {
                    let _ = entity.update(cx, |component, component_cx| {
                        signals.run(component, &mut *component_cx);
                    });
                });
            })
            .detach();
    }
}

/// A generated native component that accepts typed lazy slots.
///
/// The view macro uses this associated-type surface instead of reconstructing
/// generated item names. Consequently a component may be imported through a
/// Rust alias or module path without affecting slot lowering.
pub trait NativeComponentSlots: NativeComponent {
    /// Complete generated slot collection accepted by this component.
    type Slots: Default + 'static;

    /// Returns the component's current reconciled typed slots.
    ///
    /// Child-side template outlets use this associated-type accessor so their
    /// lowering remains valid across Rust module paths and component aliases.
    fn slots(&self) -> &Self::Slots;

    /// Combines comparable props with one typed slot collection.
    fn input_with_slots(props: Self::Props, slots: Self::Slots) -> Self::Input;
}

/// A native component that exposes one generated, typed event enum.
///
/// Components generated from a `component!` declaration implement this trait
/// exactly when they contain a non-empty `emits` section. Template hosts use
/// the associated type to stay hygienic across module paths and component
/// aliases without guessing the generated event enum's name.
pub trait NativeComponentEvents: NativeComponent {
    /// Complete event enum emitted through this component's GPUI event channel.
    type Event: 'static;
}

/// State retained for one component identity across consecutive GPUI frames.
///
/// The fixed-size subscription array avoids a heap allocation. Dropping this
/// value cancels every subscription before releasing its strong entity handle.
pub struct ComponentMount<ComponentType, const SUBSCRIPTIONS: usize, Lifecycle = ()> {
    /// Mount-scoped subscriptions whose `Drop` implementations unsubscribe.
    _subscriptions: [Subscription; SUBSCRIPTIONS],
    /// Visual lifecycle state, dropped before the strong entity handle.
    lifecycle: Lifecycle,
    /// Strong component identity retained between frames.
    entity: Entity<ComponentType>,
}

/// State retained for one component and one render-varying event handler.
///
/// The handler stays monomorphic as `Handler`; this host does not allocate a
/// trait object or perform a string lookup. The field order is intentional:
/// dropping a mount first cancels the GPUI subscription, then releases the
/// shared handler cell, and only then releases the strong component entity.
pub struct ComponentEventMount<ComponentType, Handler, Lifecycle = ()> {
    /// Native GPUI subscription, dropped before the captured handler cell.
    _subscription: Subscription,
    /// Latest parent-provided handler shared with the subscription callback.
    handler: Rc<RefCell<Handler>>,
    /// Visual lifecycle state, dropped before the strong entity handle.
    lifecycle: Lifecycle,
    /// Strong component identity retained between frames.
    entity: Entity<ComponentType>,
}

/// Internal element state that includes the subscription factory in its type.
///
/// GPUI keys element state by `TypeId` as well as the global element ID. The
/// zero-sized marker prevents two macro-generated component sites with the same
/// explicit slot text but different closure types from sharing retained state.
struct ComponentMountState<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>
where
    ComponentType: NativeComponent,
{
    /// Public mount payload with the entity and fixed subscription storage.
    mount: ComponentMount<ComponentType, SUBSCRIPTIONS, ComponentType::MountState>,
    /// Factory-type identity without storing or dropping another factory.
    subscribe: PhantomData<fn() -> Subscribe>,
}

impl<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>
    ComponentMountState<ComponentType, Subscribe, SUBSCRIPTIONS>
where
    ComponentType: NativeComponent,
{
    /// Wraps a public mount with its compile-site factory type identity.
    const fn new(
        mount: ComponentMount<ComponentType, SUBSCRIPTIONS, ComponentType::MountState>,
    ) -> Self {
        Self {
            mount,
            subscribe: PhantomData,
        }
    }
}

/// Internal element state that includes the subscribed event in its type.
struct ComponentEventMountState<ComponentType, Event, Handler>
where
    ComponentType: NativeComponent,
{
    /// Public event-aware mount payload.
    mount: ComponentEventMount<ComponentType, Handler, ComponentType::MountState>,
    /// Event-type identity without runtime storage.
    event: PhantomData<fn(&Event)>,
}

impl<ComponentType, Event, Handler> ComponentEventMountState<ComponentType, Event, Handler>
where
    ComponentType: NativeComponent,
{
    /// Wraps an event-aware mount with its concrete event type identity.
    const fn new(
        mount: ComponentEventMount<ComponentType, Handler, ComponentType::MountState>,
    ) -> Self {
        Self {
            mount,
            event: PhantomData,
        }
    }
}

impl<ComponentType, Handler, Lifecycle> ComponentEventMount<ComponentType, Handler, Lifecycle> {
    /// Creates retained state for one typed event-aware component mount.
    fn new(
        entity: Entity<ComponentType>,
        subscription: Subscription,
        handler: Rc<RefCell<Handler>>,
        lifecycle: Lifecycle,
    ) -> Self {
        Self {
            _subscription: subscription,
            handler,
            lifecycle,
            entity,
        }
    }

    /// Replaces the callback observed by the already-active subscription.
    fn replace_handler(&mut self, handler: Handler) {
        *self.handler.borrow_mut() = handler;
    }

    /// Returns another strong handle to the mounted component entity.
    #[must_use]
    pub fn entity(&self) -> Entity<ComponentType> {
        self.entity.clone()
    }
}

impl<ComponentType, const SUBSCRIPTIONS: usize> ComponentMount<ComponentType, SUBSCRIPTIONS> {
    /// Creates retained state for one mounted component entity.
    #[must_use]
    pub const fn new(
        entity: Entity<ComponentType>,
        subscriptions: [Subscription; SUBSCRIPTIONS],
    ) -> Self {
        Self {
            _subscriptions: subscriptions,
            lifecycle: (),
            entity,
        }
    }

    /// Returns another strong handle to the mounted component entity.
    #[must_use]
    pub fn entity(&self) -> Entity<ComponentType> {
        self.entity.clone()
    }
}

impl<ComponentType, const SUBSCRIPTIONS: usize, Lifecycle>
    ComponentMount<ComponentType, SUBSCRIPTIONS, Lifecycle>
{
    /// Creates retained component state with an attached visual lifecycle.
    #[doc(hidden)]
    const fn new_with_lifecycle(
        entity: Entity<ComponentType>,
        subscriptions: [Subscription; SUBSCRIPTIONS],
        lifecycle: Lifecycle,
    ) -> Self {
        Self {
            _subscriptions: subscriptions,
            lifecycle,
            entity,
        }
    }

    /// Returns another strong handle to the mounted component entity.
    fn mounted_entity(&self) -> Entity<ComponentType> {
        self.entity.clone()
    }
}

/// A transparent element adapter that preserves visual lifecycle ordering.
///
/// Layout, prepaint, and paint are delegated to the native component entity
/// without adding a layout node. The associated render token is zero-sized for
/// components without hooks. For hook-bearing components it only contains a
/// weak visual-mount guard.
#[doc(hidden)]
pub struct HostedEntity<ComponentType>
where
    ComponentType: NativeComponent,
{
    /// Native entity whose element implementation is delegated verbatim.
    entity: Entity<ComponentType>,
    /// Per-render lifecycle token selected by the component's mount-state type.
    lifecycle: <ComponentType::MountState as NativeComponentMount<ComponentType>>::RenderToken,
}

impl<ComponentType> HostedEntity<ComponentType>
where
    ComponentType: NativeComponent,
{
    /// Creates a transparent hosted entity from one retained mount.
    fn new(
        entity: Entity<ComponentType>,
        lifecycle: <ComponentType::MountState as NativeComponentMount<ComponentType>>::RenderToken,
    ) -> Self {
        Self { entity, lifecycle }
    }
}

impl<ComponentType> Element for HostedEntity<ComponentType>
where
    ComponentType: NativeComponent,
{
    type RequestLayoutState = <Entity<ComponentType> as Element>::RequestLayoutState;
    type PrepaintState = <Entity<ComponentType> as Element>::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.entity.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.entity.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout = self.entity.request_layout(id, inspector_id, window, cx);
        ComponentType::MountState::after_render(&self.lifecycle, &self.entity, window, cx);
        layout
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.entity
            .prepaint(id, inspector_id, bounds, request_layout, window, cx);
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.entity.paint(
            id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );
    }
}

impl<ComponentType> IntoElement for HostedEntity<ComponentType>
where
    ComponentType: NativeComponent,
{
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// A one-frame element recipe backed by a persistent keyed component mount.
///
/// `Subscribe` is a mount-only factory. Its body is invoked exactly once for a
/// newly observed `(slot, key)` identity and is not invoked while reconciling
/// later frames. The view layer should therefore build handler expressions
/// inside that factory body when their construction must also be mount-only.
///
/// This type delegates layout and painting directly to the returned component
/// [`Entity`]; it does not introduce a `div` or another layout node.
pub struct ComponentElement<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>
where
    ComponentType: NativeComponent,
{
    /// Compile-site identity that distinguishes sibling component positions.
    slot: ElementId,
    /// Optional user key nested below the compile-site identity.
    key: Option<ElementId>,
    /// Input constructed by the parent for this frame.
    input: ComponentType::Input,
    /// Factory used only when the keyed mount is first created.
    subscribe: Subscribe,
}

impl<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>
    ComponentElement<ComponentType, Subscribe, SUBSCRIPTIONS>
where
    ComponentType: NativeComponent,
    Subscribe: FnOnce(&Entity<ComponentType>, &mut Window, &mut App) -> [Subscription; SUBSCRIPTIONS]
        + 'static,
{
    /// Creates a one-frame recipe for a persistent component host.
    #[must_use]
    pub const fn new(
        slot: ElementId,
        key: Option<ElementId>,
        input: ComponentType::Input,
        subscribe: Subscribe,
    ) -> Self {
        Self {
            slot,
            key,
            input,
            subscribe,
        }
    }
}

impl<ComponentType, Subscribe, const SUBSCRIPTIONS: usize> RenderOnce
    for ComponentElement<ComponentType, Subscribe, SUBSCRIPTIONS>
where
    ComponentType: NativeComponent,
    Subscribe: FnOnce(&Entity<ComponentType>, &mut Window, &mut App) -> [Subscription; SUBSCRIPTIONS]
        + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        mount_component(self.slot, self.key, self.input, self.subscribe, window, cx)
    }
}

impl<ComponentType, Subscribe, const SUBSCRIPTIONS: usize> IntoElement
    for ComponentElement<ComponentType, Subscribe, SUBSCRIPTIONS>
where
    ComponentType: NativeComponent,
    Subscribe: FnOnce(&Entity<ComponentType>, &mut Window, &mut App) -> [Subscription; SUBSCRIPTIONS]
        + 'static,
{
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

/// A one-frame element recipe with one persistent, typed event subscription.
///
/// The handler is stored inline in this transient recipe. On the first frame
/// for a `(slot, key)` identity, the host creates one `Rc<RefCell<Handler>>`
/// and subscribes once through [`Window::subscribe`]. Consecutive frames move
/// the latest handler into that cell before reconciling component input, so an
/// event emitted by the child observes the latest parent captures without a
/// new subscription.
///
/// Like [`ComponentElement`], this delegates layout and painting directly to
/// the returned component [`Entity`] and adds no layout node.
pub struct ComponentEventElement<ComponentType, Event, Handler>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
{
    /// Compile-site identity that distinguishes sibling component positions.
    slot: ElementId,
    /// Optional user key nested below the compile-site identity.
    key: Option<ElementId>,
    /// Input constructed by the parent for this frame.
    input: ComponentType::Input,
    /// Latest typed callback supplied by the parent.
    handler: Handler,
    /// Event type consumed by `handler`, without runtime storage.
    event: PhantomData<fn(&Event)>,
}

impl<ComponentType, Event, Handler> ComponentEventElement<ComponentType, Event, Handler>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
    Handler: FnMut(&Event, &mut Window, &mut App) + 'static,
{
    /// Creates a one-frame recipe for an event-aware persistent component host.
    #[must_use]
    pub const fn new(
        slot: ElementId,
        key: Option<ElementId>,
        input: ComponentType::Input,
        handler: Handler,
    ) -> Self {
        Self {
            slot,
            key,
            input,
            handler,
            event: PhantomData,
        }
    }
}

impl<ComponentType, Event, Handler> RenderOnce
    for ComponentEventElement<ComponentType, Event, Handler>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
    Handler: FnMut(&Event, &mut Window, &mut App) + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        mount_component_with_events::<ComponentType, Event, Handler>(
            self.slot,
            self.key,
            self.input,
            self.handler,
            window,
            cx,
        )
    }
}

impl<ComponentType, Event, Handler> IntoElement
    for ComponentEventElement<ComponentType, Event, Handler>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
    Handler: FnMut(&Event, &mut Window, &mut App) + 'static,
{
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

/// Creates a native element recipe for one persistent component slot.
///
/// `slot` must be stable for the source position. An optional user `key` is
/// nested below it, so equal keys at different source positions cannot alias.
/// The subscription factory is mount-only; its returned handles remain active
/// exactly as long as this keyed slot remains present in consecutive frames.
#[must_use]
pub const fn component_element<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>(
    slot: ElementId,
    key: Option<ElementId>,
    input: ComponentType::Input,
    subscribe: Subscribe,
) -> ComponentElement<ComponentType, Subscribe, SUBSCRIPTIONS>
where
    ComponentType: NativeComponent,
    Subscribe: FnOnce(&Entity<ComponentType>, &mut Window, &mut App) -> [Subscription; SUBSCRIPTIONS]
        + 'static,
{
    ComponentElement::new(slot, key, input, subscribe)
}

/// Creates an event-aware recipe for one persistent component slot.
///
/// `slot` must be stable for the source position. An optional user `key` is
/// nested below it. The first frame for that identity allocates one shared
/// handler cell and calls [`Window::subscribe`] once; later frames replace the
/// concrete `Handler` value before input reconciliation. The type remains
/// fully monomorphic and performs no per-frame allocation or resubscription.
#[must_use]
pub const fn component_element_with_events<ComponentType, Event, Handler>(
    slot: ElementId,
    key: Option<ElementId>,
    input: ComponentType::Input,
    handler: Handler,
) -> ComponentEventElement<ComponentType, Event, Handler>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
    Handler: FnMut(&Event, &mut Window, &mut App) + 'static,
{
    ComponentEventElement::new(slot, key, input, handler)
}

/// Constructs a native component from the frame input exactly once.
fn construct_component<ComponentType>(
    input: &mut Option<ComponentType::Input>,
    cx: &mut App,
) -> Entity<ComponentType>
where
    ComponentType: NativeComponent,
{
    AppContext::new(cx, |component_cx| {
        ComponentType::construct(
            input.take().expect("component input is consumed once"),
            component_cx,
        )
    })
}

/// Applies the frame input to an already-mounted native component.
fn reconcile_component<ComponentType>(
    entity: &Entity<ComponentType>,
    input: &mut Option<ComponentType::Input>,
    cx: &mut App,
) -> bool
where
    ComponentType: NativeComponent,
{
    entity.update(cx, |component, component_cx| {
        component.reconcile_input(
            input.take().expect("component input is consumed once"),
            component_cx,
        )
    })
}

/// Mounts or reconciles a component at one fully qualified element identity.
fn mount_at_id<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>(
    global_id: &GlobalElementId,
    input: &mut Option<ComponentType::Input>,
    subscribe: &mut Option<Subscribe>,
    window: &mut Window,
    cx: &mut App,
) -> HostedEntity<ComponentType>
where
    ComponentType: NativeComponent,
    Subscribe: FnOnce(&Entity<ComponentType>, &mut Window, &mut App) -> [Subscription; SUBSCRIPTIONS]
        + 'static,
{
    window.with_element_state::<ComponentMountState<ComponentType, Subscribe, SUBSCRIPTIONS>, _>(
        global_id,
        |state, window| {
            let Some(state) = state else {
                let entity = construct_component::<ComponentType>(input, cx);
                let lifecycle = ComponentType::attach_mount(&entity, cx);
                let render_token = lifecycle.render_token();
                let subscriptions = subscribe
                    .take()
                    .expect("component subscription factory runs once")(
                    &entity, window, cx
                );
                let mount =
                    ComponentMount::new_with_lifecycle(entity.clone(), subscriptions, lifecycle);
                return (
                    HostedEntity::new(entity, render_token),
                    ComponentMountState::new(mount),
                );
            };

            let entity = state.mount.mounted_entity();
            if reconcile_component(&entity, input, cx) {
                state.mount.lifecycle.input_changed();
            }
            let hosted = HostedEntity::new(entity, state.mount.lifecycle.render_token());
            (hosted, state)
        },
    )
}

/// Mounts or reconciles one event-aware component at a qualified identity.
fn mount_event_at_id<ComponentType, Event, Handler>(
    global_id: &GlobalElementId,
    input: &mut Option<ComponentType::Input>,
    handler: &mut Option<Handler>,
    window: &mut Window,
    cx: &mut App,
) -> HostedEntity<ComponentType>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
    Handler: FnMut(&Event, &mut Window, &mut App) + 'static,
{
    window.with_element_state::<ComponentEventMountState<ComponentType, Event, Handler>, _>(
        global_id,
        |state, window| {
            let Some(mut state) = state else {
                let entity = construct_component::<ComponentType>(input, cx);
                let lifecycle = ComponentType::attach_mount(&entity, cx);
                let render_token = lifecycle.render_token();
                let handler = Rc::new(RefCell::new(
                    handler
                        .take()
                        .expect("component event handler is consumed once"),
                ));
                let subscribed_handler = Rc::clone(&handler);
                let subscription =
                    window.subscribe(&entity, cx, move |_entity, event: &Event, window, cx| {
                        subscribed_handler.borrow_mut()(event, window, cx);
                    });
                let mount =
                    ComponentEventMount::new(entity.clone(), subscription, handler, lifecycle);
                return (
                    HostedEntity::new(entity, render_token),
                    ComponentEventMountState::new(mount),
                );
            };

            state.mount.replace_handler(
                handler
                    .take()
                    .expect("component event handler is consumed once"),
            );
            let entity = state.mount.entity();
            if reconcile_component(&entity, input, cx) {
                state.mount.lifecycle.input_changed();
            }
            let hosted = HostedEntity::new(entity, state.mount.lifecycle.render_token());
            (hosted, state)
        },
    )
}

/// Runs a mount operation below the shared compile-slot and user-key namespaces.
fn with_component_identity<Result>(
    slot: ElementId,
    key: Option<ElementId>,
    window: &mut Window,
    mount: impl FnOnce(&GlobalElementId, &mut Window) -> Result,
) -> Result {
    window.with_global_id(slot, |slot_global_id, window| match key {
        Some(key) => window.with_global_id(key, mount),
        None => mount(slot_global_id, window),
    })
}

/// Adds the compile-site and optional user-key namespaces before mounting.
fn mount_component<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>(
    slot: ElementId,
    key: Option<ElementId>,
    input: ComponentType::Input,
    subscribe: Subscribe,
    window: &mut Window,
    cx: &mut App,
) -> HostedEntity<ComponentType>
where
    ComponentType: NativeComponent,
    Subscribe: FnOnce(&Entity<ComponentType>, &mut Window, &mut App) -> [Subscription; SUBSCRIPTIONS]
        + 'static,
{
    let mut input = Some(input);
    let mut subscribe = Some(subscribe);
    with_component_identity(slot, key, window, |global_id, window| {
        mount_at_id(global_id, &mut input, &mut subscribe, window, cx)
    })
}

/// Adds the shared identity namespaces before event-aware mounting.
fn mount_component_with_events<ComponentType, Event, Handler>(
    slot: ElementId,
    key: Option<ElementId>,
    input: ComponentType::Input,
    handler: Handler,
    window: &mut Window,
    cx: &mut App,
) -> HostedEntity<ComponentType>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
    Handler: FnMut(&Event, &mut Window, &mut App) + 'static,
{
    let mut input = Some(input);
    let mut handler = Some(handler);
    with_component_identity(slot, key, window, |global_id, window| {
        mount_event_at_id(global_id, &mut input, &mut handler, window, cx)
    })
}

#[cfg(test)]
mod tests {
    //! Construction-time behavior that does not require a platform window.

    use std::any::TypeId;
    use std::cell::Cell;
    use std::mem::size_of;
    use std::rc::Rc;

    use gpui::{Context, ElementId, IntoElement, Render, Window, div};

    use super::{
        ComponentMount, ComponentMountState, LifecycleCallback, LifecyclePhase, LifecycleSignals,
        NativeComponent, PropMissing, PropSet, RequiredProp, component_element,
    };

    /// First compile-site factory identity used by the state-key regression test.
    struct FirstFactoryIdentity;

    /// Second compile-site factory identity used by the state-key regression test.
    struct SecondFactoryIdentity;

    /// Minimal component used to type-check the public host recipe.
    struct Fixture {
        /// Last reconciled scalar input.
        value: usize,
    }

    impl NativeComponent for Fixture {
        type Props = usize;
        type Input = usize;
        type MountState = ();

        fn construct(input: Self::Input, _cx: &mut Context<Self>) -> Self {
            Self { value: input }
        }

        fn reconcile_input(&mut self, input: Self::Input, _cx: &mut Context<Self>) -> bool {
            let changed = self.value != input;
            self.value = input;
            changed
        }
    }

    impl Render for Fixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    /// Building or dropping a frame recipe does not invoke its mount-only factory.
    #[test]
    fn subscription_factory_is_lazy_until_mount() {
        let calls = Rc::new(Cell::new(0));
        let calls_for_factory = Rc::clone(&calls);
        let element = component_element::<Fixture, _, 0>(
            ElementId::from("fixture-slot"),
            None,
            1,
            move |_: &gpui::Entity<Fixture>,
                  _: &mut Window,
                  _: &mut gpui::App|
                  -> [gpui::Subscription; 0] {
                calls_for_factory.set(calls_for_factory.get() + 1);
                []
            },
        );

        drop(element);
        assert_eq!(calls.get(), 0);
    }

    /// Required-property storage is exactly one inline `Option` plus zero-sized state.
    #[test]
    fn required_property_storage_stays_inline() {
        assert_eq!(
            size_of::<RequiredProp<String, PropMissing>>(),
            size_of::<Option<String>>()
        );
        assert_eq!(
            size_of::<RequiredProp<String, PropSet>>(),
            size_of::<Option<String>>()
        );
        assert_eq!(size_of::<PropMissing>(), 0);
        assert_eq!(size_of::<PropSet>(), 0);
    }

    /// Factory closure identity affects GPUI's state type without adding storage.
    #[test]
    fn subscription_factory_type_namespaces_retained_state() {
        type FirstState = ComponentMountState<Fixture, FirstFactoryIdentity, 0>;
        type SecondState = ComponentMountState<Fixture, SecondFactoryIdentity, 0>;

        assert_ne!(TypeId::of::<FirstState>(), TypeId::of::<SecondState>());
        assert_eq!(
            size_of::<FirstState>(),
            size_of::<ComponentMount<Fixture, 0>>()
        );
    }

    /// A hook-free hosted entity adds no storage beyond the native entity handle.
    #[test]
    fn hook_free_host_token_is_zero_cost() {
        assert_eq!(
            size_of::<super::HostedEntity<Fixture>>(),
            size_of::<gpui::Entity<Fixture>>()
        );
        assert_eq!(size_of::<<Fixture as NativeComponent>::MountState>(), 0);
    }

    /// A second completed render is covered by an already-queued mounted hook.
    #[test]
    fn queued_mount_consumes_dirty_render_state() {
        let signals = LifecycleSignals::new();

        assert_eq!(signals.rendered(true), Some(LifecycleCallback::Mounted));
        signals.notified();
        assert_eq!(signals.rendered(true), None);
        assert!(!signals.dirty.get());
        assert!(signals.complete(LifecycleCallback::Mounted));
        assert_eq!(signals.phase.get(), LifecyclePhase::ActiveRendered);
        assert_eq!(signals.rendered(true), None);
    }

    /// A queued update covers later renders without leaking stale dirty state.
    #[test]
    fn queued_update_coalesces_a_second_completed_render() {
        let signals = LifecycleSignals::new();

        assert_eq!(signals.rendered(false), None);
        signals.input_changed();
        assert_eq!(signals.rendered(false), Some(LifecycleCallback::Updated));
        signals.notified();
        assert_eq!(signals.rendered(false), None);
        assert!(!signals.dirty.get());
        assert!(signals.complete(LifecycleCallback::Updated));
        assert_eq!(signals.rendered(false), None);
    }

    /// Dirty work after the last render survives callback completion for the next render.
    #[test]
    fn post_render_dirty_state_is_not_consumed_by_callback_completion() {
        let signals = LifecycleSignals::new();

        assert_eq!(signals.rendered(false), None);
        signals.notified();
        assert_eq!(signals.rendered(false), Some(LifecycleCallback::Updated));
        signals.notified();
        assert!(signals.complete(LifecycleCallback::Updated));
        assert!(signals.dirty.get());
        assert_eq!(signals.rendered(false), Some(LifecycleCallback::Updated));
    }

    /// Visual teardown invalidates callbacks and claims an eligible unmount once.
    #[test]
    fn unmount_invalidates_callbacks_and_is_claimed_exactly_once() {
        let signals = LifecycleSignals::new();

        assert_eq!(signals.rendered(true), Some(LifecycleCallback::Mounted));
        signals.queue_unmount();
        assert!(!signals.complete(LifecycleCallback::Mounted));
        assert!(signals.claim_unmount());
        assert!(!signals.claim_unmount());

        let never_rendered = LifecycleSignals::new();
        never_rendered.queue_unmount();
        assert!(!never_rendered.claim_unmount());
    }
}
