//! Executable source of truth for the `VitePress` guide examples.
#![allow(
    dead_code,
    reason = "some source regions are compiled documentation fixtures"
)]
#![allow(
    clippy::missing_docs_in_private_items,
    clippy::no_effect_underscore_binding,
    reason = "documentation snippets stay compact and component macro binders keep fixed semantic positions"
)]

use std::{any::Any, path::PathBuf, time::Duration};

use gpui_vue::animation::{Animation, AnimationExt, easing};
use gpui_vue::desktop::{
    AnyWindowHandle, DesktopApp, DesktopResult, WindowConfig, open_component_window,
};
use gpui_vue::effects::{EffectScope, next_frame, watch_entity};
use gpui_vue::media::ObjectFit;
use gpui_vue::prelude::*;
use gpui_vue::state::{global, provide_global};
use gpui_vue::ui::{
    App, Entity, FocusHandle, IntoElement, ScreenPoint, SharedString, StyleRefinement, Window, px,
    rgb,
};

// #region status_view
component! {
    /// Compact status presentation used by the template syntax guide.
    component StatusView {
        props {
            /// Label displayed beside the status dot.
            label: SharedString,
            /// Whether the status is healthy.
            online: bool = true,
        }

        template(this, _window, _cx) {
            <div class="flex flex-row items-center gap-2 rounded-lg bg-slate-900 px-3 py-2">
                <span :class={if this.props().online {
                    "h-2 w-2 rounded-full bg-emerald-400"
                } else {
                    "h-2 w-2 rounded-full bg-rose-400"
                }} />
                <text class="text-sm text-slate-200">{this.props().label.clone()}</text>
            </div>
        }
    }
}
// #endregion status_view

// #region local_counter
component! {
    /// Counter backed by allocation-free component-local state.
    component LocalCounter {
        state {
            /// Number shown by the control.
            count: Local<i32> = Local::new(0),
        }

        template(this, _window, cx) {
            <button
                id="gallery-local-counter"
                class="rounded-lg bg-blue-600 px-4 py-2 font-semibold text-white hover:bg-blue-500 active:bg-blue-700"
                @click={cx.listener(|this, _, _, cx| {
                    this.count.update(|count| count + 1, cx);
                })}
            >
                {format!("計數：{}", this.count.get())}
            </button>
        }
    }
}
// #endregion local_counter

// #region square_counter
component! {
    /// Counter whose derived square is cached by an explicit revision key.
    component SquareCounter {
        state {
            /// Source value.
            count: Local<i32> = Local::new(3),
            /// Explicitly keyed derived-value cache.
            square: Memo<i32> = Memo::new(),
        }

        template(this, _window, cx) {
            let revision = this.count.revision();
            let count = this.count.get();
            let square = *this.square.get_or_update(revision, || count * count);

            view! { <button
                id="gallery-square-counter"
                class="rounded-lg border border-slate-700 bg-slate-900 px-4 py-2 text-slate-100 hover:bg-slate-800"
                @click={cx.listener(|this, _, _, cx| {
                    this.count.update(|count| count + 1, cx);
                })}
            >
                {format!("{count}² = {square}")}
            </button> }
        }
    }
}
// #endregion square_counter

// #region style_gallery
component! {
    /// Demonstrates compile-time classes and one dynamic class branch.
    component StyleGallery {
        state {
            /// Whether the primary treatment is selected.
            selected: Local<bool> = Local::new(true),
        }

        template(this, _window, cx) {
            <button
                id="gallery-style-toggle"
                class="rounded-xl px-4 py-3 font-semibold text-white"
                :class={if this.selected.get() {
                    "bg-violet-600 hover:bg-violet-500"
                } else {
                    "bg-slate-700 hover:bg-slate-600"
                }}
                @click={cx.listener(|this, _, _, cx| {
                    this.selected.update(|selected| !*selected, cx);
                })}
            >
                {if this.selected.get() { "已選取" } else { "未選取" }}
            </button>
        }
    }
}
// #endregion style_gallery

// #region result_panel
component! {
    /// Shows that conditional branches build ordinary native elements.
    component ResultPanel {
        state {
            /// Whether the simulated result is available.
            ready: Local<bool> = Local::new(false),
        }

        template(this, _window, cx) {
            <div class="flex flex-col gap-2">
                <button
                    id="gallery-result-toggle"
                    class="rounded-lg border border-slate-700 px-3 py-2 text-slate-200 hover:bg-slate-800"
                    @click={cx.listener(|this, _, _, cx| {
                        this.ready.update(|ready| !*ready, cx);
                    })}
                >
                    "切換結果"
                </button>
                <text v-if={this.ready.get()} class="text-sm text-emerald-400">"結果已就緒"</text>
                <text v-else class="text-sm text-slate-500">"等待資料"</text>
            </div>
        }
    }
}
// #endregion result_panel

// #region layer_list
component! {
    /// Renders a keyed native list in model order.
    component LayerList {
        template(_this, _window, _cx) {
            let layers = [(1_u64, "背景"), (2, "輪廓"), (3, "節點")];
            view! { <div class="flex flex-col gap-1">
                <div
                    v-for={(id, name) in layers}
                    :key={("layer", id)}
                    class="flex flex-row items-center justify-between rounded bg-slate-900 px-3 py-2"
                >
                    <text class="text-sm text-slate-200">{name}</text>
                    <text class="text-xs text-slate-500">{format!("#{id:02}")}</text>
                </div>
            </div> }
        }
    }
}
// #endregion layer_list

// #region click_counter
component! {
    /// A typed native click listener with stable element identity.
    component ClickCounter {
        state {
            /// Number of accepted clicks.
            clicks: Local<usize> = Local::new(0),
        }

        template(this, _window, cx) {
            <button
                id="gallery-click-counter"
                class="rounded-lg bg-emerald-700 px-4 py-2 font-semibold text-white hover:bg-emerald-600"
                @click={cx.listener(|this, _, _, cx| {
                    this.clicks.update(|clicks| clicks + 1, cx);
                })}
            >
                {format!("點擊 {} 次", this.clicks.get())}
            </button>
        }
    }
}
// #endregion click_counter

// #region search_panel
component! {
    /// Native IME-aware single-line search field.
    component SearchPanel {
        state {
            /// Retained input entity.
            input: TextInputHandle = text_input("輸入中文、日文或韓文", cx),
            /// Canonical parent-owned value.
            query: Local<String> = Local::new(String::new()),
            /// Two-way synchronization retained for the component lifetime.
            input_binding: Option<TextModelBinding> = None,
        }

        mounted(this, _window, cx) {
            this.input_binding = Some(TextModelBinding::bind(
                &this.input,
                this.query.get(),
                cx,
                |this| this.query.get(),
                |this, value, cx| {
                    this.query.set(value, cx);
                },
            ));
        }

        template(this, _window, _cx) {
            let input = this.input.clone();
            view! { <div class="flex flex-col gap-2">
                {input}
                <text class="text-xs text-slate-500">{format!("目前內容：{}", this.query.get())}</text>
            </div> }
        }
    }
}
// #endregion search_panel

// #region configured_text_input
component! {
    /// Styled, bounded read-only field built from first-class input config.
    component ConfiguredTextInputDemo {
        state {
            /// Retained configured input entity.
            input: TextInputHandle = text_input_with_config(
                TextInputConfig::new("沒有內容")
                    .value("唯讀的原生欄位")
                    .read_only(true)
                    .max_length(12)
                    .style(
                        TextInputStyle::default()
                            .height(px(36.0))
                            .padding_x(px(11.0))
                            .background_color(rgb(0x17_20_33))
                            .border_color(rgb(0x33_41_55))
                            .focus_border_color(rgb(0x60_a5_fa))
                            .text_color(rgb(0xdb_ea_fe))
                            .placeholder_color(rgb(0x64_74_8b))
                            .selection_color(rgb(0x1d_4e_d8))
                            .caret_color(rgb(0x93_c5_fd))
                            .corner_radius(px(8.0))
                            .font_size(px(14.0)),
                    ),
                cx,
            ),
        }

        template(this, _window, _cx) {
            let input = this.input.clone();
            view! { <div class="flex flex-col gap-2">
                {input}
                <text class="text-xs text-slate-500">"可選取與複製；不能修改"</text>
            </div> }
        }
    }
}
// #endregion configured_text_input

// #region focus_demo
component! {
    /// Focusable keyboard surface with an explicit native focus handle.
    component FocusDemo {
        state {
            /// Focus identity tracked by the native host.
            focus: FocusHandle = cx.focus_handle(),
            /// Last key received by the surface.
            last_key: String = String::from("尚未輸入"),
        }

        template(this, _window, cx) {
            <div
                id="gallery-focus-demo"
                :track-focus={&this.focus}
                key-context="DocsGallery"
                class="rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-300 focus:border-blue-500"
                @key-down={cx.listener(|this, event: &gpui_vue::ui::KeyDownEvent, _window, cx| {
                    this.last_key.clone_from(&event.keystroke.key);
                    cx.notify();
                })}
            >
                {format!("鍵盤焦點：{}", this.last_key)}
            </div>
        }
    }
}
// #endregion focus_demo

// #region typed_props
component! {
    /// Card with one required prop and one defaulted prop.
    component ComponentCard {
        props {
            /// Primary card label.
            label: SharedString,
            /// Secondary status text.
            tone: SharedString = "Native".into(),
        }

        template(this, _window, _cx) {
            <div class="flex flex-col gap-1 rounded-xl border border-slate-800 bg-slate-900 p-4">
                <text class="font-semibold text-slate-100">{this.props().label.clone()}</text>
                <text class="text-xs text-blue-400">{this.props().tone.clone()}</text>
            </div>
        }
    }
}
// #endregion typed_props

// #region typed_events
component! {
    /// Child component that emits one typed save event.
    component SaveButton {
        emits {
            /// Reports the saved document name.
            saved(name: SharedString);
        }

        template(_this, _window, cx) {
            <button
                id="gallery-save"
                class="rounded-lg bg-blue-600 px-4 py-2 font-semibold text-white hover:bg-blue-500"
                @click={cx.listener(|_this, _, _, cx| {
                    SaveButton::emit_saved("glyph.kage".into(), cx);
                })}
            >
                "儲存"
            </button>
        }
    }
}
// #endregion typed_events

/// Props passed into the named action slot.
#[derive(Clone, Copy)]
struct ActionSlotProps {
    /// Number of available actions.
    count: usize,
}

// #region typed_slots
component! {
    /// Panel with typed default and named scoped slots.
    component SlotPanel {
        slots {
            /// Main panel content.
            default: ();
            /// Action content that receives the current count.
            actions: ActionSlotProps;
        }

        template(_this, _window, _cx) {
            <div class="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-900 p-4">
                <slot><text class="text-slate-500">"沒有內容"</text></slot>
                <slot name="actions" :props={ActionSlotProps { count: 2 }} />
            </div>
        }
    }
}
// #endregion typed_slots

// #region lifecycle_probe
component! {
    /// Displays lifecycle work that is tied to a visual component host.
    component LifecycleProbe {
        state {
            /// Human-readable lifecycle state.
            phase: SharedString = "constructed".into(),
        }

        mounted(this, _window, cx) {
            this.phase = "mounted".into();
            cx.notify();
        }

        template(this, _window, _cx) {
            <text class="rounded bg-slate-900 px-3 py-2 text-sm text-amber-400">
                {format!("Lifecycle: {}", this.phase)}
            </text>
        }
    }
}
// #endregion lifecycle_probe

// #region manual_observation
/// Retains a model observer and redraws the owner after model notifications.
pub fn retain_model_observer<Owner: 'static, Model: 'static>(
    effects: &mut EffectScope,
    model: &gpui_vue::ui::Entity<Model>,
    cx: &mut gpui_vue::ui::Context<'_, Owner>,
) {
    effects.track(watch_entity(cx, model, |_owner, _model, cx| {
        cx.notify();
    }));
}
// #endregion manual_observation

// #region explicit_attrs
component! {
    /// Native attributes are explicit typed props rather than a DOM bag.
    component ExplicitRow {
        props {
            /// Row label.
            label: SharedString,
            /// Whether the row uses the selected treatment.
            selected: bool = false,
        }

        template(this, _window, _cx) {
            <div :class={if this.props().selected {
                "rounded bg-blue-600 px-3 py-2 text-white"
            } else {
                "rounded bg-slate-900 px-3 py-2 text-slate-300"
            }}>
                {this.props().label.clone()}
            </div>
        }
    }
}
// #endregion explicit_attrs

// #region registration_demo
component! {
    /// Rust module visibility replaces a runtime component registry.
    component RegistrationDemo {
        template(_this, _window, _cx) {
            <ComponentCard label={"由 Rust 型別解析".into()} tone={"No registry".into()} />
        }
    }
}
// #endregion registration_demo

// #region controlled_field
/// Applies one controlled value without synthesizing a user-change event.
pub fn set_controlled_text(
    input: &TextInputHandle,
    value: impl Into<String>,
    cx: &mut gpui_vue::ui::App,
) {
    input.update(cx, |input, cx| input.set_text(value, cx));
}
// #endregion controlled_field

// #region component_card
component! {
    /// Composes the typed examples into one visible result card.
    component ComponentCardDemo {
        state {
            /// Latest child event shown by the parent.
            saved: SharedString = "尚未儲存".into(),
        }

        template(this, _window, cx) {
            <div class="flex flex-col gap-3">
                <ComponentCard label={"Typed component".into()} />
                <SaveButton @saved={cx.listener(|this, event: &SaveButtonEvent, _window, cx| {
                    let SaveButtonEvent::Saved { name } = event;
                    this.saved = format!("已儲存 {name}").into();
                    cx.notify();
                })} />
                <text class="text-xs text-slate-500">{this.saved.clone()}</text>
            </div>
        }
    }
}
// #endregion component_card

// #region status_badge
component! {
    /// Reusable status badge kept in one Rust module.
    component StatusBadge {
        props {
            /// Visible status label.
            label: SharedString,
        }

        template(this, _window, _cx) {
            <text class="rounded-full bg-emerald-950 px-3 py-1 text-xs font-semibold text-emerald-300">
                {this.props().label.clone()}
            </text>
        }
    }
}
// #endregion status_badge

// #region typed_toggle_demo
/// Toggles one [`Local`] value and reports whether it changed.
fn toggle_local(value: &mut Local<bool>, notifier: &mut impl ChangeNotifier) -> bool {
    value.update(|value| !*value, notifier)
}
// #endregion typed_toggle_demo

// #region typed_stepper
component! {
    /// Component whose generated state and event API remain statically typed.
    component TypedStepper {
        state {
            /// Current value.
            value: Local<i32> = Local::new(0),
        }

        emits {
            /// Reports a new stepper value.
            changed(value: i32);
        }

        template(this, _window, cx) {
            <button id="typed-stepper" class="rounded bg-blue-600 px-3 py-2 text-white" @click={cx.listener(
                |this, _, _, cx| {
                    this.value.update(|value| value + 1, cx);
                    TypedStepper::emit_changed(this.value.get(), cx);
                }
            )}>{format!("Step {}", this.value.get())}</button>
        }
    }
}
// #endregion typed_stepper

// #region typed_props_demo
fn build_card_props() -> ComponentCardProps {
    ComponentCardProps::builder()
        .label("Compile-time props".into())
        .tone("Rust typestate".into())
        .build()
}
// #endregion typed_props_demo

// #region inline_panel_demo
fn inline_panel() -> impl IntoElement {
    view! {
        <div class="rounded-xl border border-slate-800 bg-slate-900 p-4">
            <text class="text-sm text-slate-200">"Incremental view! adoption"</text>
        </div>
    }
}
// #endregion inline_panel_demo

// #region render_helper_demo
fn empty_state(label: impl Into<SharedString>) -> impl IntoElement {
    let label = label.into();
    view! {
        <div class="flex items-center justify-center rounded-lg border border-slate-800 p-6">
            <text class="text-sm text-slate-500">{label}</text>
        </div>
    }
}
// #endregion render_helper_demo

// #region memo_demo
fn formatted_total<'cache>(
    cache: &'cache mut Memo<String, Revision>,
    count: &Local<i32>,
) -> &'cache str {
    cache
        .get_or_update(count.revision(), || format!("Total: {}", count.get()))
        .as_str()
}
// #endregion memo_demo

// #region effect_scope_demo
fn clear_screen_effects(effects: &mut EffectScope) {
    effects.clear();
}
// #endregion effect_scope_demo

/// Application-wide theme used by the global-state examples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GalleryTheme {
    /// Whether the dark palette is active.
    dark: bool,
}

impl Global for GalleryTheme {}

// #region theme_global_demo
fn install_gallery_theme(app: &mut gpui_vue::ui::App) {
    provide_global(app, GalleryTheme { dark: true });
}

fn gallery_is_dark(app: &gpui_vue::ui::App) -> bool {
    global::<GalleryTheme>(app).dark
}
// #endregion theme_global_demo

// #region app_global_state
fn provide_application_state(app: &mut gpui_vue::ui::App) {
    provide_global(app, GalleryTheme { dark: true });
}
// #endregion app_global_state

// #region notification_demo
#[derive(Default)]
struct DocumentModel {
    title: String,
}

fn rename_document(model: &gpui_vue::ui::Entity<DocumentModel>, app: &mut gpui_vue::ui::App) {
    model.update(app, |model, cx| {
        model.title = "Edited glyph".into();
        cx.notify();
    });
}
// #endregion notification_demo

// #region deferred_content
component! {
    /// Shows content after one native frame without blocking render.
    component DeferredContent {
        state {
            /// Whether the deferred callback ran.
            ready: Local<bool> = Local::new(false),
        }

        mounted(_this, window, cx) {
            next_frame(cx, window, |this, _window, cx| {
                this.ready.set(true, cx);
            });
        }

        template(this, _window, _cx) {
            <text class="text-sm text-slate-300">
                {if this.ready.get() { "內容已就緒" } else { "載入中…" }}
            </text>
        }
    }
}
// #endregion deferred_content

// #region async_resource_demo
component! {
    /// Owner-held asynchronous data with cancellation and stale-result protection.
    component AsyncResourceDemo {
        state {
            /// Latest native task and its UI-facing state.
            resource: AsyncResource<SharedString> = AsyncResource::new(),
        }

        mounted(this, _window, cx) {
            let _ = this.resource.load(
                cx,
                |this| &mut this.resource,
                async |_cx| Ok::<_, String>("Initial resource loaded".into()),
            );
        }

        template(this, _window, cx) {
            let generation = this.resource.generation();
            let status = match this.resource.state() {
                AsyncState::Idle => "Idle".to_owned(),
                AsyncState::Loading => "Loading…".to_owned(),
                AsyncState::Ready(value) => value.to_string(),
                AsyncState::Error(error) => format!("Error: {error}"),
            };

            view! { <div class="flex flex-col gap-2">
                <text class="text-xs font-semibold text-slate-500">"ASYNC RESOURCE"</text>
                <text class="text-sm text-slate-200">{status}</text>
                <text class="text-xs text-slate-500">{format!("Generation {generation}")}</text>
                <button
                    id="gallery-async-reload"
                    class="rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 hover:bg-slate-800"
                    @click={cx.listener(|this, _, _, cx| {
                        let request = this.resource.generation() + 1;
                        this.resource.reload(
                            cx,
                            |this| &mut this.resource,
                            async move |_cx| {
                                Ok::<_, String>(format!("Reload {request} completed").into())
                            },
                        );
                    })}
                >
                    "Reload"
                </button>
            </div> }
        }
    }
}
// #endregion async_resource_demo

// #region overlay_demo
fn gallery_popup() -> impl IntoElement {
    deferred_overlay(
        anchored_overlay(view! {
            <div id="gallery-native-popup" occlude class="flex flex-col gap-1 rounded-lg border border-blue-500 bg-slate-900 px-3 py-2 text-sm text-slate-100 shadow-lg">
                <text class="font-semibold text-blue-300">"Native popup layer"</text>
                <text class="text-xs text-slate-400">"Anchored, fitted, then deferred"</text>
            </div>
        })
        .anchor(OverlayCorner::TopLeft)
        .offset_xy(8.0, 8.0)
        .snap_to_window_with_margin(12.0),
    )
    .priority(10)
}

component! {
    /// Interactive popup built from gpui-vue's real anchored/deferred bridge.
    component OverlayDemo {
        state {
            /// Whether the floating subtree is mounted.
            open: Local<bool> = Local::new(false),
        }

        template(this, _window, cx) {
            <div class="flex flex-col gap-2">
                <text class="text-xs font-semibold text-slate-500">"NATIVE OVERLAY"</text>
                <button
                    id="gallery-overlay-toggle"
                    class="rounded-lg bg-blue-600 px-3 py-2 text-sm font-semibold text-white hover:bg-blue-500"
                    @click={cx.listener(|this, _, _, cx| {
                        this.open.update(|open| !*open, cx);
                    })}
                >
                    {if this.open.get() { "Hide popup" } else { "Show popup" }}
                </button>
                <div v-if={this.open.get()}>{gallery_popup()}</div>
            </div>
        }
    }
}
// #endregion overlay_demo

// #region drag_drop_demo
/// Exact Rust value retained for the lifetime of one native drag.
struct GalleryDragPayload {
    glyph: SharedString,
}

component! {
    /// Retained entity rendered by GPUI as the drag preview.
    component GalleryDragPreview {
        props {
            /// Glyph label copied from the borrowed drag payload.
            glyph: SharedString,
        }

        template(this, _window, _cx) {
            <div class="rounded-lg border border-blue-400 bg-slate-800 px-3 py-2 shadow-lg">
                <text class="text-sm font-semibold text-blue-200">
                    {format!("Dragging {}", this.props().glyph)}
                </text>
            </div>
        }
    }
}

fn gallery_can_drop(payload: &dyn Any, _: &mut Window, _: &mut App) -> bool {
    payload.is::<GalleryDragPayload>()
}

fn gallery_drag_over(
    style: StyleRefinement,
    _: &GalleryDragPayload,
    _: &mut Window,
    _: &mut App,
) -> StyleRefinement {
    style.bg(rgb(0x17_2f_55)).border_color(rgb(0x60_a5_fa))
}

component! {
    /// Visible typed drag/drop round trip backed by native GPUI interaction.
    component DragDropDemo {
        state {
            /// Human-readable result of the latest accepted drop.
            last_drop: Local<SharedString> = Local::new("No glyph dropped yet".into()),
        }

        template(this, _window, cx) {
            <div class="flex flex-col gap-3">
                <div class="flex flex-col gap-1">
                    <text class="text-xs font-semibold text-slate-500">"TYPED DRAG & DROP"</text>
                    <text class="text-xs text-slate-400">"Drag the glyph payload into the native drop target."</text>
                </div>
                <div class="grid grid-cols-2 gap-3">
                    <div
                        id="gallery-drag-source"
                        class="flex min-h-24 cursor-grab flex-col items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 active:cursor-grabbing"
                        :drag-payload={GalleryDragPayload { glyph: "永 · u6c38".into() }}
                        :drag-preview={|payload: &GalleryDragPayload,
                                        _offset: ScreenPoint,
                                        _window: &mut Window,
                                        cx: &mut App|
                         -> Entity<GalleryDragPreview> {
                            GalleryDragPreview::new(
                                GalleryDragPreviewProps::new(payload.glyph.clone()),
                                cx,
                            )
                        }}
                    >
                        <text class="text-2xl font-bold text-slate-100">"永"</text>
                        <text class="text-xs text-slate-500">"Typed payload · drag me"</text>
                    </div>
                    <div
                        id="gallery-drop-target"
                        class="flex min-h-24 flex-col items-center justify-center gap-1 rounded-lg border border-dashed border-slate-600 bg-slate-950 px-4 py-3"
                        :can-drop={gallery_can_drop}
                        :drag-over={gallery_drag_over}
                        @drop={cx.listener(|this, payload: &GalleryDragPayload, _, cx| {
                            this.last_drop.set(
                                format!("Dropped {} successfully", payload.glyph).into(),
                                cx,
                            );
                        })}
                    >
                        <text class="text-sm font-semibold text-slate-200">"Drop target"</text>
                        <text class="text-xs text-slate-500">"Accepts GalleryDragPayload only"</text>
                    </div>
                </div>
                <text class="text-xs text-blue-300">{this.last_drop.get().clone()}</text>
            </div>
        }
    }
}
// #endregion drag_drop_demo

// #region desktop_window_demo
component! {
    /// Small generated component mounted as a real secondary window root.
    component SecondaryGalleryWindow {
        template(_this, _window, cx) {
            <div class="h-full w-full flex flex-col items-center justify-center gap-4 bg-slate-950 p-8 text-white">
                <StatusBadge label={"SECONDARY WINDOW".into()} />
                <text class="text-xl font-semibold text-slate-100">"A second native component window"</text>
                <text class="text-sm text-slate-400">"It uses the same retained lifecycle host as the main gallery."</text>
                <button
                    id="gallery-secondary-close"
                    class="rounded-lg border border-slate-700 bg-slate-900 px-4 py-2 text-slate-100 hover:bg-slate-800"
                    @click={cx.listener(|_this, _, window, _cx| window.remove_window())}
                >
                    "Close window"
                </button>
            </div>
        }
    }
}

fn open_secondary_gallery_window(app: &mut gpui_vue::ui::App) -> DesktopResult<AnyWindowHandle> {
    open_component_window(
        app,
        WindowConfig::new("gpui-vue Secondary Window", 520.0, 320.0)
            .min_size(420.0, 260.0)
            .transparent_titlebar(true),
        |_window, app| SecondaryGalleryWindow::new(SecondaryGalleryWindowProps::new(), app),
    )
}

component! {
    /// Opens an additional generated component without reaching through GPUI.
    component DesktopWindowDemo {
        state {
            /// Result of the latest native window creation request.
            result: SharedString = "Ready to open".into(),
        }

        template(this, _window, cx) {
            <div class="flex flex-col gap-2">
                <text class="text-xs font-semibold text-slate-500">"MULTI-WINDOW"</text>
                <text class="text-sm text-slate-300">{this.result.clone()}</text>
                <button
                    id="gallery-open-secondary"
                    class="rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 hover:bg-slate-800"
                    @click={cx.listener(|this, _, _, cx| {
                        this.result = match open_secondary_gallery_window(cx) {
                            Ok(_) => "Secondary window opened".into(),
                            Err(error) => format!("Could not open: {error}").into(),
                        };
                        cx.notify();
                    })}
                >
                    "Open secondary window"
                </button>
            </div>
        }
    }
}
// #endregion desktop_window_demo

// #region load_state_demo
#[derive(Clone, Debug, Eq, PartialEq)]
enum LoadState<T> {
    Idle,
    Loading,
    Ready(T),
    Error(String),
}

fn load_state_label(state: &LoadState<String>) -> &str {
    match state {
        LoadState::Idle => "尚未開始",
        LoadState::Loading => "載入中…",
        LoadState::Ready(value) => value,
        LoadState::Error(message) => message,
    }
}
// #endregion load_state_demo

// #region route_view_demo
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Dashboard,
    Settings,
}

fn route_label(route: Route) -> &'static str {
    match route {
        Route::Dashboard => "Dashboard",
        Route::Settings => "Settings",
    }
}
// #endregion route_view_demo

// #region untrusted_text_demo
fn untrusted_text(value: String) -> impl IntoElement {
    view! { <text class="text-sm text-slate-200">{value}</text> }
}
// #endregion untrusted_text_demo

// #region keyboard_action_demo
component! {
    /// Keyboard-operable native control with stable focus identity.
    component KeyboardActionDemo {
        state {
            /// Focus handle used for native keyboard dispatch.
            focus: FocusHandle = cx.focus_handle(),
        }

        template(this, _window, _cx) {
            <button id="keyboard-action" :track-focus={&this.focus} class="rounded bg-blue-600 px-3 py-2 text-white">
                "可用 Tab 與 Enter 操作"
            </button>
        }
    }
}
// #endregion keyboard_action_demo

// #region root_overlay_demo
component! {
    /// Root-owned modal layer that blocks pointer input behind it.
    component RootOverlayDemo {
        template(_this, _window, _cx) {
            <div id="overlay-scrim" occlude class="absolute inset-0 flex items-center justify-center bg-black/60">
                <div id="overlay-panel" occlude class="rounded-xl bg-slate-900 p-5 text-white">
                    "Native overlay"
                </div>
            </div>
        }
    }
}
// #endregion root_overlay_demo

// #region keyed_queue_demo
component! {
    /// Stable keys preserve each queued row's native identity.
    component KeyedQueueDemo {
        template(_this, _window, _cx) {
            let jobs = [(7_u64, "Parse"), (8, "Render"), (9, "Export")];
            view! { <div class="flex flex-col gap-1">
                <div v-for={(id, label) in jobs} :key={("job", id)} class="rounded bg-slate-900 px-3 py-2 text-slate-200">
                    {label}
                </div>
            </div> }
        }
    }
}
// #endregion keyed_queue_demo

// #region native_fade_demo
fn native_fade() -> impl IntoElement {
    view! { <div class="h-11 w-44 rounded-lg bg-blue-600" /> }.with_animation(
        "docs-native-fade",
        Animation::new(Duration::from_millis(180)).with_easing(easing::ease_in_out),
        Styled::opacity,
    )
}
// #endregion native_fade_demo

// #region animation_demo
fn pulsing_square() -> impl IntoElement {
    view! { <div class="h-8 w-8 rounded-lg bg-blue-500" /> }.with_animation(
        "docs-pulse",
        Animation::new(Duration::from_millis(900))
            .repeat()
            .with_easing(easing::ease_in_out),
        |element, delta| element.opacity(0.35 + delta * 0.65),
    )
}
// #endregion animation_demo

// #region image_bindings_demo
component! {
    /// Native image pipeline with typed fit and lazy state replacements.
    component ImageBindingsDemo {
        template(_this, _window, _cx) {
            let icon = PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/kage_editor/assets/kage-editor-icon.png",
            ));

            view! { <div class="flex flex-col gap-2">
                <text class="text-xs font-semibold text-slate-500">"TYPED IMAGE STATES"</text>
                <img
                    :src={icon}
                    :object-fit={ObjectFit::Contain}
                    :loading={|| view! {
                        <div class="size-full rounded-lg bg-slate-800" />
                    }.into_any_element()}
                    :fallback={|| view! {
                        <div class="size-full flex items-center justify-center rounded-lg border border-rose-700 text-rose-300">
                            "Image unavailable"
                        </div>
                    }.into_any_element()}
                    class="h-24 w-full rounded-lg bg-slate-900"
                />
            </div> }
        }
    }
}
// #endregion image_bindings_demo

component! {
    /// Screenshot fixture that mounts only the local-state example.
    component LocalCounterFixture {
        template(_this, _window, _cx) {
            <view class="h-full w-full flex items-center justify-center bg-slate-950 p-8 text-white">
                <div class="w-full rounded-xl border border-slate-800 bg-slate-950 p-4">
                    <LocalCounter />
                </div>
            </view>
        }
    }
}

component! {
    /// Screenshot fixture that mounts only the template example.
    component StatusViewFixture {
        template(_this, _window, _cx) {
            <view class="h-full w-full flex items-center justify-center bg-slate-950 p-8 text-white">
                <div class="w-full rounded-xl border border-slate-800 bg-slate-950 p-4">
                    <StatusView label={"Renderer connected".into()} />
                </div>
            </view>
        }
    }
}

component! {
    /// Screenshot fixture that mounts only the component example.
    component ComponentCardFixture {
        template(_this, _window, _cx) {
            <view class="h-full w-full flex items-center justify-center bg-slate-950 p-8 text-white">
                <div class="w-full rounded-xl border border-slate-800 bg-slate-950 p-4">
                    <ComponentCardDemo />
                </div>
            </view>
        }
    }
}

component! {
    /// Screenshot fixture that mounts only the keyed-list example.
    component LayerListFixture {
        template(_this, _window, _cx) {
            <view class="h-full w-full flex items-center justify-center bg-slate-950 p-8 text-white">
                <div class="w-full rounded-xl border border-slate-800 bg-slate-950 p-4">
                    <LayerList />
                </div>
            </view>
        }
    }
}

component! {
    /// Screenshot fixture that mounts only the IME-aware input example.
    component SearchPanelFixture {
        template(_this, _window, _cx) {
            <view class="h-full w-full flex items-center justify-center bg-slate-950 p-8 text-white">
                <div class="w-full rounded-xl border border-slate-800 bg-slate-950 p-4">
                    <SearchPanel />
                </div>
            </view>
        }
    }
}

component! {
    /// Screenshot fixture that mounts only the registration example.
    component RegistrationFixture {
        template(_this, _window, _cx) {
            <view class="h-full w-full flex items-center justify-center bg-slate-950 p-8 text-white">
                <div class="w-full rounded-xl border border-slate-800 bg-slate-950 p-4">
                    <RegistrationDemo />
                </div>
            </view>
        }
    }
}

// #region app_root
component! {
    /// Executable gallery shown by the documentation screenshots.
    component DocsGallery {
        template(_this, _window, _cx) {
            <view id="docs-gallery-scroll" class="h-full w-full flex flex-col gap-5 overflow-y-scroll bg-slate-950 p-8 text-white">
                <div class="flex flex-col gap-1">
                    <text class="text-3xl font-bold">"gpui-vue Guide Gallery"</text>
                    <text class="text-sm text-slate-400">"The code and the visible result share one compiled source."</text>
                </div>

                <div class="grid grid-cols-3 gap-4">
                    <div class="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <text class="text-xs font-semibold text-slate-500">"STATE"</text>
                        <LocalCounter />
                        <SquareCounter />
                        <ClickCounter />
                    </div>
                    <div class="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <text class="text-xs font-semibold text-slate-500">"TEMPLATES"</text>
                        <StatusView label={"Renderer connected".into()} />
                        <StyleGallery />
                        <ResultPanel />
                    </div>
                    <div class="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <text class="text-xs font-semibold text-slate-500">"COMPONENTS"</text>
                        <ComponentCardDemo />
                        <LifecycleProbe />
                    </div>
                    <div class="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <text class="text-xs font-semibold text-slate-500">"LISTS"</text>
                        <LayerList />
                    </div>
                    <div class="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <text class="text-xs font-semibold text-slate-500">"NATIVE INPUT"</text>
                        <SearchPanel />
                        <ConfiguredTextInputDemo />
                        <FocusDemo />
                    </div>
                    <div class="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <text class="text-xs font-semibold text-slate-500">"COMPOSITION"</text>
                        <RegistrationDemo />
                        <SlotPanel>
                            <text class="text-sm text-slate-200">"由 parent 延遲渲染"</text>
                            <template #actions={ActionSlotProps { count }}>
                                <text class="text-xs text-blue-400">{format!("{count} 個操作")}</text>
                            </template>
                        </SlotPanel>
                    </div>
                </div>

                <div class="flex flex-col gap-3">
                    <text class="text-xs font-semibold text-slate-500">"ADVANCED NATIVE PRIMITIVES"</text>
                    <div class="flex flex-row items-center gap-4 rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <StatusBadge label={"Native animation".into()} />
                        <TypedStepper />
                        {native_fade()}
                        {pulsing_square()}
                        {inline_panel()}
                    </div>
                    <div class="grid grid-cols-3 gap-4">
                        <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                            <KeyedQueueDemo />
                        </div>
                        <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                            <DeferredContent />
                        </div>
                        <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                            <KeyboardActionDemo />
                        </div>
                    </div>
                    <div class="grid grid-cols-3 gap-4">
                        <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                            <AsyncResourceDemo />
                        </div>
                        <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                            <OverlayDemo />
                        </div>
                        <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                            <DesktopWindowDemo />
                        </div>
                    </div>
                    <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <DragDropDemo />
                    </div>
                    <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <ImageBindingsDemo />
                    </div>
                    <div class="rounded-xl border border-slate-800 bg-slate-950 p-4">
                        <text class="text-xs font-semibold text-slate-500">"VERIFIED DERIVED RESULTS"</text>
                        <div class="grid grid-cols-3 gap-3 pt-3">
                            <text class="text-sm text-slate-300">{format!("Theme global: {}", gallery_is_dark(_cx))}</text>
                            <text class="text-sm text-slate-300">{format!("Route: {}", route_label(Route::Dashboard))}</text>
                            <text class="text-sm text-slate-300">{format!("Async state: {}", load_state_label(&LoadState::Ready("Ready".into())))}</text>
                            {empty_state("沒有搜尋結果")}
                            {untrusted_text("<script> stays text".into())}
                            <ComponentCard label={build_card_props().label} tone={build_card_props().tone} />
                        </div>
                    </div>
                </div>
            </view>
        }
    }
}

fn main() {
    let fixture = std::env::args().nth(1);

    match fixture.as_deref() {
        Some("local-counter") => DesktopApp::new(fixture_window("Local state", 560.0, 260.0))
            .plugin(install_gallery_theme as fn(&mut gpui_vue::ui::App))
            .run_component(|_, cx| LocalCounterFixture::new(LocalCounterFixtureProps::new(), cx)),
        Some("status-view") => DesktopApp::new(fixture_window("Template", 560.0, 260.0))
            .plugin(install_gallery_theme as fn(&mut gpui_vue::ui::App))
            .run_component(|_, cx| StatusViewFixture::new(StatusViewFixtureProps::new(), cx)),
        Some("component-card") => {
            DesktopApp::new(fixture_window("Component", 560.0, 380.0))
                .plugin(install_gallery_theme as fn(&mut gpui_vue::ui::App))
                .run_component(|_, cx| {
                    ComponentCardFixture::new(ComponentCardFixtureProps::new(), cx)
                });
        }
        Some("layer-list") => DesktopApp::new(fixture_window("Keyed list", 560.0, 360.0))
            .plugin(install_gallery_theme as fn(&mut gpui_vue::ui::App))
            .run_component(|_, cx| LayerListFixture::new(LayerListFixtureProps::new(), cx)),
        Some("search-panel") => {
            DesktopApp::new(fixture_window("Native input", 640.0, 320.0))
                .plugin(install_gallery_theme as fn(&mut gpui_vue::ui::App))
                .run_component(|_, cx| SearchPanelFixture::new(SearchPanelFixtureProps::new(), cx));
        }
        Some("registration") => {
            DesktopApp::new(fixture_window("Registration", 560.0, 300.0))
                .plugin(install_gallery_theme as fn(&mut gpui_vue::ui::App))
                .run_component(|_, cx| {
                    RegistrationFixture::new(RegistrationFixtureProps::new(), cx)
                });
        }
        Some(unknown) => panic!(
            "unknown docs_gallery fixture `{unknown}`; expected local-counter, status-view, component-card, layer-list, search-panel, or registration"
        ),
        None => {
            let window = WindowConfig::new("gpui-vue Guide Gallery", 1180.0, 820.0)
                .min_size(900.0, 680.0)
                .transparent_titlebar(true);

            DesktopApp::new(window)
                .plugin(install_gallery_theme as fn(&mut gpui_vue::ui::App))
                .run_component(|_, cx| DocsGallery::new(DocsGalleryProps::new(), cx));
        }
    }
}
// #endregion app_root

fn fixture_window(title: &str, width: f32, height: f32) -> WindowConfig {
    WindowConfig::new(format!("gpui-vue · {title}"), width, height).transparent_titlebar(true)
}
