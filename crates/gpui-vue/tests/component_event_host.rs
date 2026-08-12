//! Type, layout, and construction-time coverage for the typed event host.
//!
//! GPUI only permits `Window::with_element_state` during an actual draw pass,
//! while this workspace deliberately keeps its default dependency free of the
//! desktop-backed `test-support` feature. These tests therefore cover the
//! public monomorphic types and pre-mount behavior; mount/reconcile ordering is
//! enforced in the shared runtime path and checked by strict compilation.

use std::{
    cell::{Cell, RefCell},
    mem::{size_of, size_of_val},
    rc::Rc,
};

use gpui_vue::gpui::{
    App, Context, ElementId, Entity, EventEmitter, IntoElement, Render, Subscription, Window, div,
};
use gpui_vue::{
    ComponentElement, ComponentEventElement, ComponentEventMount, NativeComponent,
    NativeComponentEvents, component_element, component_element_with_events,
};

/// Minimal native component used by the external host contract tests.
struct Fixture {
    /// Last scalar input received from its parent.
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

/// Typed event advertised by [`Fixture`].
struct FixtureEvent;

impl EventEmitter<FixtureEvent> for Fixture {}

impl NativeComponentEvents for Fixture {
    type Event = FixtureEvent;
}

/// Concrete handler type used to inspect the retained mount representation.
type FixtureHandler = fn(&FixtureEvent, &mut Window, &mut App);

/// Verifies that the new constructor exposes a native GPUI element recipe.
#[test]
fn event_host_has_a_fully_typed_native_element_path() {
    let element: ComponentEventElement<Fixture, FixtureEvent, _> = component_element_with_events(
        ElementId::from("event-fixture-slot"),
        Some(ElementId::from("record-9")),
        9,
        |_event: &FixtureEvent, _window: &mut Window, _cx: &mut App| {},
    );

    let _native_gpui_element = element.into_element();
}

/// Building and dropping a frame recipe neither calls nor wraps its handler.
#[test]
fn event_handler_is_lazy_until_a_native_event_is_emitted() {
    let calls = Rc::new(Cell::new(0));
    let calls_for_handler = Rc::clone(&calls);
    let element = component_element_with_events::<Fixture, FixtureEvent, _>(
        ElementId::from("lazy-event-fixture-slot"),
        None,
        1,
        move |_event: &FixtureEvent, _window: &mut Window, _cx: &mut App| {
            calls_for_handler.set(calls_for_handler.get() + 1);
        },
    );

    drop(element);
    assert_eq!(calls.get(), 0);
}

/// Verifies zero-capture recipes stay inline and the retained state has only
/// its subscription, one shared handler pointer, and one entity handle.
#[test]
fn event_host_storage_has_no_erased_dispatch_container() {
    let plain = component_element::<Fixture, _, 0>(
        ElementId::from("plain-size-slot"),
        None,
        1,
        |_: &Entity<Fixture>, _: &mut Window, _: &mut App| -> [Subscription; 0] { [] },
    );
    let event = component_element_with_events::<Fixture, FixtureEvent, _>(
        ElementId::from("event-size-slot"),
        None,
        1,
        |_event: &FixtureEvent, _window: &mut Window, _cx: &mut App| {},
    );

    assert_eq!(size_of_val(&plain), size_of_val(&event));
    assert_eq!(
        size_of::<ComponentEventMount<Fixture, FixtureHandler>>(),
        size_of::<Subscription>()
            + size_of::<Rc<RefCell<FixtureHandler>>>()
            + size_of::<Entity<Fixture>>()
    );

    let _: ComponentElement<Fixture, _, 0> = plain;
}
