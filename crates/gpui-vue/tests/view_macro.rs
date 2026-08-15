//! Compile and type-level coverage for the `view!` macro expansion.

use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
};

use gpui_vue::gpui::{App, ClickEvent, IntoElement, ParentElement, PinchEvent, Window};
use gpui_vue::media::ObjectFit;
use gpui_vue::ui::{
    Context, DragMoveEvent, Entity, ExternalPaths, FocusHandle, KeyUpEvent, ModifiersChangedEvent,
    MouseDownEvent, Render, ScreenPoint, StyleRefinement, px, rgb,
};
use gpui_vue::{Slot, component, view};

/// Typed props supplied by [`Child`] when it renders its actions slot.
#[derive(Clone, Copy)]
struct ChildActionProps {
    /// Number shown by the parent-provided action content.
    count: usize,
}

component! {
    /// Cross-crate fixture for native `PascalCase` host lowering.
    component Child {
        props {
            /// Label rendered by the child entity.
            label: String,
        }

        slots {
            /// Optional content supplied through the typed P0 slot binding.
            default: ();
            /// Optional actions content with typed scoped props.
            actions: ChildActionProps;
        }

        template(this, window, cx) {
            <div>
                <text>{this.props().label.clone()}</text>
                <slot>
                    <button
                        id="default-slot-fallback"
                        @click={cx.listener(
                            |_this, _event: &ClickEvent, _window, _cx| {},
                        )}
                    >
                        "Fallback"
                    </button>
                    <text>"Second fallback root"</text>
                </slot>
                <slot name="actions" :props={ChildActionProps { count: 2 }} />
            </div>
        }
    }
}

use self::Child as SlottedAlias;

mod widgets {
    //! Components kept behind a module boundary to exercise hygienic aliases.

    use gpui_vue::{component, view};

    component! {
        /// Public child used through the [`super::Card`] alias.
        pub component Child {
            props {
                /// Label supplied by either complete or individual prop syntax.
                pub label: String,
            }

            template(this, _window, _cx) {
                view! { <text>{this.props().label.clone()}</text> }
            }
        }
    }
}

mod event_widgets {
    //! Public emitting component kept behind a module boundary for alias hygiene.

    use gpui_vue::{component, view};

    component! {
        /// Child used to compile-test typed `PascalCase` template listeners.
        pub component EventChild {
            props {
                /// Label captured independently by the parent listener.
                pub label: String,
            }

            emits {
                /// Reports a changed numeric value.
                value_change(value: usize);
                /// Requests that the child be closed.
                close();
            }

            slots {
                /// Optional parent-owned body content.
                default: ();
            }

            template(this, _window, _cx) {
                view! { <text>{this.props().label.clone()}</text> }
            }
        }
    }
}

use event_widgets::EventChild as EventCard;
use widgets::Child as Card;

component! {
    /// Direct-markup parent whose providers re-enter its live native context.
    component ContextualSlotParent {
        props {
            /// Text read lazily when a descendant invokes the provider.
            label: String,
        }

        state {
            /// Click count mutated by a listener authored inside slot content.
            clicks: usize = 0,
        }

        template(this, window, cx) {
            <SlottedAlias label={this.props().label.clone()}>
                <text>{format!("{}:{}", this.props().label, this.clicks)}</text>
                <template #actions={ChildActionProps { count }}>
                    <button id="contextual-slot-action" @click={cx.listener(
                        |this, _event: &ClickEvent, _window, cx| {
                            this.clicks += 1;
                            cx.notify();
                        },
                    )}>
                        {format!("{}:{count}:{:?}", this.props().label, window.viewport_size())}
                    </button>
                    <SlottedAlias label={this.props().label.clone()}>
                        <text>{this.clicks.to_string()}</text>
                    </SlottedAlias>
                </template>
            </SlottedAlias>
        }
    }
}

/// Emits each `view!` call from the same repeated transcriber tokens.
///
/// In particular, the two `Card` tags have the same source span and each
/// invocation-local element ordinal is zero. Their generated subscription
/// closure types must therefore remain part of the retained mount identity.
macro_rules! same_span_component_siblings {
    ($($label:expr),+ $(,)?) => {
        gpui_vue::gpui::div()
            $(.child(view! { <Card label={$label} /> }))+
    };
}

/// Produces the concrete type IDs of repeated zero-capture host factories.
macro_rules! same_span_component_type_ids {
    ($($label:expr),+ $(,)?) => {{
        let mut ids = Vec::new();
        $(
            let component = view! { <Card label={$label} /> };
            ids.push(concrete_type_id(&component));
        )+
        ids
    }};
}

/// Returns the concrete type identity inferred for one native element recipe.
fn concrete_type_id<Value: 'static>(_: &Value) -> TypeId {
    TypeId::of::<Value>()
}

component! {
    /// Cross-crate fixture for a `PascalCase` component with no declared props.
    component EmptyChild {
        template(this, _window, _cx) {
            let _ = this.props();
            view! { <text>"empty child"</text> }
        }
    }
}

component! {
    /// Component whose implicit default provider deliberately ignores non-unit props.
    component TypedDefaultChild {
        slots {
            /// Typed default content whose props may be ignored by ordinary children.
            default: usize;
        }

        template(this, window, cx) {
            this.slots().default.render_or_else(
                7,
                window,
                &mut *cx,
                |value, _window, _cx| gpui_vue::gpui::div().child(value.to_string()),
            )
        }
    }
}

component! {
    /// Cross-crate fixture for exact individual prop setter lowering.
    component IndividualPropsChild {
        props {
            /// Move-only label supplied through a kebab-case bound attribute.
            owned_label: String,
            /// Move-only bytes supplied through an ordinary expression attribute.
            payload: Vec<u8>,
            /// Boolean value supplied through a bare attribute.
            enabled: bool,
            /// Borrowed string supplied through a static literal attribute.
            display_name: &'static str,
        }

        template(this, _window, _cx) {
            let summary = format!(
                "{}:{}:{}:{}",
                this.props().owned_label,
                this.props().payload.len(),
                this.props().enabled,
                this.props().display_name,
            );
            view! { <text>{summary}</text> }
        }
    }
}

component! {
    /// Component whose Rust-keyword property is authored without `r#` in markup.
    component RawKeywordPropsChild {
        props {
            /// Keyword-named value exposed through the raw `r#type` builder method.
            r#type: usize,
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().r#type.to_string()}</text> }
        }
    }
}

fn build_view(show_details: bool, items: Vec<(usize, String)>) -> impl IntoElement {
    view! {
        <div class="w-full flex flex-col gap-4 p-4 bg-slate-950 text-white">
            <button
                id="compile-tested-button"
                class="rounded-lg bg-blue-600 hover:bg-blue-500 active:bg-blue-700"
                @click={|_, _, _| {}}
            >
                "Click"
            </button>
            <text v-if={show_details} class="text-sm text-slate-400">"Details"</text>
            <div id="focusable-card" class="p-2 focus:bg-slate-800">"Focusable card"</div>
            <div class="flex flex-col gap-1">
                <span
                    v-for={item in items}
                    v-if={show_details}
                    :key={("item", item.0)}
                    class="px-2 py-1 rounded bg-emerald-700"
                >
                    {item.1}
                </span>
            </div>
        </div>
    }
}

fn modified_click(_: &ClickEvent, _: &mut Window, _: &mut App) {}

fn native_pinch(_: &PinchEvent, _: &mut Window, _: &mut App) {}

fn native_key_up(_: &KeyUpEvent, _: &mut Window, _: &mut App) {}

fn native_modifiers_changed(_: &ModifiersChangedEvent, _: &mut Window, _: &mut App) {}

fn native_mouse_down_out(_: &MouseDownEvent, _: &mut Window, _: &mut App) {}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn native_hover(_: &bool, _: &mut Window, _: &mut App) {}

fn native_focus(_: &mut Window, _: &mut App) {}

fn native_blur(_: &mut Window, _: &mut App) {}

/// Exact typed value retained by GPUI throughout one row drag.
struct DraggedRow {
    row: usize,
}

/// A second payload type used to verify independent repeated listener lanes.
struct DraggedTab;

/// Native entity rendered under the pointer while a row is being dragged.
struct RowDragPreview(usize);

impl Render for RowDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        gpui_vue::ui::div().child(self.0.to_string())
    }
}

fn move_dragged_row(_: &DragMoveEvent<DraggedRow>, _: &mut Window, _: &mut App) {}

fn move_dragged_tab(_: &DragMoveEvent<DraggedTab>, _: &mut Window, _: &mut App) {}

fn drop_dragged_row(_: &DraggedRow, _: &mut Window, _: &mut App) {}

fn drop_external_paths(_: &ExternalPaths, _: &mut Window, _: &mut App) {}

fn accepts_dragged_row(payload: &dyn Any, _: &mut Window, _: &mut App) -> bool {
    payload.is::<DraggedRow>()
}

fn row_drag_over(
    style: StyleRefinement,
    _: &DraggedRow,
    _: &mut Window,
    _: &mut App,
) -> StyleRefinement {
    style
}

fn external_paths_drag_over(
    style: StyleRefinement,
    _: &ExternalPaths,
    _: &mut Window,
    _: &mut App,
) -> StyleRefinement {
    style
}

/// Builds a drag source/drop target whose binding factories expose evaluation
/// order without escaping their borrowed log into GPUI's retained callbacks.
fn build_drag_drop_view(evaluations: &RefCell<Vec<&'static str>>) -> impl IntoElement + use<> {
    view! {
        <div
            id="typed-drag-surface"
            :drag-preview={{
                evaluations.borrow_mut().push("preview");
                |payload: &DraggedRow,
                 _offset: ScreenPoint,
                 _window: &mut Window,
                 cx: &mut App| -> Entity<RowDragPreview> {
                    let row = payload.row;
                    cx.new(move |_| RowDragPreview(row))
                }
            }}
            @drag-move={{
                evaluations.borrow_mut().push("row move");
                move_dragged_row
            }}
            :drag-payload={{
                evaluations.borrow_mut().push("payload");
                DraggedRow { row: 7 }
            }}
            on:drag-move={{
                evaluations.borrow_mut().push("tab move");
                move_dragged_tab
            }}
            :can-drop={{
                evaluations.borrow_mut().push("predicate");
                accepts_dragged_row
            }}
            :drag-over={{
                evaluations.borrow_mut().push("row style");
                row_drag_over
            }}
            @drop={{
                evaluations.borrow_mut().push("row drop");
                drop_dragged_row
            }}
            :drag-over={{
                evaluations.borrow_mut().push("file style");
                external_paths_drag_over
            }}
            on:drop={{
                evaluations.borrow_mut().push("file drop");
                drop_external_paths
            }}
        />
    }
}

fn build_native_pinch_view() -> impl IntoElement {
    view! {
        <div id="pinch-surface" on:pinch={native_pinch} />
    }
}

/// Builds one intrinsic `v-text` child between two source-ordered listeners.
fn build_v_text_view(
    evaluations: &RefCell<Vec<&'static str>>,
    visible: bool,
) -> impl IntoElement + use<> {
    view! {
        <span
            id="native-v-text"
            v-show={visible}
            @hover={{
                evaluations.borrow_mut().push("hover");
                native_hover
            }}
            v-text={{
                evaluations.borrow_mut().push("text");
                "Native text child".to_owned()
            }}
            @click={{
                evaluations.borrow_mut().push("click");
                modified_click
            }}
        />
    }
}

/// Compile-checks the extended typed native event surface, including exact
/// focus and blur subscriptions backed by one explicit focus handle.
#[allow(dead_code)]
fn build_extended_native_event_view(focus: &FocusHandle) -> impl IntoElement + use<> {
    view! {
        <div
            id="extended-native-events"
            :track-focus={focus}
            @key-up={native_key_up}
            @modifiers-changed={native_modifiers_changed}
            @mouse-down-out.left={native_mouse_down_out}
            on:mouse-down-out.right={native_mouse_down_out}
            @mouse-down-out.middle={native_mouse_down_out}
            @hover={native_hover}
            @focus={native_focus}
            @blur={native_blur}
        />
    }
}

fn build_structural_view(ready: bool, pending: bool) -> impl IntoElement {
    let id = "retained";
    view! {
        <>
            <template v-if={ready}>
                <text>"Ready"</text>
                <button
                    id="modified-click"
                    @click.stop.prevent.ctrl.exact={modified_click}
                >
                    "Continue"
                </button>
            </template>
            <text v-else-if={pending}>"Pending"</text>
            <text v-else>"Idle"</text>
            <div
                :id
                v-show={ready}
                :class={if ready {
                    "bg-emerald-700 hover:bg-emerald-600"
                } else {
                    "bg-slate-700"
                }}
            >
                "Always retained"
            </div>
        </>
    }
}

fn build_extended_tailwind_view() -> impl IntoElement {
    view! {
        <div
            id="extended-tailwind"
            class="absolute grid grid-cols-3 grid-rows-2 col-span-full col-start-2 hover:col-start-3 row-span-2 row-start-1 aspect-video active:aspect-square flex-none grow shrink basis-full p-13.5 -mx-[4px] w-[62.5%] top-auto border-x-2 border-slate-500/50 rounded-3xl rounded-t-lg rounded-tl-4xl shadow-2xs cursor-context-menu truncate whitespace-normal leading-tight focus:leading-normal line-clamp-3 overflow-hidden opacity-[.5] bg-[#336699cc]/[12.5%]"
        >
            "Typed utility lowering"
        </div>
    }
}

/// Compile-checks exact nested overflow values, CSS line-height ratios, and
/// GPUI's keyboard-modality-aware focus-visible refinement in one native view.
fn build_native_focus_visible_tailwind_view() -> impl IntoElement {
    view! {
        <div
            id="native-focus-visible-tailwind"
            class="overflow-clip hover:overflow-visible leading-[1.5] focus:bg-slate-800 focus-visible:bg-blue-500 active:bg-blue-700"
        >
            <span class="overflow-x-visible overflow-y-clip leading-[150%]">
                "Exact native Tailwind lowering"
            </span>
        </div>
    }
}

fn build_layout_tailwind_view() -> impl IntoElement {
    view! {
        <div
            id="layout-tailwind"
            class="grid place-content-evenly content-start items-stretch justify-stretch hover:justify-evenly overflow-y-scroll max-h-24"
        >
            <span class="content-normal self-baseline basis-full min-w-0 max-w-full rounded-[7px] rounded-t-[0.75rem] hover:rounded-br-[5px]">
                "Native alignment and retained scroll lowering"
            </span>
        </div>
    }
}

fn build_native_group_variant_view() -> impl IntoElement {
    view! {
        <div class="group">
            <span class="text-slate-500 group-hover:text-white">
                "Hovered through the native parent group"
            </span>
            <span id="group-active-target" class="group-active:opacity-50">
                "Active through the native parent group"
            </span>
            <div id="focused-ancestor" focusable>
                <span id="in-focus-target" class="in-focus:bg-slate-800">
                    "Styled inside the focused ancestor"
                </span>
            </div>
        </div>
    }
}

fn build_merged_dynamic_class_view(condition_calls: &Cell<usize>) -> impl IntoElement {
    view! {
        <div
            id="merged-dynamic-class"
            class="p-2 hover:bg-slate-500 active:bg-slate-600 focus:text-slate-700"
            :class={if {
                condition_calls.set(condition_calls.get() + 1);
                true
            } {
                "px-4 hover:bg-blue-500 active:bg-blue-600 focus:text-blue-700"
            } else {
                "px-6 hover:bg-red-500 active:bg-red-600 focus:text-red-700"
            }}
        >
            "One condition evaluation and one callback per state"
        </div>
    }
}

fn build_button_cursor_override_view() -> impl IntoElement {
    view! {
        <button id="disabled-action" class="cursor-not-allowed">
            "Unavailable"
        </button>
    }
}

fn consume_condition(value: String) -> bool {
    !value.into_bytes().is_empty()
}

fn build_consuming_dynamic_class_view(value: String) -> impl IntoElement {
    view! {
        <div :class={if consume_condition(value) { "p-2" } else { "p-4" }}>
            "The condition may consume captured state"
        </div>
    }
}

fn build_pascal_component_view(label: String) -> impl IntoElement {
    view! {
        <Child :props={ChildProps::new(label)} />
    }
}

fn build_shorthand_pascal_component_view(label: String) -> impl IntoElement {
    view! {
        <Child :label />
    }
}

fn build_empty_pascal_component_view() -> impl IntoElement {
    view! {
        <EmptyChild />
    }
}

fn build_individual_pascal_props_view(owned_label: String, payload: Vec<u8>) -> impl IntoElement {
    view! {
        <IndividualPropsChild
            :owned-label={owned_label}
            payload={payload}
            enabled
            display-name="literal"
        />
    }
}

fn build_raw_keyword_prop_views(r#type: usize) -> impl IntoElement {
    gpui_vue::gpui::div()
        .child(view! { <RawKeywordPropsChild type={7_usize} /> })
        .child(view! { <RawKeywordPropsChild :type /> })
}

fn build_slotted_pascal_component_view() -> impl IntoElement {
    let slots = ChildSlots::new().with_default(Slot::from_fn(|()| {
        gpui_vue::gpui::div().child("typed slot")
    }));
    view! {
        <div>
            <Child
                key="slotted-child"
                label={"with slot".to_owned()}
                :slots={slots}
            />
        </div>
    }
}

/// Builds default and scoped named providers from owned parent captures.
fn build_declarative_pascal_slots(
    label: String,
    default_capture: String,
    action_capture: String,
) -> impl IntoElement {
    view! {
        <Child label={label}>
            <text>{default_capture.clone()}</text>
            <div>"second default root"</div>
            <template #actions={ChildActionProps { count }}>
                <text>{format!("{action_capture}:{count}")}</text>
            </template>
        </Child>
    }
}

/// Builds an unscoped named provider for a non-unit slot-props type.
fn build_unscoped_named_pascal_slot(label: String, capture: String) -> impl IntoElement {
    view! {
        <Child label={label}>
            <template #actions>
                <text>{capture.clone()}</text>
            </template>
        </Child>
    }
}

/// Builds an implicit default provider that ignores typed default-slot props.
fn build_typed_implicit_default_slot(capture: String) -> impl IntoElement {
    view! {
        <TypedDefaultChild>
            <text>{capture.clone()}</text>
        </TypedDefaultChild>
    }
}

/// Builds declarative providers through a Rust alias of the component type.
fn build_aliased_declarative_slots(label: String, capture: String) -> impl IntoElement {
    view! {
        <SlottedAlias label={label}>
            <text>{capture.clone()}</text>
            <template #actions={ChildActionProps { count }}>
                <text>{count.to_string()}</text>
            </template>
        </SlottedAlias>
    }
}

fn build_keyed_pascal_component_list(items: Vec<(usize, String)>) -> impl IntoElement {
    view! {
        <div>
            <Child
                v-for={item in items}
                :key={("child", item.0)}
                :props={ChildProps::new(item.1)}
            />
        </div>
    }
}

fn build_aliased_pascal_component_views() -> impl IntoElement {
    gpui_vue::gpui::div()
        .child(view! {
            <Card :props={widgets::ChildProps::new("complete".to_owned())} />
        })
        .child(view! {
            <Card label={"individual".to_owned()} />
        })
}

fn build_contextual_slot_parent() -> impl IntoElement {
    view! { <ContextualSlotParent label={"live parent".to_owned()} /> }
}

fn build_same_span_pascal_component_siblings() -> impl IntoElement {
    same_span_component_siblings!("first".to_owned(), "second".to_owned())
}

/// Builds an aliased emitting component with two freshly evaluated handlers.
fn build_aliased_event_component_view(
    handler_evaluations: &Cell<usize>,
    label: String,
) -> impl IntoElement {
    let captured_label = label.clone();
    view! {
        <EventCard
            label={label}
            @value-change={{
                handler_evaluations.set(handler_evaluations.get() + 1);
                move |event: &event_widgets::EventChildEvent, _window: &mut Window, _cx: &mut App| {
                    let _ = (&captured_label, event);
                }
            }}
            on:close={{
                handler_evaluations.set(handler_evaluations.get() + 1);
                |_event: &event_widgets::EventChildEvent, _window: &mut Window, _cx: &mut App| {}
            }}
        >
            <text>"event component body"</text>
        </EventCard>
    }
}

/// Builds an intrinsic whose runtime dimensions and color cannot be expressed
/// by one compile-time Tailwind class literal.
fn build_typed_inline_style_view(
    refiner_evaluations: &Cell<usize>,
    selected: bool,
) -> impl IntoElement {
    view! {
        <div
            class="h-8 text-slate-500 hover:text-white"
            :class={if selected { "bg-blue-600" } else { "bg-slate-900" }}
            :style={{
                refiner_evaluations.set(refiner_evaluations.get() + 1);
                let runtime_width = if selected { px(96.0) } else { px(64.0) };
                move |style| {
                    style
                        .w(runtime_width)
                        .text_color(rgb(0xAA_BB_CC))
                }
            }}
        >
            "Runtime style"
        </div>
    }
}

/// Compile-checks native pointer occlusion and the styled image intrinsic.
fn build_image_overlay_view(source: String) -> impl IntoElement {
    view! {
        <div id="image-overlay" occlude class="absolute inset-0">
            <img
                :src={source}
                class="h-8 rounded"
                :style={|style| style.w(px(72.0))}
            />
        </div>
    }
}

/// Compile-checks GPUI's exact typed image policies and records render-time
/// expression order independently from the lazy replacement callbacks.
fn build_typed_image_view(evaluations: &RefCell<Vec<&'static str>>) -> impl IntoElement + use<> {
    view! {
        <img
            :loading={{
                evaluations.borrow_mut().push("loading");
                || gpui_vue::ui::div().child("Loading").into_any_element()
            }}
            :src={{
                evaluations.borrow_mut().push("source");
                "images/preview.png"
            }}
            :fallback={{
                evaluations.borrow_mut().push("fallback");
                || gpui_vue::ui::div().child("Unavailable").into_any_element()
            }}
            :object-fit={{
                evaluations.borrow_mut().push("object fit");
                ObjectFit::Cover
            }}
            class="size-full rounded"
        />
    }
}

/// Compile-checks an unannotated handler behind the generated click-modifier
/// wrapper, including the higher-ranked lifetimes required by GPUI listeners.
fn build_stopped_click_view() -> impl IntoElement {
    view! {
        <div id="stopped-click" @click.stop={|_, _, _| {}}>
            "Stopped"
        </div>
    }
}

#[test]
fn expands_to_a_native_gpui_element_tree() {
    let _element = build_view(true, vec![(1, "Vapor".to_owned())]).into_element();
    let _pinch = build_native_pinch_view().into_element();
    let _structural = build_structural_view(true, false).into_element();
    let _tailwind = build_extended_tailwind_view().into_element();
    let _native_focus_visible_tailwind = build_native_focus_visible_tailwind_view().into_element();
    let _layout_tailwind = build_layout_tailwind_view().into_element();
    let _native_group_variants = build_native_group_variant_view().into_element();
    let _button_cursor = build_button_cursor_override_view().into_element();
    let _consuming_condition =
        build_consuming_dynamic_class_view("owned".to_owned()).into_element();
    let _component = build_pascal_component_view("native child".to_owned()).into_element();
    let _shorthand_component =
        build_shorthand_pascal_component_view("shorthand".to_owned()).into_element();
    let _empty_component = build_empty_pascal_component_view().into_element();
    let _individual_component =
        build_individual_pascal_props_view("owned".to_owned(), vec![1, 2, 3]).into_element();
    let _raw_keyword_props = build_raw_keyword_prop_views(11).into_element();
    let _slotted_component = build_slotted_pascal_component_view().into_element();
    let _declarative_slots = build_declarative_pascal_slots(
        "declarative".to_owned(),
        "default".to_owned(),
        "action".to_owned(),
    )
    .into_element();
    let _unscoped_named =
        build_unscoped_named_pascal_slot("unscoped".to_owned(), "ignored props".to_owned())
            .into_element();
    let _typed_default =
        build_typed_implicit_default_slot("typed default".to_owned()).into_element();
    let _aliased_declarative =
        build_aliased_declarative_slots("alias".to_owned(), "captured".to_owned()).into_element();
    let _keyed_components =
        build_keyed_pascal_component_list(vec![(1, "first".to_owned()), (2, "second".to_owned())])
            .into_element();
    let _aliased_components = build_aliased_pascal_component_views().into_element();
    let _contextual_parent = build_contextual_slot_parent().into_element();
    let _same_span_components = build_same_span_pascal_component_siblings().into_element();
    let handler_evaluations = Cell::new(0);
    let _event_component =
        build_aliased_event_component_view(&handler_evaluations, "events".to_owned())
            .into_element();
    assert_eq!(handler_evaluations.get(), 2);
    let inline_style_evaluations = Cell::new(0);
    let _inline_style =
        build_typed_inline_style_view(&inline_style_evaluations, true).into_element();
    assert_eq!(inline_style_evaluations.get(), 1);
    let _image_overlay =
        build_image_overlay_view("https://example.invalid/preview.png".to_owned()).into_element();
    let image_evaluations = RefCell::new(Vec::new());
    let _typed_image = build_typed_image_view(&image_evaluations).into_element();
    assert_eq!(
        image_evaluations.into_inner(),
        ["loading", "source", "fallback", "object fit"]
    );
    let _stopped_click = build_stopped_click_view().into_element();
    let v_text_evaluations = RefCell::new(Vec::new());
    let _v_text = build_v_text_view(&v_text_evaluations, true).into_element();
    assert_eq!(v_text_evaluations.into_inner(), ["hover", "text", "click"]);
    let drag_evaluations = RefCell::new(Vec::new());
    let _drag_drop = build_drag_drop_view(&drag_evaluations).into_element();
    assert_eq!(
        drag_evaluations.into_inner(),
        [
            "preview",
            "row move",
            "payload",
            "tab move",
            "predicate",
            "row style",
            "row drop",
            "file style",
            "file drop",
        ]
    );
}

#[test]
fn extended_native_event_handlers_keep_their_exact_public_types() {
    let mouse_down = gpui_vue::ui::type_mouse_down_handler(native_mouse_down_out);
    let focus = gpui_vue::ui::boxed_focus_handler(native_focus);
    let blur = gpui_vue::ui::boxed_focus_handler(native_blur);
    let _ = (mouse_down, focus, blur);
}

#[test]
fn dynamic_class_condition_is_evaluated_once() {
    let condition_calls = Cell::new(0);
    let _element = build_merged_dynamic_class_view(&condition_calls).into_element();
    assert_eq!(condition_calls.get(), 1);
}

#[test]
fn repeated_macro_component_sites_have_distinct_factory_types() {
    let ids = same_span_component_type_ids!("first".to_owned(), "second".to_owned());
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}
