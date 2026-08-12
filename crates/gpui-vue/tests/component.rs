//! Expansion and type-level coverage for the `component!` item macro.

use gpui_vue::gpui::{
    App, Context, ElementId, Entity, EventEmitter, IntoElement, ParentElement, Subscription, Window,
};
use gpui_vue::{
    Local, NativeComponent, NativeComponentEvents, NativeComponentSlots, PropMissing, PropSet,
    Slot, component, component_element, view,
};

component! {
    /// A compile-tested component with required and defaulted properties.
    pub component Greeting {
        props {
            /// Text required by `GreetingProps::new`.
            pub label: String,
            /// Initial count supplied by a default or fluent override.
            pub initial: usize = 3,
        }

        state {
            /// Entity-local count initialized from the immutable properties.
            pub count: Local<usize> = Local::new(props.initial),
            /// A value mutated by the one-shot setup hook.
            pub setup_marker: usize = 0,
        }

        setup(this, props, cx) {
            this.setup_marker = props.initial + 1;
            let _ = cx.entity_id();
        }

        template(this, _window, _cx) {
            view! {
                <div class="flex gap-2">
                    <text>{this.props().label.clone()}</text>
                    <text>{this.count.get().to_string()}</text>
                </div>
            }
        }
    }
}

component! {
    /// A direct-markup outlet fixture whose context binders are used implicitly.
    component DirectOutletPanel {
        props {
            /// A raw-keyword property with an ordinary `with_type` override.
            r#type: usize = 0,
        }

        slots {
            /// Optional default body.
            default: ();
            /// Optional typed actions body.
            actions: ActionSlotProps;
            /// A raw-keyword slot whose fluent setter is `with_type`.
            r#type: ();
        }

        template(_this, _window, _cx) {
            <div>
                <slot />
                <slot name="actions" :props={ActionSlotProps { count: 2 }} />
                <slot name="type" />
            </div>
        }
    }
}

component! {
    /// A component that compile-checks every visual lifecycle section.
    component LifecycleFixture {
        state {
            /// Number of lifecycle bodies that have run.
            calls: usize = 0,
        }

        unmounted(this, cx) {
            this.calls += 1;
            let _ = cx.background_executor();
        }

        mounted(this, window, cx) {
            this.calls += 1;
            let _ = (window.viewport_size(), cx.entity_id());
        }

        updated(this, window, cx) {
            this.calls += 1;
            let _ = (window.viewport_size(), cx.entity_id());
        }

        template(this, _window, _cx) {
            view! { <text>{this.calls.to_string()}</text> }
        }
    }
}

/// Requires a generated lifecycle component to select the native typed mount.
fn assert_visual_lifecycle<Component>()
where
    Component: gpui_vue::ComponentLifecycleHooks<MountState = gpui_vue::ComponentLifecycleMount<Component>>,
{
}

/// Verifies all lifecycle bodies and their GPUI binders compile as typed Rust.
#[test]
fn generates_typed_visual_lifecycle_hooks() {
    assert_visual_lifecycle::<LifecycleFixture>();
}

/// Verifies required/default props and the generated GPUI entity constructor type.
#[test]
fn constructs_typed_props_and_exposes_an_entity_constructor() {
    let props = GreetingProps::new("hello".to_owned()).with_initial(7);
    assert_eq!(props.label, "hello");
    assert_eq!(props.initial, 7);

    let constructor: fn(GreetingProps, &mut App) -> Entity<Greeting> = Greeting::new::<App>;
    let _ = constructor;

    let missing: GreetingPropsBuilder<PropMissing> = GreetingProps::builder();
    let complete: GreetingPropsBuilder<PropSet> = missing.label("builder".to_owned()).initial(9);
    let built = complete.build();
    assert_eq!(built.label, "builder");
    assert_eq!(built.initial, 9);
}

/// Requires a generated component to expose the exact native host input type.
fn assert_native_component<Component, Input>()
where
    Component: NativeComponent<Input = Input>,
    Input: 'static,
{
}

/// Requires a generated slotted component to expose its exact associated slot type.
fn assert_native_component_slots<Component, Slots>()
where
    Component: NativeComponentSlots<Slots = Slots>,
    Slots: Default + 'static,
{
}

/// Verifies generated inputs, trait entry points, and direct GPUI element conversion.
#[test]
fn generates_a_native_persistent_host_contract() {
    assert_native_component::<Greeting, GreetingInput>();

    let _: for<'context, 'borrow> fn(
        GreetingInput,
        &'borrow mut Context<'context, Greeting>,
    ) -> Greeting = <Greeting as NativeComponent>::construct;
    let _: for<'component, 'context, 'borrow> fn(
        &'component mut Greeting,
        GreetingInput,
        &'borrow mut Context<'context, Greeting>,
    ) -> bool = <Greeting as NativeComponent>::reconcile_input;

    let original = GreetingProps::new("stable".to_owned()).with_initial(5);
    let equal = GreetingProps::new("stable".to_owned()).with_initial(5);
    let changed = GreetingProps::new("changed".to_owned()).with_initial(5);
    assert!(original == equal);
    assert!(original != changed);

    let element = component_element::<Greeting, _, 0>(
        ElementId::from("greeting-host"),
        Some(ElementId::from("record-7")),
        GreetingInput::new(original),
        |_: &Entity<Greeting>, _: &mut Window, _: &mut App| -> [Subscription; 0] { [] },
    );
    let _native_gpui_element = element.into_element();
}

component! {
    /// A component whose properties are all defaulted.
    component FullyDefaulted {
        props {
            /// Default text used by `Default` and `new`.
            label: &'static str = "ready",
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().label}</text> }
        }
    }
}

/// Verifies the generated `Default` implementation for an all-default props type.
#[test]
fn all_default_props_implement_default() {
    let props = FullyDefaultedProps::default();
    assert_eq!(props.label, "ready");
    let via_constructor = FullyDefaultedProps::new();
    assert_eq!(via_constructor.label, "ready");
    let via_builder = FullyDefaultedProps::builder().label("overridden").build();
    assert_eq!(via_builder.label, "overridden");
}

component! {
    /// A component with several move-only required builder properties.
    component MultiRequiredBuilder {
        props {
            /// Move-only display label.
            label: String,
            /// Move-only byte storage.
            bytes: Vec<u8>,
            /// Default flag that does not participate in typestate.
            enabled: bool = true,
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().label.clone()}</text> }
        }
    }
}

/// Verifies N-required transitions, move-only values, and last-write-wins setters.
#[test]
fn props_builder_supports_move_only_values_and_repeated_setters() {
    let props = MultiRequiredBuilderProps::builder()
        .bytes(vec![1, 2, 3])
        .label("first".to_owned())
        .label("last".to_owned())
        .enabled(false)
        .build();

    assert_eq!(props.label, "last");
    assert_eq!(props.bytes, [1, 2, 3]);
    assert!(!props.enabled);
}

component! {
    /// A component exposing compile-time checked native GPUI events.
    component TypedEmitter {
        emits {
            /// Reports a newly selected value.
            change(value: i32);
            /// Reports a submission with more than one typed payload.
            submit(
                /// Submitted label.
                label: String,
                accepted: bool,
            );
            /// Reports that the component returned to its initial state.
            reset();
        }

        template(this, _window, _cx) {
            let _ = this.props();
            view! { <div /> }
        }
    }
}

/// Requires the generated component to advertise its event enum to GPUI.
fn assert_typed_emitter<Component>()
where
    Component: EventEmitter<TypedEmitterEvent> + NativeComponentEvents<Event = TypedEmitterEvent>,
{
}

/// Verifies event variants and helpers retain their declared payload types.
#[test]
fn generates_typed_native_event_api() {
    assert_typed_emitter::<TypedEmitter>();
    assert_native_component::<TypedEmitter, TypedEmitterInput>();

    let _: for<'context, 'borrow> fn(i32, &'borrow mut Context<'context, TypedEmitter>) =
        TypedEmitter::emit_change;
    let _: for<'context, 'borrow> fn(String, bool, &'borrow mut Context<'context, TypedEmitter>) =
        TypedEmitter::emit_submit;
    let _: for<'context, 'borrow> fn(&'borrow mut Context<'context, TypedEmitter>) =
        TypedEmitter::emit_reset;

    let change = TypedEmitterEvent::Change { value: 41 };
    let TypedEmitterEvent::Change { value } = change else {
        panic!("constructed the change variant")
    };
    assert_eq!(value, 41);

    let submit = TypedEmitterEvent::Submit {
        label: "save".to_owned(),
        accepted: true,
    };
    let TypedEmitterEvent::Submit { label, accepted } = submit else {
        panic!("constructed the submit variant")
    };
    assert_eq!(label, "save");
    assert!(accepted);

    assert!(matches!(TypedEmitterEvent::Reset, TypedEmitterEvent::Reset));
}

/// Props exposed to the named scoped slot in [`SlottedPanel`].
#[derive(Clone, Copy)]
pub struct ActionSlotProps {
    /// The current number of actions.
    pub count: usize,
}

component! {
    /// A component exercising default, named, scoped, and fallback slots.
    pub component SlottedPanel {
        slots {
            /// The panel's default unscoped body.
            default: ();
            /// A named slot receiving the current action count.
            actions: ActionSlotProps;
        }

        template(this, window, cx) {
            let body = this.slots().default.render_or_else(
                (),
                window,
                &mut *cx,
                |(), _window, _cx| gpui_vue::gpui::div().child("fallback"),
            );
            let actions = this.slots().actions.render(
                ActionSlotProps { count: 2 },
                window,
                &mut *cx,
            );
            gpui_vue::gpui::div().child(body).children(actions)
        }
    }
}

/// Verifies generated slot types, builders, and both constructor paths.
#[test]
fn generates_typed_lazy_slot_api() {
    let slots = SlottedPanelSlots::new()
        .with_default(Slot::from_fn(|()| gpui_vue::gpui::div().child("provided")))
        .with_actions(Slot::new(|props: ActionSlotProps, _window, _cx| {
            gpui_vue::gpui::div().child(props.count.to_string())
        }));

    assert!(slots.default.is_present());
    assert!(slots.actions.is_present());
    let _: &Slot<()> = &slots.default;
    let _: &Slot<ActionSlotProps> = &slots.actions;

    let default_constructor: fn(SlottedPanelProps, &mut App) -> Entity<SlottedPanel> =
        SlottedPanel::new::<App>;
    let slotted_constructor: fn(
        SlottedPanelProps,
        SlottedPanelSlots,
        &mut App,
    ) -> Entity<SlottedPanel> = SlottedPanel::new_with_slots::<App>;
    let _ = (default_constructor, slotted_constructor);

    assert_native_component::<SlottedPanel, SlottedPanelInput>();
    assert_native_component_slots::<SlottedPanel, SlottedPanelSlots>();
    let _: for<'component> fn(&'component SlottedPanel) -> &'component SlottedPanelSlots =
        <SlottedPanel as NativeComponentSlots>::slots;
    let trait_input = <SlottedPanel as NativeComponentSlots>::input_with_slots(
        SlottedPanelProps::new(),
        SlottedPanelSlots::default(),
    );
    let element = component_element::<SlottedPanel, _, 0>(
        ElementId::from("slotted-panel-host"),
        None,
        SlottedPanelInput::new(SlottedPanelProps::new()).with_slots(slots),
        |_: &Entity<SlottedPanel>, _: &mut Window, _: &mut App| -> [Subscription; 0] { [] },
    );
    let _native_gpui_element = element.into_element();
    let _ = trait_input;

    assert_native_component_slots::<DirectOutletPanel, DirectOutletPanelSlots>();
    let direct_constructor: fn(
        DirectOutletPanelProps,
        DirectOutletPanelSlots,
        &mut App,
    ) -> Entity<DirectOutletPanel> = DirectOutletPanel::new_with_slots::<App>;
    let _ = direct_constructor;

    let raw_slots = DirectOutletPanelSlots::new()
        .with_type(Slot::from_fn(|()| gpui_vue::gpui::div().child("raw slot")));
    let _: &Slot<()> = &raw_slots.r#type;
    let raw_props = DirectOutletPanelProps::new().with_type(7);
    assert_eq!(raw_props.r#type, 7);
    let _raw_provider = view! {
        <DirectOutletPanel>
            <template #type>
                <text>"raw provider"</text>
            </template>
        </DirectOutletPanel>
    }
    .into_element();
}

/// Components without a slots section carry no hidden slot storage.
#[test]
fn components_without_slots_keep_their_original_layout() {
    assert_eq!(
        std::mem::size_of::<FullyDefaulted>(),
        std::mem::size_of::<FullyDefaultedProps>()
    );
}
