//! A native, professional KAGE Editor built with `gpui-vue`.
//!
//! Run from the workspace with `cargo run -p kage-editor`, or run plain
//! `cargo run` from this package directory.

#![allow(
    clippy::assigning_clones,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collapsible_match,
    clippy::missing_docs_in_private_items,
    clippy::needless_pass_by_value,
    clippy::needless_question_mark,
    clippy::no_effect_underscore_binding,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_self
)]

mod canvas;
mod controls;
mod engine;
mod glyphwiki;
mod i18n;
mod model;

use std::cell::Cell;
use std::f32::consts::FRAC_PI_2;
use std::rc::Rc;

use canvas::{CanvasOverlay, CanvasSnapshot, CanvasTransform, InteractionMode, ResizeHandle};
use controls::{
    inspector_row, section_header, separator, tool_button, toolbar_button, toolbar_icon_button,
};
use glyphwiki::{GlyphWikiClient, GlyphWikiHttpClient, SearchResponse, thumbnail_url};
use gpui_vue::desktop::{DesktopApp, WindowConfig};
use gpui_vue::prelude::*;
use gpui_vue::ui::{
    AnyElement, App, ClickEvent, Context, FocusHandle, IntoElement, KeyDownEvent, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PinchEvent, ScreenPoint, ScrollWheelEvent, SharedString,
    Subscription, Window, px, read_clipboard_text, write_clipboard_text,
};
use gpui_vue::{TextInputEvent, TextInputHandle, text_input};
use i18n::{UiText, text};
use model::{
    AffineTransform, CenterlineMode, ControlPointRef, DESIGN_SIZE, EditorModel, GestureKind,
    KageTransform, MaskMode, OrderDirection, Point, Rect, SelectionMode, StrokeId, StrokeKind,
    Typeface, UiLanguage, ValidationSeverity,
};

const MIN_ZOOM: f32 = 0.45;
const MAX_ZOOM: f32 = 3.2;
const APP_ICON_ASSET: &str = "icons/kage-editor.png";
const RANDOM_COMPONENT_COUNT: usize = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tool {
    Select,
    Freehand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SidebarTab {
    Inspector,
    Components,
    Layers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FramePoint {
    First,
    Second,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GridField {
    OriginX,
    OriginY,
    SpacingX,
    SpacingY,
}

/// Network-backed component-search state for the current submitted query.
#[derive(Clone, Debug, Default)]
enum ComponentSearchState {
    /// No explicit query has been submitted, so remote recommendations remain visible.
    #[default]
    Idle,
    /// A `GlyphWiki` request is in flight.
    Loading { query: String },
    /// `GlyphWiki` returned an ordered result list.
    Ready { query: String, names: Vec<String> },
    /// `GlyphWiki` asked for a more specific query.
    TooShort { query: String },
    /// `GlyphWiki` found no matching names.
    NoData { query: String },
    /// Networking failed; the last remote recommendation batch remains available.
    Error { query: String, message: String },
}

impl ComponentSearchState {
    /// Returns the query associated with a submitted search state.
    fn query(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Loading { query }
            | Self::Ready { query, .. }
            | Self::TooShort { query }
            | Self::NoData { query }
            | Self::Error { query, .. } => Some(query),
        }
    }
}

/// Startup and refresh state for the random `GlyphWiki` recommendations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum ComponentSuggestionsState {
    /// The root component has not started its first request yet.
    #[default]
    Idle,
    /// A new batch is loading; an earlier complete batch remains renderable.
    Loading { names: Vec<String> },
    /// One complete random batch is ready.
    Ready { names: Vec<String> },
    /// The request failed; an earlier complete batch remains as a fallback.
    Error { names: Vec<String>, message: String },
}

impl ComponentSuggestionsState {
    /// Returns the last atomically committed batch, if one exists.
    fn names(&self) -> &[String] {
        match self {
            Self::Idle => &[],
            Self::Loading { names } | Self::Ready { names } | Self::Error { names, .. } => names,
        }
    }
}

/// Retains the current batch while a refresh is in flight.
fn begin_component_suggestions(state: &mut ComponentSuggestionsState) {
    let names = state.names().to_vec();
    *state = ComponentSuggestionsState::Loading { names };
}

/// Commits only the newest complete recommendation batch.
fn complete_component_suggestions(
    state: &mut ComponentSuggestionsState,
    current_generation: u64,
    completed_generation: u64,
    result: Result<Vec<String>, String>,
) -> bool {
    if current_generation != completed_generation {
        return false;
    }

    let retained = state.names().to_vec();
    *state = match result {
        Ok(names) if names.len() == RANDOM_COMPONENT_COUNT => {
            ComponentSuggestionsState::Ready { names }
        }
        Ok(names) => ComponentSuggestionsState::Error {
            names: retained,
            message: format!(
                "GlyphWiki returned {} of {RANDOM_COMPONENT_COUNT} recommendations",
                names.len()
            ),
        },
        Err(message) => ComponentSuggestionsState::Error {
            names: retained,
            message,
        },
    };
    true
}

/// Filters the retained random batch while a query is being composed.
fn filter_component_suggestion_names(names: &[String], query: &str) -> Vec<String> {
    let terms = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return names.to_vec();
    }

    names
        .iter()
        .filter(|name| {
            let name = name.to_lowercase();
            terms.iter().all(|term| name.contains(term))
        })
        .cloned()
        .collect()
}

/// Loading state for a remote component and its recursive dependencies.
#[derive(Clone, Debug, Default)]
enum ComponentLoadState {
    /// No component source request is active.
    #[default]
    Idle,
    /// A complete dependency closure is being downloaded.
    Loading { name: String },
    /// The last attempted component could not be loaded.
    Error { name: String, message: String },
}

/// Render-ready component search item from the local cache or `GlyphWiki`.
#[derive(Clone, Debug)]
struct ComponentCardData {
    /// Stable `GlyphWiki` component name.
    name: String,
    /// Human-readable label shown below the preview.
    label: String,
    /// Cached KAGE source when locally available.
    source: Option<String>,
    /// Whether this item originated in the current remote result list.
    remote: bool,
}

#[derive(Clone, Copy, Debug)]
struct FrameResize {
    stroke: StrokeId,
    first: Point,
    second: Point,
    x_point: Option<FramePoint>,
    y_point: Option<FramePoint>,
}

#[derive(Clone, Debug)]
enum DragGesture {
    Move {
        last: Point,
    },
    Control {
        control: ControlPointRef,
    },
    Resize {
        handle: ResizeHandle,
        source: Rect,
        origin: Point,
        frame: Option<FrameResize>,
    },
    Marquee {
        origin: Point,
        current: Point,
    },
    Freehand,
}

component! {
    /// Full native KAGE Editor workspace and interaction state.
    component KageEditor {
        state {
            /// Focus target used for document-wide keyboard shortcuts.
            focus: FocusHandle = cx.focus_handle(),
            /// Undoable, serializable editor state.
            model: EditorModel = EditorModel::demo(),
            /// Active pointer tool.
            tool: Tool = Tool::Select,
            /// Active right-sidebar page.
            sidebar: SidebarTab = SidebarTab::Inspector,
            /// Current artboard magnification.
            zoom: f32 = 1.0,
            /// Design-space translation used to reach zoomed edges and pasteboard geometry.
            pan: Point = Point::new(0.0, 0.0),
            /// Latest canvas coordinate transform published during painting.
            canvas_transform: Rc<Cell<Option<CanvasTransform>>> = Rc::new(Cell::new(None)),
            /// Pointer gesture currently being grouped into one undo step.
            drag: Option<DragGesture> = None,
            /// Latest design-space pointer position.
            pointer: Option<Point> = None,
            /// Samples captured by the intelligent freehand tool.
            freehand: Vec<Point> = Vec::new(),
            /// Search phrase for the part library.
            component_query: String = String::new(),
            /// Native single-line input that owns selection and IME composition state.
            component_search_input: TextInputHandle = text_input(
                text(
                    UiLanguage::TraditionalChinese,
                    UiText::SearchComponentsPlaceholder,
                ),
                cx,
            ),
            /// Owned native subscription for the retained input's typed events.
            component_search_subscription: Option<Subscription> = None,
            /// Latest submitted `GlyphWiki` search state.
            component_search: ComponentSearchState = ComponentSearchState::Idle,
            /// Generation token used to discard stale search responses.
            component_search_generation: u64 = 0,
            /// Random `GlyphWiki` recommendations shown without a submitted query.
            component_suggestions: ComponentSuggestionsState = ComponentSuggestionsState::Idle,
            /// Generation token used to discard stale recommendation batches.
            component_suggestions_generation: u64 = 0,
            /// Source/dependency loading state for a clicked remote component.
            component_load: ComponentLoadState = ComponentLoadState::Idle,
            /// Generation token used to discard stale component loads.
            component_load_generation: u64 = 0,
            /// Cloneable native `GlyphWiki` API client.
            glyphwiki: GlyphWikiClient = GlyphWikiClient::new(),
            /// Whether the compact preferences sheet is open.
            show_settings: bool = false,
            /// Whether the KAGE source export sheet is open.
            show_export: bool = false,
            /// Stable, filtered, line-separated KAGE source shown and copied by the export sheet.
            export_source: String = String::new(),
            /// Whether the current export source has been copied from the sheet.
            export_copied: bool = false,
            /// Human-readable operation feedback.
            status: String = text(
                UiLanguage::TraditionalChinese,
                UiText::ReadyEngineConnected,
            ).to_owned(),
        }

        mounted(this, window, cx) {
            window.focus(&this.focus);
            let input = this.component_search_input.clone();
            this.component_search_subscription = Some(cx.subscribe_in(
                &input,
                window,
                |this, _input, event, window, cx| {
                    match event {
                        TextInputEvent::Change(value) => {
                            if this.update_component_query(value) {
                                cx.notify();
                            }
                        }
                        TextInputEvent::Submit(value) => {
                            this.update_component_query(value);
                            this.start_component_search(cx);
                        }
                        TextInputEvent::Escape => {
                            window.focus(&this.focus);
                            this.status = "Component search closed".to_owned();
                            cx.notify();
                        }
                    }
                },
            ));
            this.start_component_suggestions(cx);
        }

        template(this, window, cx) {
            this.workspace(window, cx)
        }
    }
}

impl KageEditor {
    fn ui(&self, key: UiText) -> &'static str {
        text(self.model.settings().language, key)
    }

    /// Opens or closes the modal settings surface without leaving an edit gesture alive behind it.
    fn set_settings_visible(&mut self, visible: bool, window: &mut Window) {
        window.focus(&self.focus);
        if visible {
            self.show_export = false;
            if self.model.transaction_active() {
                let _ = self.model.cancel_transaction();
            }
            self.drag = None;
            self.freehand.clear();
            self.pointer = None;
        }
        self.show_settings = visible;
    }

    /// Opens or closes the modal export surface and freezes its displayed source.
    fn set_export_visible(&mut self, visible: bool, window: &mut Window) {
        window.focus(&self.focus);
        if visible {
            self.show_settings = false;
            if self.model.transaction_active() {
                let _ = self.model.cancel_transaction();
            }
            self.drag = None;
            self.freehand.clear();
            self.pointer = None;
            self.export_source = format_export_kage(&self.model.to_export_kage());
            self.export_copied = false;
        }
        self.show_export = visible;
    }

    /// Whether a modal surface currently owns all workspace input.
    fn modal_visible(&self) -> bool {
        self.show_settings || self.show_export
    }

    fn record_kind_label(&self, kind: StrokeKind) -> String {
        match kind {
            StrokeKind::Metadata => format!("{} · type 0", self.ui(UiText::EngineTransform)),
            StrokeKind::Transform => "Extension · type 9".to_owned(),
            _ => self.ui(kind_text(kind)).to_owned(),
        }
    }

    fn workspace(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let key_listener = cx.listener(Self::on_key_down);
        let toolbar = self.toolbar(cx).into_any_element();
        let tool_rail = self.tool_rail(cx).into_any_element();
        let canvas = self.canvas_workspace(cx).into_any_element();
        let sidebar = self.sidebar(cx).into_any_element();
        let status_bar = self.status_bar().into_any_element();

        view! {
            <div
                id="kage-editor-root"
                :track-focus={&self.focus}
                key-context="KageEditor"
                @key-down={key_listener}
                class="size-full min-w-[980px] min-h-[680px] flex flex-col overflow-hidden font-sans text-[13px] text-[#e7e7ea] bg-[#171719]"
            >
                {toolbar}
                <div class="flex-1 min-h-0 flex">
                    {tool_rail}
                    {canvas}
                    {sidebar}
                </div>
                {status_bar}
                <template v-if={self.show_settings}>
                    {self.settings_sheet(cx)}
                </template>
                <template v-if={self.show_export}>
                    {self.export_sheet(cx)}
                </template>
            </div>
        }
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let undo_enabled = self.model.can_undo();
        let redo_enabled = self.model.can_redo();
        let selection = self.model.selection().len();
        let app_name = self.ui(UiText::AppName);
        let document_title = self.ui(UiText::UntitledDocument);
        let undo = toolbar_button(
            "undo",
            "↶",
            false,
            undo_enabled,
            cx.listener(|this, _, _, cx| {
                if this.model.undo() {
                    this.status = "Undo".to_owned();
                    cx.notify();
                }
            }),
        );
        let redo = toolbar_button(
            "redo",
            "↷",
            false,
            redo_enabled,
            cx.listener(|this, _, _, cx| {
                if this.model.redo() {
                    this.status = "Redo".to_owned();
                    cx.notify();
                }
            }),
        );
        let history_separator = separator(true);
        let select_previous = toolbar_button(
            "select-prev",
            "‹",
            false,
            !self.model.strokes().is_empty(),
            cx.listener(|this, _, _, cx| {
                this.model.select_previous();
                this.status = "Previous record".to_owned();
                cx.notify();
            }),
        );
        let select_next = toolbar_button(
            "select-next",
            "›",
            false,
            !self.model.strokes().is_empty(),
            cx.listener(|this, _, _, cx| {
                this.model.select_next();
                this.status = "Next record".to_owned();
                cx.notify();
            }),
        );
        let cut = toolbar_button(
            "cut",
            self.ui(UiText::Cut),
            false,
            selection != 0,
            cx.listener(|this, _, _, cx| {
                let count = this.model.cut_selected();
                this.status = format!("Cut {count} record(s)");
                cx.notify();
            }),
        );
        let copy = toolbar_button(
            "copy",
            self.ui(UiText::Copy),
            false,
            selection != 0,
            cx.listener(|this, _, _, cx| {
                let count = this.model.copy_selected();
                this.status = format!("Copied {count} record(s)");
                cx.notify();
            }),
        );
        let paste = toolbar_button(
            "paste",
            self.ui(UiText::Paste),
            false,
            self.model.pasteboard_len() != 0,
            cx.listener(|this, _, _, cx| {
                let count = this.model.paste().len();
                this.status = format!("Pasted {count} record(s)");
                cx.notify();
            }),
        );
        let grid = toolbar_button(
            "toggle-grid",
            self.ui(UiText::Grid),
            self.model.settings().grid.visible,
            true,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.grid.visible = !settings.grid.visible;
                this.model.set_settings(settings);
                cx.notify();
            }),
        );
        let centerlines = toolbar_button(
            "toggle-centerline",
            self.ui(UiText::Centerlines),
            self.model.settings().centerline != CenterlineMode::None,
            true,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.centerline = toggled_toolbar_centerline(settings.centerline);
                this.model.set_settings(settings);
                cx.notify();
            }),
        );
        let preferences = toolbar_icon_button(
            "preferences",
            "⚙",
            self.show_settings,
            true,
            cx.listener(|this, _, window, cx| {
                this.set_settings_visible(!this.show_settings, window);
                cx.notify();
            }),
        );
        let document_separator = separator(true);
        let import = toolbar_button(
            "import",
            self.ui(UiText::Import),
            false,
            true,
            cx.listener(|this, _, _, cx| this.import_kage(cx)),
        );
        let export = toolbar_button(
            "export",
            self.ui(UiText::ExportKage),
            self.show_export,
            true,
            cx.listener(|this, _, window, cx| this.export_kage(window, cx)),
        );

        view! {
            <div class="h-[52px] flex-none flex items-center gap-1 pl-[78px] pr-3 bg-[#222225] border-b border-[#343438]">
                <div class="w-[190px] flex items-center gap-2">
                    <img :src={APP_ICON_ASSET} class="w-[30px] h-[30px] flex-none" />
                    <div class="flex flex-col gap-px">
                        <div class="text-[14px] font-semibold">{app_name}</div>
                        <div class="text-[13px] text-[#929299]">{document_title}</div>
                    </div>
                </div>
                {undo}
                {redo}
                {history_separator}
                {select_previous}
                {select_next}
                {cut}
                {copy}
                {paste}
                <div class="flex-1" />
                {grid}
                {centerlines}
                {preferences}
                {document_separator}
                {import}
                {export}
            </div>
        }
    }

    fn tool_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let select = tool_button(
            "tool-select",
            "↖",
            self.ui(UiText::SelectionTool),
            self.tool == Tool::Select,
            cx.listener(|this, _, window, cx| {
                this.tool = Tool::Select;
                this.freehand.clear();
                this.status = "Selection tool".to_owned();
                window.focus(&this.focus);
                cx.notify();
            }),
        );
        let freehand = tool_button(
            "tool-freehand",
            "✎",
            self.ui(UiText::FreehandTool),
            self.tool == Tool::Freehand,
            cx.listener(|this, _, window, cx| {
                this.activate_freehand();
                window.focus(&this.focus);
                cx.notify();
            }),
        );
        let tools_separator = separator(false);
        let add_line = tool_button(
            "add-line",
            "╱",
            self.ui(UiText::AddLine),
            false,
            cx.listener(|this, _, _, cx| {
                this.model.insert_stroke(model::Stroke::line(
                    Point::new(42.0, 100.0),
                    Point::new(158.0, 100.0),
                ));
                this.status = "Inserted line".to_owned();
                cx.notify();
            }),
        );
        let add_curve = tool_button(
            "add-curve",
            "⌒",
            self.ui(UiText::AddCurve),
            false,
            cx.listener(|this, _, _, cx| {
                this.model.insert_stroke(model::Stroke::curve(
                    Point::new(42.0, 130.0),
                    Point::new(100.0, 48.0),
                    Point::new(158.0, 130.0),
                ));
                this.status = "Inserted curve".to_owned();
                cx.notify();
            }),
        );
        let delete = tool_button(
            "delete",
            "⌫",
            self.ui(UiText::Delete),
            false,
            cx.listener(|this, _, _, cx| {
                let count = this.model.delete_selected();
                this.status = format!("Deleted {count} record(s)");
                cx.notify();
            }),
        );

        view! {
            <div class="w-[54px] flex-none flex flex-col items-center gap-1 py-2 bg-[#1d1d20] border-r border-[#303034]">
                {select}
                {freehand}
                {tools_separator}
                {add_line}
                {add_curve}
                <div class="flex-1" />
                {delete}
            </div>
        }
    }

    fn canvas_workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let overlay = self.canvas_overlay();
        let snapshot = CanvasSnapshot::from_model(&self.model, self.zoom, overlay);
        let transform_slot = Rc::clone(&self.canvas_transform);
        let drawing = canvas::canvas_element(snapshot, move |_bounds, transform, _window, _cx| {
            transform_slot.set(Some(transform));
        });
        let tool_name = match self.tool {
            Tool::Select => self.ui(UiText::SelectionTool),
            Tool::Freehand => self.ui(UiText::FreehandTool),
        };
        let zoom_label = format!("{}%", (self.zoom * 100.0).round());
        let zoom_out = toolbar_button(
            "zoom-out",
            "−",
            false,
            self.zoom > MIN_ZOOM,
            cx.listener(|this, _, _, cx| {
                this.set_zoom(this.zoom - 0.2, cx);
            }),
        );
        let zoom_in = toolbar_button(
            "zoom-in",
            "+",
            false,
            self.zoom < MAX_ZOOM,
            cx.listener(|this, _, _, cx| {
                this.set_zoom(this.zoom + 0.2, cx);
            }),
        );
        let mouse_down = cx.listener(Self::on_canvas_down);
        let mouse_move = cx.listener(Self::on_canvas_move);
        let mouse_up = cx.listener(Self::on_canvas_up);
        let mouse_up_out = cx.listener(Self::on_canvas_up);
        let scroll_wheel = cx.listener(Self::on_canvas_wheel);
        let pinch = cx.listener(Self::on_canvas_pinch);

        view! {
            <div
                id="glyph-canvas"
                class="relative flex-1 min-w-0 overflow-hidden bg-[#141416] cursor-crosshair"
                @mouse-down.left={mouse_down}
                @mouse-move={mouse_move}
                @mouse-up.left={mouse_up}
                @mouse-up-out.left={mouse_up_out}
                @scroll-wheel={scroll_wheel}
                @pinch={pinch}
            >
                {drawing}
                <div class="absolute left-3 top-3 h-[30px] px-2 flex items-center gap-2 rounded-md bg-[#202024] border border-[#343439] text-[12px] text-[#b1b1b7]">
                    {tool_name}
                    <div class="w-px h-3 bg-[#3c3c42]" />
                    {zoom_label.clone()}
                </div>
                <div class="absolute right-3 bottom-3 flex items-center gap-1">
                    {zoom_out}
                    <div class="w-[58px] h-[30px] flex items-center justify-center rounded-md bg-[#202024] border border-[#343439] text-[12px]">
                        {zoom_label}
                    </div>
                    {zoom_in}
                </div>
            </div>
        }
    }

    fn canvas_overlay(&self) -> CanvasOverlay {
        let marquee = match self.drag.as_ref() {
            Some(DragGesture::Marquee {
                origin, current, ..
            }) => Some(Rect::new(*origin, *current)),
            _ => None,
        };
        let transform = self.canvas_transform.get();
        let hovered_control = self.pointer.and_then(|pointer| {
            transform.and_then(|transform| {
                canvas::hit_selected_control_point(
                    &self.model,
                    pointer,
                    canvas::control_hit_tolerance(transform),
                )
            })
        });
        CanvasOverlay {
            active_control: match self.drag.as_ref() {
                Some(DragGesture::Control { control }) => Some(*control),
                _ => None,
            },
            hovered_control,
            hovered_stroke: if hovered_control.is_none() {
                self.pointer.and_then(|pointer| {
                    canvas::hit_test_rendered(&self.model, pointer, 4.5 / self.zoom)
                })
            } else {
                None
            },
            pointer: self.pointer,
            pan: self.pan,
            freehand: self.freehand.clone(),
            marquee,
            mode: match self.tool {
                Tool::Select => InteractionMode::Select,
                Tool::Freehand => InteractionMode::Freehand,
            },
            mask_mode: self.model.settings().mask,
        }
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.sidebar {
            SidebarTab::Inspector => self.inspector_panel(cx).into_any_element(),
            SidebarTab::Components => self.components_panel(cx).into_any_element(),
            SidebarTab::Layers => self.layers_panel(cx).into_any_element(),
        };
        let inspector_tab = self
            .sidebar_tab(
                "tab-inspector",
                self.ui(UiText::Inspector),
                SidebarTab::Inspector,
                cx,
            )
            .into_any_element();
        let components_tab = self
            .sidebar_tab(
                "tab-components",
                self.ui(UiText::Components),
                SidebarTab::Components,
                cx,
            )
            .into_any_element();
        let layers_tab = self
            .sidebar_tab(
                "tab-layers",
                self.ui(UiText::Layers),
                SidebarTab::Layers,
                cx,
            )
            .into_any_element();

        view! {
            <div class="w-[314px] flex-none flex flex-col min-h-0 bg-[#1e1e21] border-l border-[#343438]">
                <div class="h-[40px] flex-none flex items-end px-2 gap-1 border-b border-[#333337]">
                    {inspector_tab}
                    {components_tab}
                    {layers_tab}
                </div>
                {body}
            </div>
        }
    }

    fn sidebar_tab(
        &self,
        id: &'static str,
        label: &'static str,
        tab: SidebarTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.sidebar == tab;
        let select_tab = cx.listener(move |this, _, window, cx| {
            this.sidebar = tab;
            window.focus(&this.focus);
            cx.notify();
        });

        view! {
            <div
                id={id}
                class="relative h-[36px] px-2 flex items-center justify-center cursor-pointer text-[12px] font-semibold hover:text-[#e8e8ea]"
                :class={if active {
                    "text-[#f1f1f3]"
                } else {
                    "text-[#929299]"
                }}
                @click={select_tab}
            >
                {label}
                <div
                    v-if={active}
                    class="absolute bottom-0 left-2 right-2 h-[2px] rounded-xs bg-[#0a84ff]"
                />
            </div>
        }
    }

    fn inspector_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.model.selected_strokes().cloned().collect::<Vec<_>>();
        let selection_count = selected.len();
        let selected_indices = self
            .model
            .strokes()
            .iter()
            .enumerate()
            .filter(|(_, stroke)| self.model.is_selected(stroke.id()))
            .map(|(index, _)| (index + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let component_count = selected
            .iter()
            .filter(|stroke| stroke.kind() == StrokeKind::Component)
            .count();
        let selected_component = (selection_count == 1
            && selected[0].kind() == StrokeKind::Component)
            .then(|| selected[0].clone());
        let component_stretch_value = selected_component
            .as_ref()
            .and_then(|stroke| self.model.component_stretch_value(stroke.id()));
        let single_path = selection_count == 1 && selected[0].kind().is_path();
        let selection_bounds = self.model.selection_bounds();
        let mut issues = self.model.validate();
        issues.sort_by_key(|issue| {
            (
                !issue.stroke.is_some_and(|id| self.model.is_selected(id)),
                matches!(issue.severity, ValidationSeverity::Warning),
            )
        });

        let selection_summary = if selected_indices.is_empty() {
            format!("0 / {}", self.model.strokes().len())
        } else {
            format!("{selected_indices} / {}", self.model.strokes().len())
        };
        let bounds_value = selection_bounds.map_or_else(
            || "—".to_owned(),
            |bounds| {
                format!(
                    "{:.1} × {:.1}  @  {:.1}, {:.1}",
                    bounds.width(),
                    bounds.height(),
                    bounds.min.x,
                    bounds.min.y
                )
            },
        );
        let type_value = selected.first().map_or_else(
            || "—".to_owned(),
            |stroke| self.record_kind_label(stroke.kind()),
        );
        let point_count_value = selected
            .iter()
            .map(|stroke| stroke.points().len())
            .sum::<usize>()
            .to_string();
        let style_stroke = single_path.then(|| selected[0].clone());
        let style_id = style_stroke
            .as_ref()
            .map_or_else(String::new, |stroke| format!("ID {}", stroke.id().get()));
        let style_points = style_stroke.as_ref().map_or_else(Vec::new, |stroke| {
            stroke
                .points()
                .iter()
                .enumerate()
                .map(|(index, point)| {
                    (
                        index,
                        format!("P{}", index + 1),
                        format!("{:.1}, {:.1}", point.x, point.y),
                    )
                })
                .collect::<Vec<_>>()
        });
        let component_id = selected_component.as_ref().map(model::Stroke::id);
        let component_name = selected_component
            .as_ref()
            .map_or_else(String::new, |stroke| {
                stroke
                    .component()
                    .map_or_else(|| "—".to_owned(), |component| component.name().to_owned())
            });
        let component_stretch_label = component_stretch_value.map_or_else(
            || self.ui(UiText::Neutral).to_owned(),
            |value| format!("{value:+} / 10"),
        );
        let selected_metadata = selected
            .first()
            .filter(|stroke| selection_count == 1 && stroke.kind() == StrokeKind::Metadata);
        let metadata_transform = selected_metadata.and_then(model::Stroke::kage_transform);
        let metadata_transform_label =
            metadata_transform.map_or("Unknown type 0", kage_transform_name);
        let (show_affine_transform, show_order_controls) = inspector_transform_sections(
            selection_count,
            self.model.can_affine_transform_selection(),
        );
        let issues_empty = issues.is_empty();
        let issue_rows = issues
            .into_iter()
            .take(5)
            .enumerate()
            .map(|(index, issue)| (index, issue.severity, issue.message))
            .collect::<Vec<_>>();

        let stroke_kind_picker = if single_path {
            self.stroke_kind_picker(cx).into_any_element()
        } else {
            view! { <div /> }.into_any_element()
        };
        let head_stepper = if let Some(stroke) = style_stroke.as_ref() {
            self.style_stepper(self.ui(UiText::Head), stroke.head(), true, cx)
                .into_any_element()
        } else {
            view! { <div /> }.into_any_element()
        };
        let tail_stepper = if let Some(stroke) = style_stroke.as_ref() {
            self.style_stepper(self.ui(UiText::Tail), stroke.tail(), false, cx)
                .into_any_element()
        } else {
            view! { <div /> }.into_any_element()
        };
        let transform_controls = self.transform_controls(cx).into_any_element();
        let order_controls = self.order_controls(cx).into_any_element();
        let stretch_controls =
            if let (Some(component), Some(value)) = (component_id, component_stretch_value) {
                self.component_stretch_controls(component, value, cx)
                    .into_any_element()
            } else {
                view! { <div /> }.into_any_element()
            };
        let kage_transform_controls = self
            .kage_transform_controls(metadata_transform, cx)
            .into_any_element();
        let decompose_component = toolbar_button(
            "decompose-component",
            format!(
                "{} · {component_count}",
                self.ui(UiText::DecomposeToStrokes)
            ),
            false,
            true,
            cx.listener(|this, _, _, cx| {
                match this.model.decompose_selected_components() {
                    Ok(ids) => {
                        this.status =
                            format!("Decomposed selected components into {} records", ids.len());
                    }
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            }),
        );
        let decompose_components = toolbar_button(
            "decompose-components",
            format!(
                "{} · {component_count}",
                self.ui(UiText::DecomposeToStrokes)
            ),
            false,
            true,
            cx.listener(|this, _, _, cx| {
                match this.model.decompose_selected_components() {
                    Ok(ids) => {
                        this.status =
                            format!("Decomposed selected components into {} records", ids.len());
                    }
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            }),
        );

        view! {
            <div id="inspector-scroll" class="flex-1 min-h-0 overflow-y-scroll">
                <div class="flex flex-col px-3 pb-4">
                    {section_header(self.ui(UiText::Selection), selection_summary)}

                    <div
                        v-if={selection_count == 0}
                        class="py-5 px-3 rounded-md bg-[#242428] border border-[#343439] text-center text-[13px] text-[#929299]"
                    >
                        {self.ui(UiText::NoSelection)}
                    </div>

                    <template v-if={selection_count != 0}>
                        {inspector_row(self.ui(UiText::Bounds), bounds_value)}
                        {inspector_row(self.ui(UiText::Type), type_value)}
                        {inspector_row(self.ui(UiText::Points), point_count_value)}
                    </template>

                    <template v-if={single_path}>
                        {separator(false)}
                        {section_header(
                            self.ui(UiText::StrokeType),
                            self.ui(UiText::KageRecord),
                        )}
                        {stroke_kind_picker}
                    </template>

                    <template v-if={single_path}>
                        {separator(false)}
                        {section_header(self.ui(UiText::Style), style_id)}
                        {head_stepper}
                        {tail_stepper}
                        <div
                            v-for={row in style_points}
                            :key={("inspector-point", row.0)}
                            class="min-h-[32px] flex items-center justify-between gap-3 text-[13px]"
                        >
                            <div class="text-[#a0a0a7]">{row.1}</div>
                            <div class="text-[#e5e5e8] text-ellipsis">{row.2}</div>
                        </div>
                    </template>

                    <template v-if={show_affine_transform}>
                        {separator(false)}
                        {section_header(
                            self.ui(UiText::Transform),
                            self.ui(UiText::ConnectedGeometry),
                        )}
                        {transform_controls}
                    </template>

                    <template v-if={show_order_controls}>
                        {separator(false)}
                        {section_header(self.ui(UiText::PaintOrder), "")}
                        {order_controls}
                    </template>

                    <template v-if={selected_component.is_some()}>
                        {separator(false)}
                        {section_header(
                            self.ui(UiText::Component),
                            format!("{component_count} selected"),
                        )}
                        {inspector_row(self.ui(UiText::Name), component_name)}
                        {inspector_row(self.ui(UiText::Stretch), component_stretch_label)}
                        <template v-if={component_stretch_value.is_some()}>
                            {stretch_controls}
                        </template>
                        {decompose_component}
                    </template>

                    <template v-if={component_count != 0 && selected_component.is_none()}>
                        {separator(false)}
                        {section_header(
                            self.ui(UiText::Component),
                            format!("{component_count} selected"),
                        )}
                        {decompose_components}
                    </template>

                    <template v-if={selected_metadata.is_some()}>
                        {separator(false)}
                        {section_header(self.ui(UiText::EngineTransform), "type 0")}
                        {inspector_row(
                            self.ui(UiText::Operation),
                            metadata_transform_label,
                        )}
                        {kage_transform_controls}
                    </template>

                    {separator(false)}
                    {section_header(
                        self.ui(UiText::DocumentHealth),
                        if issues_empty {
                            self.ui(UiText::Clean)
                        } else {
                            self.ui(UiText::Review)
                        },
                    )}
                    <div
                        v-if={issues_empty}
                        class="h-[30px] flex items-center gap-2 text-[13px] text-[#72c98b]"
                    >
                        {"●"}
                        {self.ui(UiText::NoStructuralIssues)}
                    </div>
                    <div
                        v-for={row in issue_rows}
                        :key={("validation-issue", row.0)}
                        class="min-h-[30px] py-1 flex gap-2 text-[12px] text-[#b8b8be]"
                    >
                        <div :class={if matches!(row.1, ValidationSeverity::Error) {
                            "text-[#ff6b68]"
                        } else {
                            "text-[#e7ad55]"
                        }}>
                            {"●"}
                        </div>
                        {row.2}
                    </div>
                </div>
            </div>
        }
    }

    fn stroke_kind_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let kinds = [
            StrokeKind::Line,
            StrokeKind::Curve,
            StrokeKind::Bend,
            StrokeKind::Corner,
            StrokeKind::Bezier,
            StrokeKind::Sweep,
        ];
        let active = self
            .model
            .selected_strokes()
            .next()
            .map(model::Stroke::kind);
        let buttons = kinds.map(|kind| {
            toolbar_button(
                ("stroke-kind", kind.code() as usize),
                format!("{} · {}", kind.code(), self.ui(kind_text(kind))),
                active == Some(kind),
                true,
                cx.listener(move |this, _, _, cx| {
                    let changed = this.model.set_selected_kind(kind);
                    this.status = format!("Changed {changed} record(s) to {}", kind_name(kind));
                    cx.notify();
                }),
            )
            .into_any_element()
        });
        let [line, curve, bend, corner, bezier, sweep] = buttons;

        view! {
            <div class="flex flex-wrap gap-1">
                {line}
                {curve}
                {bend}
                {corner}
                {bezier}
                {sweep}
            </div>
        }
    }

    fn style_stepper(
        &self,
        label: &'static str,
        value: i32,
        is_head: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let decrement = toolbar_button(
            if is_head { "head-minus" } else { "tail-minus" },
            "−",
            false,
            true,
            cx.listener(move |this, _, _, cx| {
                this.adjust_style(is_head, -1);
                cx.notify();
            }),
        );
        let increment = toolbar_button(
            if is_head { "head-plus" } else { "tail-plus" },
            "+",
            false,
            true,
            cx.listener(move |this, _, _, cx| {
                this.adjust_style(is_head, 1);
                cx.notify();
            }),
        );

        view! {
            <div class="h-[32px] flex items-center justify-between text-[13px]">
                <div class="text-[#8e8e94]">{label}</div>
                <div class="flex gap-1 items-center">
                    {decrement}
                    <div class="w-[38px] text-center font-mono text-[12px]">
                        {value.to_string()}
                    </div>
                    {increment}
                </div>
            </div>
        }
    }

    fn transform_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let flip_horizontal = toolbar_button(
            "flip-horizontal",
            self.ui(UiText::FlipHorizontal),
            false,
            true,
            cx.listener(|this, _, _, cx| {
                this.apply_transform("Flip left-right", |center| {
                    AffineTransform::flip_horizontal(center.x)
                });
                cx.notify();
            }),
        );
        let flip_vertical = toolbar_button(
            "flip-vertical",
            self.ui(UiText::FlipVertical),
            false,
            true,
            cx.listener(|this, _, _, cx| {
                this.apply_transform("Flip top-bottom", |center| {
                    AffineTransform::flip_vertical(center.y)
                });
                cx.notify();
            }),
        );
        let rotate_left = toolbar_button(
            "rotate-left",
            format!("↺ 90° · {}", self.ui(UiText::RotateLeft)),
            false,
            true,
            cx.listener(|this, _, _, cx| {
                this.apply_transform("Rotate left", |center| {
                    AffineTransform::rotation_about(-FRAC_PI_2, center)
                });
                cx.notify();
            }),
        );
        let rotate_right = toolbar_button(
            "rotate-right",
            format!("↻ 90° · {}", self.ui(UiText::RotateRight)),
            false,
            true,
            cx.listener(|this, _, _, cx| {
                this.apply_transform("Rotate right", |center| {
                    AffineTransform::rotation_about(FRAC_PI_2, center)
                });
                cx.notify();
            }),
        );

        view! {
            <div class="flex flex-wrap gap-1">
                {flip_horizontal}
                {flip_vertical}
                {rotate_left}
                {rotate_right}
            </div>
        }
    }

    fn component_stretch_controls(
        &self,
        component: StrokeId,
        value: i32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let decrement = toolbar_button(
            "stretch-minus",
            "−",
            false,
            value > -10,
            cx.listener(move |this, _, _, cx| {
                this.set_component_stretch_value(component, value - 1);
                cx.notify();
            }),
        );
        let increment = toolbar_button(
            "stretch-plus",
            "+",
            false,
            value < 10,
            cx.listener(move |this, _, _, cx| {
                this.set_component_stretch_value(component, value + 1);
                cx.notify();
            }),
        );

        view! {
            <div class="flex gap-1">
                {decrement}
                <div class="w-[54px] h-[28px] flex items-center justify-center rounded-md bg-[#29292d] border border-[#3d3d42] font-mono text-[12px]">
                    {format!("{value:+}")}
                </div>
                {increment}
            </div>
        }
    }

    fn kage_transform_controls(
        &self,
        active: Option<KageTransform>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let choices = kage_transform_choices(
            active,
            self.ui(UiText::FlipHorizontal),
            self.ui(UiText::FlipVertical),
        );
        let buttons = choices.map(|(index, transform, label, is_active)| {
            toolbar_button(
                ("kage-transform", index),
                label,
                is_active,
                true,
                cx.listener(move |this, _, _, cx| {
                    let (head, tail) = transform.parameters();
                    this.model.set_selected_style(head, tail);
                    this.status = format!("Type 0 · {}", kage_transform_name(transform));
                    cx.notify();
                }),
            )
            .into_any_element()
        });
        let [
            flip_horizontal,
            flip_vertical,
            rotate_90,
            rotate_180,
            rotate_270,
        ] = buttons;

        view! {
            <div class="flex flex-wrap gap-1">
                {flip_horizontal}
                {flip_vertical}
                {rotate_90}
                {rotate_180}
                {rotate_270}
            </div>
        }
    }

    fn order_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_index = self.model.strokes().iter().position(|stroke| {
            self.model.selection().len() == 1 && self.model.is_selected(stroke.id())
        });
        let can_move_backward = selected_index.is_some_and(|index| index != 0);
        let can_move_forward =
            selected_index.is_some_and(|index| index + 1 < self.model.strokes().len());
        let send_backward = toolbar_button(
            "send-backward",
            self.ui(UiText::SendBackward),
            false,
            can_move_backward,
            cx.listener(|this, _, _, cx| {
                this.model.move_selected_in_order(OrderDirection::Backward);
                this.status = "Moved selection backward".to_owned();
                cx.notify();
            }),
        );
        let bring_forward = toolbar_button(
            "bring-forward",
            self.ui(UiText::BringForward),
            false,
            can_move_forward,
            cx.listener(|this, _, _, cx| {
                this.model.move_selected_in_order(OrderDirection::Forward);
                this.status = "Moved selection forward".to_owned();
                cx.notify();
            }),
        );

        view! {
            <div class="mt-1 flex gap-1">
                {send_backward}
                {bring_forward}
            </div>
        }
    }

    fn components_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        struct ComponentPanelItem {
            key: SharedString,
            name: String,
            label: String,
            source_label: String,
            remote: bool,
            preview: AnyElement,
        }

        let preview_typeface = self.model.settings().typeface;
        let preview_use_curve = self.model.settings().use_curve;
        let library = self.model.component_library();
        let suggestion_matches = || {
            filter_component_suggestion_names(
                self.component_suggestions.names(),
                &self.component_query,
            )
            .into_iter()
            .map(|name| {
                let cached = library.get(&name);
                ComponentCardData {
                    label: cached.map_or_else(|| name.clone(), |item| item.label().to_owned()),
                    source: cached.map(|item| item.source().to_owned()),
                    name,
                    remote: true,
                }
            })
            .collect::<Vec<_>>()
        };
        let current_remote_state = self.component_search.query() == Some(&self.component_query);
        let (matches, origin_label, notice) = if current_remote_state {
            match &self.component_search {
                ComponentSearchState::Loading { .. } => (
                    Vec::new(),
                    "GlyphWiki".to_owned(),
                    Some(("Searching GlyphWiki…".to_owned(), false)),
                ),
                ComponentSearchState::Ready { names, .. } => (
                    names
                        .iter()
                        .map(|name| {
                            let cached = library.get(name);
                            ComponentCardData {
                                name: name.clone(),
                                label: cached
                                    .map_or_else(|| name.clone(), |item| item.label().to_owned()),
                                source: cached.map(|item| item.source().to_owned()),
                                remote: true,
                            }
                        })
                        .collect(),
                    "GlyphWiki".to_owned(),
                    None,
                ),
                ComponentSearchState::TooShort { .. } => (
                    Vec::new(),
                    "GlyphWiki".to_owned(),
                    Some(("Query too short · add more detail".to_owned(), true)),
                ),
                ComponentSearchState::NoData { .. } => (
                    Vec::new(),
                    "GlyphWiki".to_owned(),
                    Some((self.ui(UiText::NoComponentMatches).to_owned(), false)),
                ),
                ComponentSearchState::Error { message, .. } => (
                    suggestion_matches(),
                    "Random GlyphWiki picks".to_owned(),
                    Some((
                        format!("GlyphWiki search failed · {message} · showing random picks"),
                        true,
                    )),
                ),
                ComponentSearchState::Idle => unreachable!("idle has no submitted query"),
            }
        } else {
            match &self.component_suggestions {
                ComponentSuggestionsState::Idle
                | ComponentSuggestionsState::Loading { names: _ } => (
                    suggestion_matches(),
                    "GlyphWiki recommendations".to_owned(),
                    Some(("Loading 50 random GlyphWiki components…".to_owned(), false)),
                ),
                ComponentSuggestionsState::Ready { .. } => (
                    suggestion_matches(),
                    if self.component_query.trim().is_empty() {
                        "50 random GlyphWiki picks".to_owned()
                    } else {
                        "Random picks · Return searches GlyphWiki".to_owned()
                    },
                    None,
                ),
                ComponentSuggestionsState::Error { message, .. } => (
                    suggestion_matches(),
                    "GlyphWiki recommendations".to_owned(),
                    Some((
                        format!("Could not refresh random components · {message}"),
                        true,
                    )),
                ),
            }
        };
        let load_notice = match &self.component_load {
            ComponentLoadState::Idle => None,
            ComponentLoadState::Loading { name } => {
                Some((format!("Loading {name} and dependencies…"), false))
            }
            ComponentLoadState::Error { name, message } => {
                Some((format!("Could not load {name} · {message}"), true))
            }
        };
        let has_notice = notice.is_some();
        let match_count = matches.len();
        let query_is_empty = self.component_query.is_empty();
        let search_disabled = self.component_query.trim().is_empty();
        let suggestions_loading = matches!(
            self.component_suggestions,
            ComponentSuggestionsState::Idle | ComponentSuggestionsState::Loading { .. }
        );
        let suggestions_failed = matches!(
            self.component_suggestions,
            ComponentSuggestionsState::Error { .. }
        );
        let suggestions_action_visible =
            !current_remote_state && self.component_query.trim().is_empty() && !suggestions_loading;
        let suggestions_action_label = if suggestions_failed {
            "Retry"
        } else {
            "Refresh"
        };
        let match_label = format!("{match_count} {}", self.ui(UiText::Matches));
        let descendants_hint = self.ui(UiText::FindDescendants);
        let no_matches = self.ui(UiText::NoComponentMatches);
        let (notice_visible, notice_message, notice_is_error) = notice.map_or_else(
            || (false, String::new(), false),
            |(message, is_error)| (true, message, is_error),
        );
        let (load_notice_visible, load_notice_message, load_notice_is_error) = load_notice
            .map_or_else(
                || (false, String::new(), false),
                |(message, is_error)| (true, message, is_error),
            );
        let cards = matches
            .into_iter()
            .map(|item| {
                let loading = matches!(
                    &self.component_load,
                    ComponentLoadState::Loading { name: loading_name }
                        if loading_name == &item.name
                );
                let preview = if let Some(source) = item.source {
                    component_preview(source, preview_typeface, preview_use_curve)
                        .into_any_element()
                } else {
                    remote_component_preview(item.name.clone(), loading).into_any_element()
                };
                let source_label = if item.remote {
                    format!("GW · {}", item.name)
                } else {
                    item.name.clone()
                };
                ComponentPanelItem {
                    key: SharedString::from(format!("component-card-{}", item.name)),
                    name: item.name,
                    label: item.label,
                    source_label,
                    remote: item.remote,
                    preview,
                }
            })
            .collect::<Vec<_>>();
        let search_input = self.component_search_input.clone();
        let placeholder = self.ui(UiText::SearchComponentsPlaceholder).to_owned();
        self.component_search_input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
        });
        let focus_search = cx.listener(|this, _, window, cx| {
            this.component_search_input.read(cx).focus(window);
            this.status = "Type a query, then press Return to search GlyphWiki".to_owned();
            cx.notify();
        });
        let clear_search = cx.listener(|this, _, window, cx| {
            this.clear_component_query(cx);
            this.component_search_input.read(cx).focus(window);
            cx.notify();
        });
        let search_glyphwiki = cx.listener(|this, _, _, cx| {
            this.start_component_search(cx);
        });
        let refresh_suggestions = cx.listener(|this, _, _, cx| {
            this.start_component_suggestions(cx);
        });

        view! {
            <div class="flex-1 min-h-0 flex flex-col">
                <div class="flex-none p-3 pb-2">
                    <div class="flex items-center gap-1">
                        <div id="component-search" class="flex-1 min-w-0" @click={focus_search}>
                            {search_input}
                        </div>
                        <div
                            v-if={!query_is_empty}
                            id="clear-component-search"
                            class="h-[30px] w-[26px] flex items-center justify-center rounded-md cursor-pointer text-[14px] text-[#88888f] hover:bg-[#34343a] hover:text-white"
                            @click={clear_search}
                        >
                            "×"
                        </div>
                        <div
                            id="search-glyphwiki"
                            class="h-[30px] px-2 flex items-center justify-center rounded-md bg-[#34343a] border border-[#46464c] cursor-pointer text-[12px] hover:bg-[#404047]"
                            :class={if search_disabled {
                                "text-[#6c6c72]"
                            } else {
                                "text-[#e8e8eb]"
                            }}
                            @click={search_glyphwiki}
                        >
                            "GW ↵"
                        </div>
                    </div>
                    <div class="mt-2 flex justify-between text-[11px] text-[#85858c]">
                        {match_label}
                        <div class="flex flex-col items-end">
                            <div class="flex items-center gap-1.5">
                                {origin_label}
                                <div
                                    v-if={suggestions_action_visible}
                                    id="refresh-component-suggestions"
                                    class="rounded px-1.5 py-0.5 cursor-pointer text-[#a9a9af] hover:bg-[#34343a] hover:text-white"
                                    @click={refresh_suggestions}
                                >
                                    {suggestions_action_label}
                                </div>
                            </div>
                            {descendants_hint}
                        </div>
                    </div>
                    <div
                        v-if={notice_visible}
                        class="mt-2 rounded px-2 py-1 text-[11px]"
                        :class={if notice_is_error {
                            "bg-[#392528] text-[#ff9b9b]"
                        } else {
                            "bg-[#252d38] text-[#8ec5ff]"
                        }}
                    >
                        {notice_message}
                    </div>
                    <div
                        v-if={load_notice_visible}
                        class="mt-2 rounded px-2 py-1 text-[11px]"
                        :class={if load_notice_is_error {
                            "bg-[#392528] text-[#ff9b9b]"
                        } else {
                            "bg-[#2d2b24] text-[#e8c77c]"
                        }}
                    >
                        {load_notice_message}
                    </div>
                </div>
                <div id="components-scroll" class="flex-1 min-h-0 overflow-y-scroll px-3 pb-4">
                    <div
                        v-if={match_count == 0 && !has_notice}
                        class="py-6 text-center text-[13px] text-[#929299]"
                    >
                        {no_matches}
                    </div>
                    <div class="grid grid-cols-2 gap-2">
                        <div
                            v-for={item in cards}
                            :key={item.key.clone()}
                            class="h-[121px] flex flex-col overflow-hidden rounded-md bg-[#252529] border border-[#37373c] cursor-pointer hover:bg-[#2c2c31] hover:border-[#4b4b51] active:bg-[#202024]"
                            @click={cx.listener({
                                let insert_name = item.name.clone();
                                let descendant_name = item.name.clone();
                                let remote = item.remote;
                                move |this, event: &ClickEvent, window, cx| {
                                    if event.modifiers().shift {
                                        this.set_component_query_and_input(
                                            descendant_name.clone(),
                                            cx,
                                        );
                                        this.component_search_input.read(cx).focus(window);
                                        this.start_component_search(cx);
                                    } else {
                                        window.focus(&this.focus);
                                        this.insert_or_load_component(
                                            insert_name.clone(),
                                            remote,
                                            cx,
                                        );
                                    }
                                    cx.notify();
                                }
                            })}
                        >
                            {item.preview}
                            <div class="h-[38px] flex items-center justify-between px-2 border-t border-[#36363b] text-[12px]">
                                <div class="flex-1 min-w-0 overflow-hidden text-ellipsis font-semibold text-[#e0e0e3]">
                                    {item.label}
                                </div>
                                <div
                                    class="min-w-0 overflow-hidden text-ellipsis font-mono text-[10px]"
                                    :class={if item.remote {
                                        "text-[#5faeff]"
                                    } else {
                                        "text-[#85858c]"
                                    }}
                                >
                                    {item.source_label}
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        }
    }

    fn layers_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        struct LayerPanelRow {
            index: usize,
            id: StrokeId,
            kind_code: String,
            label: String,
            head: i32,
            tail: i32,
            selected: bool,
        }

        let rows = layer_rows(&self.model)
            .map(|(index, stroke)| {
                let kind = stroke.kind();
                LayerPanelRow {
                    index,
                    id: stroke.id(),
                    kind_code: kind.code().to_string(),
                    label: stroke
                        .component()
                        .map(model::ComponentRef::name)
                        .map_or_else(|| self.record_kind_label(kind), str::to_owned),
                    head: stroke.head(),
                    tail: stroke.tail(),
                    selected: self.model.is_selected(stroke.id()),
                }
            })
            .collect::<Vec<_>>();
        let row_count = rows.len();
        let header = section_header(self.ui(UiText::PaintOrder), format!("{row_count} records"));
        let no_layers = self.ui(UiText::NoLayers);

        view! {
            <div class="flex-1 min-h-0 flex flex-col">
                <div class="px-3">{header}</div>
                <div id="layers-scroll" class="flex-1 min-h-0 overflow-y-scroll px-2 pb-3">
                    <div
                        v-if={row_count == 0}
                        class="py-6 text-center text-[13px] text-[#929299]"
                    >
                        {no_layers}
                    </div>
                    <div
                        v-for={row in rows}
                        :key={("layer", row.id.get())}
                        class="h-[42px] mb-1 px-2 flex items-center gap-2 rounded border cursor-pointer hover:bg-[#2d2d32]"
                        :class={if row.selected {
                            "bg-[#173b60] border-[#286aa8]"
                        } else {
                            "bg-[#242428] border-[#343439]"
                        }}
                        @click={cx.listener({
                            let id = row.id;
                            move |this, event: &ClickEvent, _, cx| {
                                let mode = if event.modifiers().shift {
                                    SelectionMode::Toggle
                                } else {
                                    SelectionMode::Replace
                                };
                                this.model.select(id, mode);
                                this.status = format!("Selected record {}", id.get());
                                cx.notify();
                            }
                        })}
                    >
                        <div
                            class="w-[24px] h-[24px] flex items-center justify-center rounded bg-[#18181b] font-mono text-[11px]"
                            :class={if row.selected {
                                "text-[#76bbff]"
                            } else {
                                "text-[#86868d]"
                            }}
                        >
                            {row.kind_code}
                        </div>
                        <div class="flex-1 min-w-0 flex flex-col gap-[2px]">
                            <div class="text-[13px] font-semibold">{row.label}</div>
                            <div class="text-[10px] text-[#85858c]">
                                {format!(
                                    "#{:02} · head {} · tail {}",
                                    row.index + 1,
                                    row.head,
                                    row.tail,
                                )}
                            </div>
                        </div>
                        <div class="text-[12px] text-[#85858c]">"≡"</div>
                    </div>
                </div>
            </div>
        }
    }

    fn settings_sheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = *self.model.settings();
        let close = toolbar_button(
            "close-settings",
            self.ui(UiText::Done),
            true,
            true,
            cx.listener(|this, _, window, cx| {
                this.set_settings_visible(false, window);
                cx.notify();
            }),
        );
        let mincho = self.setting_choice(
            "typeface-mincho",
            self.ui(UiText::Mincho),
            settings.typeface == Typeface::Mincho,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.typeface = Typeface::Mincho;
                this.model.set_settings(settings);
                cx.notify();
            }),
        );
        let gothic = self.setting_choice(
            "typeface-gothic",
            self.ui(UiText::Gothic),
            settings.typeface == Typeface::Gothic,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.typeface = Typeface::Gothic;
                this.model.set_settings(settings);
                cx.notify();
            }),
        );
        let skeleton = self.setting_choice(
            "typeface-skeleton",
            self.ui(UiText::Skeleton),
            settings.typeface == Typeface::Skeleton,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.typeface = Typeface::Skeleton;
                this.model.set_settings(settings);
                cx.notify();
            }),
        );
        let curves_off = self.setting_choice(
            "curve-rendering-off",
            self.ui(UiText::Off),
            !settings.use_curve,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.use_curve = false;
                this.model.set_settings(settings);
                this.status = "Smooth Mincho strokes disabled".to_owned();
                cx.notify();
            }),
        );
        let curves_on = self.setting_choice(
            "curve-rendering-on",
            self.ui(UiText::On),
            settings.use_curve,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.use_curve = true;
                this.model.set_settings(settings);
                this.status = "Smooth Mincho strokes enabled".to_owned();
                cx.notify();
            }),
        );
        let centerline_none = self.centerline_choice(
            "centerline-none",
            self.ui(UiText::CenterlineNone),
            CenterlineMode::None,
            cx,
        );
        let centerline_selection = self.centerline_choice(
            "centerline-selection",
            self.ui(UiText::CenterlineSelection),
            CenterlineMode::Selection,
            cx,
        );
        let centerline_always = self.centerline_choice(
            "centerline-always",
            self.ui(UiText::CenterlineAlways),
            CenterlineMode::Always,
            cx,
        );
        let origin_x = self.grid_stepper(
            "grid-origin-x",
            self.ui(UiText::GridOriginX),
            settings.grid.origin_x,
            GridField::OriginX,
            cx,
        );
        let origin_y = self.grid_stepper(
            "grid-origin-y",
            self.ui(UiText::GridOriginY),
            settings.grid.origin_y,
            GridField::OriginY,
            cx,
        );
        let spacing_x = self.grid_stepper(
            "grid-spacing-x",
            self.ui(UiText::GridSpacingX),
            settings.grid.spacing_x,
            GridField::SpacingX,
            cx,
        );
        let spacing_y = self.grid_stepper(
            "grid-spacing-y",
            self.ui(UiText::GridSpacingY),
            settings.grid.spacing_y,
            GridField::SpacingY,
            cx,
        );
        let mask_none =
            self.mask_choice("mask-none", self.ui(UiText::MaskNone), MaskMode::None, cx);
        let mask_circle = self.mask_choice(
            "mask-circle",
            self.ui(UiText::MaskCircle),
            MaskMode::Circle,
            cx,
        );
        let mask_rounded = self.mask_choice(
            "mask-rounded",
            self.ui(UiText::MaskRoundedSquare),
            MaskMode::RoundedSquare,
            cx,
        );
        let mask_square = self.mask_choice(
            "mask-square",
            self.ui(UiText::MaskSquare),
            MaskMode::Square,
            cx,
        );
        let mask_diamond = self.mask_choice(
            "mask-diamond",
            self.ui(UiText::MaskDiamond),
            MaskMode::Diamond,
            cx,
        );
        let english = self.language_choice(
            "language-en",
            self.ui(UiText::English),
            UiLanguage::English,
            cx,
        );
        let japanese = self.language_choice(
            "language-ja",
            self.ui(UiText::Japanese),
            UiLanguage::Japanese,
            cx,
        );
        let korean = self.language_choice(
            "language-ko",
            self.ui(UiText::Korean),
            UiLanguage::Korean,
            cx,
        );
        let simplified_chinese = self.language_choice(
            "language-zh-hans",
            self.ui(UiText::SimplifiedChinese),
            UiLanguage::SimplifiedChinese,
            cx,
        );
        let traditional_chinese = self.language_choice(
            "language-zh-hant",
            self.ui(UiText::TraditionalChinese),
            UiLanguage::TraditionalChinese,
            cx,
        );
        let snap = toolbar_button(
            "toggle-snap",
            if settings.grid.snap {
                self.ui(UiText::On)
            } else {
                self.ui(UiText::Off)
            },
            settings.grid.snap,
            true,
            cx.listener(|this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.grid.snap = !settings.grid.snap;
                this.model.set_settings(settings);
                cx.notify();
            }),
        );

        view! {
            <div
                id="settings-backdrop"
                occlude
                class="absolute inset-0 flex items-center justify-center bg-[#00000066]"
                @click={cx.listener(|this, _, window, cx| {
                    this.set_settings_visible(false, window);
                    cx.notify();
                })}
            >
                <div
                    id="settings-sheet"
                    occlude
                    class="w-[500px] max-h-[620px] flex flex-col overflow-hidden rounded-lg bg-[#252528] border border-[#46464c]"
                    @click={|_, _, cx| cx.stop_propagation()}
                >
                    <div class="h-[48px] px-4 flex-none flex items-center justify-between border-b border-[#3b3b40]">
                        <div class="text-[15px] font-semibold">{self.ui(UiText::DocumentAppearance)}</div>
                        {close}
                    </div>
                    <div id="settings-scroll" class="min-h-0 p-4 flex flex-col gap-3 overflow-y-scroll">
                        <div class="flex flex-col gap-2">
                            <div class="text-[11px] font-semibold text-[#929299]">{self.ui(UiText::Typeface)}</div>
                            <div class="flex flex-wrap gap-1">{mincho}{gothic}{skeleton}</div>
                        </div>
                        <div class="flex flex-col gap-2">
                            <div class="text-[11px] font-semibold text-[#929299]">{self.ui(UiText::SmoothStrokes)}</div>
                            <div class="flex flex-wrap gap-1">{curves_off}{curves_on}</div>
                        </div>
                        <div class="flex flex-col gap-2">
                            <div class="text-[11px] font-semibold text-[#929299]">{self.ui(UiText::Centerlines)}</div>
                            <div class="flex flex-wrap gap-1">{centerline_none}{centerline_selection}{centerline_always}</div>
                        </div>
                        <div class="flex flex-col gap-2">
                            <div class="text-[11px] font-semibold text-[#929299]">{self.ui(UiText::Grid)}</div>
                            <div class="flex flex-wrap gap-1">{origin_x}{origin_y}{spacing_x}{spacing_y}</div>
                        </div>
                        <div class="flex flex-col gap-2">
                            <div class="text-[11px] font-semibold text-[#929299]">{self.ui(UiText::Mask)}</div>
                            <div class="flex flex-wrap gap-1">{mask_none}{mask_circle}{mask_rounded}{mask_square}{mask_diamond}</div>
                        </div>
                        <div class="flex flex-col gap-2">
                            <div class="text-[11px] font-semibold text-[#929299]">{self.ui(UiText::Language)}</div>
                            <div class="flex flex-wrap gap-1">{english}{japanese}{korean}{simplified_chinese}{traditional_chinese}</div>
                        </div>
                        <div class="pt-2 flex items-center justify-between border-t border-[#3a3a3f]">
                            <div class="flex flex-col gap-[2px]">
                                <div class="text-[13px]">
                                    {format!(
                                        "{} · {:.0} × {:.0}",
                                        self.ui(UiText::SnapToGrid),
                                        settings.grid.spacing_x,
                                        settings.grid.spacing_y,
                                    )}
                                </div>
                                <div class="text-[11px] text-[#929299]">{self.ui(UiText::SnapPointerHint)}</div>
                            </div>
                            {snap}
                        </div>
                    </div>
                </div>
            </div>
        }
    }

    /// Read-only, scrollable presentation of the exact filtered KAGE export.
    fn export_sheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let source_lines = export_source_lines(&self.export_source);
        let record_count = if self.export_source.is_empty() {
            0
        } else {
            source_lines.len()
        };
        let copy_label = if self.export_copied {
            self.ui(UiText::Copied)
        } else {
            self.ui(UiText::CopyAll)
        };
        let close = toolbar_button(
            "close-export",
            self.ui(UiText::Done),
            false,
            true,
            cx.listener(|this, _, window, cx| {
                this.set_export_visible(false, window);
                cx.notify();
            }),
        );
        let copy_all = toolbar_button(
            "copy-all-export",
            copy_label,
            true,
            true,
            cx.listener(|this, _, _, cx| this.copy_export_kage(cx)),
        );

        view! {
            <div
                id="export-backdrop"
                occlude
                class="absolute inset-0 flex items-center justify-center bg-[#00000075]"
                @click={cx.listener(|this, _, window, cx| {
                    this.set_export_visible(false, window);
                    cx.notify();
                })}
            >
                <div
                    id="export-sheet"
                    occlude
                    class="w-[720px] h-[560px] flex flex-col overflow-hidden rounded-lg bg-[#242427] border border-[#4a4a50]"
                    @click={|_, _, cx| cx.stop_propagation()}
                >
                    <div class="h-[52px] flex-none px-4 flex items-center justify-between border-b border-[#3b3b40]">
                        <div class="flex items-center gap-2">
                            <div class="px-2 py-1 rounded bg-[#343438] font-mono text-[11px] text-[#a7a7ae]">"KAGE"</div>
                            <div class="text-[15px] font-semibold">{self.ui(UiText::ExportKage)}</div>
                        </div>
                        {close}
                    </div>
                    <div class="flex-1 min-h-0 p-4 flex flex-col gap-3">
                        <div class="flex-none flex items-center justify-between text-[12px] text-[#929299]">
                            <div>{self.ui(UiText::FilteredKageHint)}</div>
                            <div>{format!("{record_count} records · {} bytes", self.export_source.len())}</div>
                        </div>
                        <div
                            id="export-source-scroll"
                            class="flex-1 min-h-0 overflow-scroll rounded-md bg-[#18181a] border border-[#3b3b40]"
                        >
                            <div class="py-2">
                                <div
                                    v-for={(index, line) in source_lines.into_iter().enumerate()}
                                    :key={("export-line", index)}
                                    class="min-h-[24px] flex items-start whitespace-nowrap font-mono text-[12px]"
                                >
                                    <div class="w-[48px] flex-none pr-3 text-right text-[#5f5f66]">{format!("{}", index + 1)}</div>
                                    <div class="pr-4 text-[#d8d8dd]">{line}</div>
                                </div>
                            </div>
                        </div>
                        <div class="h-[34px] flex-none flex items-center justify-between">
                            <div
                                class="text-[12px]"
                                :class={if self.export_copied { "text-[#72c98b]" } else { "text-[#85858c]" }}
                            >
                                <template v-if={self.export_copied}>
                                    {format!("●  {}", self.ui(UiText::Copied))}
                                </template>
                            </div>
                            {copy_all}
                        </div>
                    </div>
                </div>
            </div>
        }
    }

    fn setting_choice(
        &self,
        id: &'static str,
        label: &'static str,
        active: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        view! {
            <div
                :id={id}
                class="h-[32px] px-2 flex items-center justify-center rounded-md border text-[12px] cursor-pointer hover:bg-[#3a3a40] hover:text-white"
                :class={if active {
                    "bg-[#0a84ff] border-[#2a94ff] text-white"
                } else {
                    "bg-[#303034] border-[#424248] text-[#b7b7bd]"
                }}
                @click={handler}
            >
                {label}
            </div>
        }
    }

    fn grid_stepper(
        &self,
        id: &'static str,
        label: &'static str,
        value: f32,
        field: GridField,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let minus = toolbar_button(
            SharedString::from(format!("{id}-minus")),
            "−",
            false,
            true,
            cx.listener(move |this, _, _, cx| {
                this.adjust_grid(field, -1.0);
                cx.notify();
            }),
        );
        let plus = toolbar_button(
            SharedString::from(format!("{id}-plus")),
            "+",
            false,
            true,
            cx.listener(move |this, _, _, cx| {
                this.adjust_grid(field, 1.0);
                cx.notify();
            }),
        );

        view! {
            <div
                :id={id}
                class="w-[196px] h-[32px] pl-2 flex items-center gap-1 rounded-md bg-[#2a2a2e] border border-[#3d3d42]"
            >
                <div class="flex-1 text-[11px] text-[#929299]">{label}</div>
                {minus}
                <div class="w-[32px] text-center font-mono text-[11px]">
                    {format!("{value:.0}")}
                </div>
                {plus}
            </div>
        }
    }

    fn centerline_choice(
        &self,
        id: &'static str,
        label: &'static str,
        mode: CenterlineMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_choice(
            id,
            label,
            self.model.settings().centerline == mode,
            cx.listener(move |this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.centerline = mode;
                this.model.set_settings(settings);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn mask_choice(
        &self,
        id: &'static str,
        label: &'static str,
        mode: MaskMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_choice(
            id,
            label,
            self.model.settings().mask == mode,
            cx.listener(move |this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.mask = mode;
                this.model.set_settings(settings);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn language_choice(
        &self,
        id: &'static str,
        label: &'static str,
        language: UiLanguage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_choice(
            id,
            label,
            self.model.settings().language == language,
            cx.listener(move |this, _, _, cx| {
                let mut settings = *this.model.settings();
                settings.language = language;
                this.model.set_settings(settings);
                this.status = text(language, UiText::ReadyEngineConnected).to_owned();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn status_bar(&self) -> impl IntoElement {
        let pointer = self.pointer.map_or_else(
            || "x —  y —".to_owned(),
            |pointer| format!("x {:>6.1}   y {:>6.1}", pointer.x, pointer.y),
        );
        let engine_status = format!("●  {}", self.ui(UiText::EngineOnline));
        let status = self.status.clone();
        let record_count = format!("{} records", self.model.strokes().len());
        let selection_count = format!("{} selected", self.model.selection().len());

        view! {
            <div class="h-[28px] flex-none px-3 flex items-center gap-3 bg-[#202023] border-t border-[#343438] text-[11px] text-[#929299]">
                <div class="font-semibold text-[#62adff]">{engine_status}</div>
                <div class="w-px h-[11px] bg-[#3a3a3f]" />
                <div class="flex-1 min-w-0 overflow-hidden text-ellipsis">{status}</div>
                {record_count}
                {selection_count}
                <div class="font-mono text-[#9a9aa0]">{pointer}</div>
            </div>
        }
    }

    fn adjust_style(&mut self, is_head: bool, delta: i32) {
        let Some(stroke) = self.model.selected_strokes().next() else {
            return;
        };
        let choices = if is_head {
            stroke.kind().head_shapes()
        } else {
            stroke.kind().tail_shapes()
        };
        if choices.is_empty() {
            return;
        }
        let head = if is_head {
            cycle_style_value(choices, stroke.head(), delta)
        } else {
            stroke.head()
        };
        let tail = if is_head {
            stroke.tail()
        } else {
            cycle_style_value(choices, stroke.tail(), delta)
        };
        let changed = self.model.set_selected_style(head, tail);
        self.status = format!("Updated style on {changed} record(s)");
    }

    fn adjust_grid(&mut self, field: GridField, delta: f32) {
        let mut settings = *self.model.settings();
        match field {
            GridField::OriginX => {
                settings.grid.origin_x = (settings.grid.origin_x + delta).clamp(0.0, 200.0);
            }
            GridField::OriginY => {
                settings.grid.origin_y = (settings.grid.origin_y + delta).clamp(0.0, 200.0);
            }
            GridField::SpacingX => {
                settings.grid.spacing_x = (settings.grid.spacing_x + delta).clamp(2.0, 200.0);
            }
            GridField::SpacingY => {
                settings.grid.spacing_y = (settings.grid.spacing_y + delta).clamp(2.0, 200.0);
            }
        }
        self.model.set_settings(settings);
        self.status = format!(
            "Grid · origin {:.0},{:.0} · spacing {:.0}×{:.0}",
            settings.grid.origin_x,
            settings.grid.origin_y,
            settings.grid.spacing_x,
            settings.grid.spacing_y
        );
    }

    fn apply_transform(
        &mut self,
        label: &'static str,
        transform: impl FnOnce(Point) -> AffineTransform,
    ) {
        if !self.model.can_affine_transform_selection() {
            return;
        }
        let Some(bounds) = self.model.selection_bounds() else {
            return;
        };
        if self
            .model
            .transform_selected(label, transform(bounds.center()), true)
        {
            self.status = label.to_owned();
        }
    }

    /// Enters gesture capture with the same empty-selection contract as the
    /// reference editor. Newly recognized strokes remain unselected so the
    /// next gesture is never obscured by editing furniture.
    fn activate_freehand(&mut self) {
        if self.model.transaction_active() {
            let _ = self.model.cancel_transaction();
        }
        self.model.clear_selection();
        self.tool = Tool::Freehand;
        self.drag = None;
        self.freehand.clear();
        self.status = "Intelligent freehand · draw on the artboard".to_owned();
    }

    fn set_component_stretch_value(&mut self, id: StrokeId, value: i32) {
        match self.model.set_component_stretch_value(id, value) {
            Ok(true) => self.status = "Component stretch updated".to_owned(),
            Ok(false) => self.status = "Component stretch unchanged".to_owned(),
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Clears the query and invalidates any in-flight search result.
    fn clear_component_query(&mut self, cx: &mut Context<Self>) {
        self.set_component_query_and_input(String::new(), cx);
        self.status = "Showing random GlyphWiki recommendations".to_owned();
    }

    /// Mirrors a user-originated input value into search state.
    fn update_component_query(&mut self, value: &str) -> bool {
        let changed = replace_component_query(&mut self.component_query, value);
        if changed {
            self.invalidate_component_search();
        }
        changed
    }

    /// Applies a programmatic query to both the retained input and editor state.
    fn set_component_query_and_input(&mut self, value: String, cx: &mut Context<Self>) {
        self.component_search_input.update(cx, |input, cx| {
            input.set_text(value.clone(), cx);
        });
        self.update_component_query(&value);
    }

    /// Marks submitted results stale after the user edits the query.
    fn invalidate_component_search(&mut self) {
        self.component_search_generation = self.component_search_generation.wrapping_add(1);
        self.component_search = ComponentSearchState::Idle;
    }

    /// Loads a complete random recommendation batch without blocking the UI.
    fn start_component_suggestions(&mut self, cx: &mut Context<Self>) {
        self.component_suggestions_generation =
            self.component_suggestions_generation.wrapping_add(1);
        let generation = self.component_suggestions_generation;
        begin_component_suggestions(&mut self.component_suggestions);

        let client = self.glyphwiki.clone();
        let worker = cx.background_executor().spawn(smol::unblock(move || {
            client.random_names(RANDOM_COMPONENT_COUNT)
        }));
        cx.spawn(async move |this, cx| {
            let result = worker.await.map_err(|error| error.to_string());
            let _ = this.update(cx, |this, cx| {
                if complete_component_suggestions(
                    &mut this.component_suggestions,
                    this.component_suggestions_generation,
                    generation,
                    result,
                ) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Runs a non-blocking `GlyphWiki` search for the current query.
    fn start_component_search(&mut self, cx: &mut Context<Self>) {
        let query = self.component_query.trim().to_owned();
        if query.is_empty() {
            self.clear_component_query(cx);
            cx.notify();
            return;
        }

        self.set_component_query_and_input(query.clone(), cx);
        self.component_search_generation = self.component_search_generation.wrapping_add(1);
        let generation = self.component_search_generation;
        self.component_search = ComponentSearchState::Loading {
            query: query.clone(),
        };
        self.status = format!("Searching GlyphWiki for {query:?}");

        let client = self.glyphwiki.clone();
        let request_query = query.clone();
        let worker = cx
            .background_executor()
            .spawn(smol::unblock(move || client.search(&request_query)));
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.component_search_generation != generation {
                    return;
                }
                match result {
                    Ok(SearchResponse::Matches(names)) => {
                        this.status = format!("GlyphWiki · {} result(s)", names.len());
                        this.component_search = ComponentSearchState::Ready { query, names };
                    }
                    Ok(SearchResponse::TooShort) => {
                        this.status = "GlyphWiki query is too short".to_owned();
                        this.component_search = ComponentSearchState::TooShort { query };
                    }
                    Ok(SearchResponse::NoData) => {
                        this.status = "GlyphWiki returned no matching components".to_owned();
                        this.component_search = ComponentSearchState::NoData { query };
                    }
                    Err(error) => {
                        let message = error.to_string();
                        this.status = format!("GlyphWiki search failed · {message}");
                        this.component_search = ComponentSearchState::Error { query, message };
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Inserts a cached component or resolves the remote dependency closure.
    fn insert_or_load_component(&mut self, name: String, remote: bool, cx: &mut Context<Self>) {
        if self.model.component_library().get(&name).is_some() {
            self.insert_cached_component(&name);
            return;
        }
        if !remote {
            self.status = format!("Component {name} is not available locally");
            return;
        }

        self.component_load_generation = self.component_load_generation.wrapping_add(1);
        let generation = self.component_load_generation;
        self.component_load = ComponentLoadState::Loading { name: name.clone() };
        self.status = format!("Loading {name} from GlyphWiki");

        let client = self.glyphwiki.clone();
        let known = self.model.component_library().clone();
        let request_name = name.clone();
        let worker = cx.background_executor().spawn(smol::unblock(move || {
            client.load_component_tree(&request_name, &known)
        }));
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.component_load_generation != generation {
                    return;
                }
                match result {
                    Ok(definitions) => {
                        let loaded = definitions.len();
                        let mut library = this.model.component_library().clone();
                        for definition in definitions {
                            library.upsert(definition);
                        }
                        this.model.set_component_library(library);
                        this.component_load = ComponentLoadState::Idle;
                        this.insert_cached_component(&name);
                        this.status =
                            format!("Inserted {name} · loaded {loaded} GlyphWiki component(s)");
                    }
                    Err(error) => {
                        let message = error.to_string();
                        this.status = format!("Could not load {name} · {message}");
                        this.component_load = ComponentLoadState::Error {
                            name: name.clone(),
                            message,
                        };
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Inserts a full-frame type-99 reference from the current cache.
    fn insert_cached_component(&mut self, name: &str) {
        match self.model.insert_component(
            name,
            Rect::new(Point::new(0.0, 0.0), Point::new(DESIGN_SIZE, DESIGN_SIZE)),
        ) {
            Ok(_) => {
                self.status = format!("Inserted component {name}");
                self.sidebar = SidebarTab::Inspector;
                self.component_load = ComponentLoadState::Idle;
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn export_kage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_export_visible(true, window);
        self.status = format!(
            "{} · {} bytes",
            self.ui(UiText::ExportKage),
            self.export_source.len()
        );
        cx.notify();
    }

    /// Copies the entire source currently presented by the export sheet.
    fn copy_export_kage(&mut self, cx: &mut Context<Self>) {
        write_clipboard_text(cx, self.export_source.clone());
        self.export_copied = true;
        self.status = format!("{} · KAGE", self.ui(UiText::Copied));
        cx.notify();
    }

    fn import_kage(&mut self, cx: &mut Context<Self>) {
        let Some(source) = read_clipboard_text(cx) else {
            self.status = "Clipboard has no text".to_owned();
            cx.notify();
            return;
        };
        match self.model.load_kage(source.trim()) {
            Ok(()) => self.status = "Imported KAGE source from clipboard".to_owned(),
            Err(error) => self.status = format!("Import failed · {error}"),
        }
        cx.notify();
    }

    fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        if !zoom.is_finite() {
            return;
        }
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.status = format!("Zoom · {}%", (self.zoom * 100.0).round());
        cx.notify();
    }

    fn set_zoom_about(&mut self, zoom: f32, anchor: ScreenPoint, cx: &mut Context<Self>) {
        if !zoom.is_finite() {
            return;
        }
        let next_zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if (next_zoom - self.zoom).abs() <= f32::EPSILON {
            return;
        }
        if let Some(transform) = self
            .canvas_transform
            .get()
            .filter(|transform| transform.canvas_bounds().contains(&anchor))
        {
            self.pan = canvas::pan_for_anchored_zoom(
                transform.canvas_bounds(),
                self.zoom,
                self.pan,
                next_zoom,
                anchor,
            );
        }
        self.zoom = next_zoom;
        self.status = format!("Zoom · {}%", (self.zoom * 100.0).round());
        cx.notify();
    }

    fn design_pointer(&self, screen: ScreenPoint) -> Option<Point> {
        self.canvas_transform
            .get()
            .filter(|transform| transform.canvas_bounds().contains(&screen))
            .map(|transform| transform.screen_to_design(screen))
    }

    fn editing_pointer(&self, pointer: Point) -> Point {
        let snapped = self.model.snap_point(pointer);
        Point::new(snapped.x.round(), snapped.y.round())
    }

    fn freehand_pointer(&self, pointer: Point) -> Point {
        self.model.snap_freehand_point(pointer, 10.0 / self.zoom)
    }

    fn additive_selection(event: &MouseDownEvent) -> bool {
        event.modifiers.shift || event.modifiers.control || event.modifiers.platform
    }

    fn on_canvas_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if canvas_input_blocked(self.modal_visible()) {
            return;
        }
        window.focus(&self.focus);
        let Some(pointer) = self.design_pointer(event.position) else {
            return;
        };
        self.pointer = Some(pointer);
        let edit_pointer = self.editing_pointer(pointer);

        if self.tool == Tool::Freehand {
            self.freehand.clear();
            self.freehand.push(self.freehand_pointer(pointer));
            self.drag = Some(DragGesture::Freehand);
            self.status = "Recognizing gesture…".to_owned();
            cx.notify();
            return;
        }

        let Some(transform) = self.canvas_transform.get() else {
            return;
        };
        if let Some(bounds) = self.model.selection_bounds()
            && canvas::selection_uses_resize_handles(&self.model)
            && let Some(handle) = canvas::hit_resize_handle(bounds, pointer, transform)
        {
            let _ = self.model.begin_transaction("Resize selection");
            self.drag = Some(DragGesture::Resize {
                handle,
                source: bounds,
                origin: edit_pointer,
                frame: frame_resize(&self.model, handle),
            });
            self.status = "Resize selection".to_owned();
            cx.notify();
            return;
        }

        if let Some(control) = canvas::hit_selected_control_point(
            &self.model,
            pointer,
            canvas::control_hit_tolerance(transform),
        ) {
            let _ = self.model.begin_transaction("Move control point");
            self.drag = Some(DragGesture::Control { control });
            self.status = format!("Control point {}", control.point + 1);
            cx.notify();
            return;
        }

        let additive = Self::additive_selection(event);
        if let Some(stroke) = canvas::hit_test_rendered(&self.model, pointer, 5.0 / self.zoom) {
            let was_selected = self.model.is_selected(stroke);
            if let Some(mode) = stroke_selection_change(was_selected, additive) {
                self.model.select(stroke, mode);
            }
            if !self.model.selection().is_empty() {
                if canvas::selection_has_transformed_geometry(&self.model) {
                    self.status =
                        "Transformed outline selected · edit or remove the later type-0 operation"
                            .to_owned();
                } else {
                    let _ = self.model.begin_transaction("Move selection");
                    self.drag = Some(DragGesture::Move { last: edit_pointer });
                    self.status = "Move selection".to_owned();
                }
            }
        } else {
            self.drag = Some(DragGesture::Marquee {
                origin: pointer,
                current: pointer,
            });
            self.status = "Marquee selection".to_owned();
        }
        cx.notify();
    }

    fn on_canvas_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if canvas_input_blocked(self.modal_visible()) {
            return;
        }
        let Some(pointer) = self.design_pointer(event.position) else {
            self.pointer = None;
            cx.notify();
            return;
        };
        self.pointer = Some(pointer);
        if !event.dragging() {
            cx.notify();
            return;
        }
        let edit_pointer = self.editing_pointer(pointer);
        let freehand_pointer = self.freehand_pointer(pointer);

        match self.drag.as_mut() {
            Some(DragGesture::Move { last }) => {
                let delta = Point::new(edit_pointer.x - last.x, edit_pointer.y - last.y);
                self.model.move_selected(delta, true);
                *last = edit_pointer;
            }
            Some(DragGesture::Control { control }) => {
                self.model.move_control_point(*control, edit_pointer, true);
            }
            Some(DragGesture::Resize {
                handle,
                source,
                origin,
                frame,
            }) => {
                if let Some(frame) = frame {
                    let (first, second) = resize_frame_target(*frame, *origin, edit_pointer);
                    self.model.resize_frame_record(frame.stroke, first, second);
                } else {
                    let target = resize_target(*source, *handle, *origin, edit_pointer);
                    self.model.preview_resize_selected(target, false);
                }
            }
            Some(DragGesture::Marquee { current, .. }) => *current = pointer,
            Some(DragGesture::Freehand) => {
                if self
                    .freehand
                    .last()
                    .is_none_or(|last| last.distance(freehand_pointer) >= 0.8)
                {
                    self.freehand.push(freehand_pointer);
                }
            }
            None => {}
        }
        cx.notify();
    }

    fn on_canvas_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if canvas_input_blocked(self.modal_visible()) {
            // This callback is also installed as `mouse-up-out`, which GPUI
            // dispatches during capture. Stopping propagation here would
            // prevent the modal's choice buttons from receiving their click.
            return;
        }
        let gesture = self.drag.take();
        match gesture {
            Some(DragGesture::Move { .. })
            | Some(DragGesture::Control { .. })
            | Some(DragGesture::Resize { .. }) => {
                let _ = self.model.commit_transaction();
                self.status = "Geometry updated".to_owned();
            }
            Some(DragGesture::Marquee { origin, current }) => {
                if origin.distance(current) < 1.0 {
                    self.model.clear_selection();
                } else {
                    let records = canvas::records_intersecting_rendered(
                        &self.model,
                        Rect::new(origin, current),
                    );
                    apply_record_selection(&mut self.model, &records, SelectionMode::Add);
                }
                self.status = format!("Selected {} record(s)", self.model.selection().len());
            }
            Some(DragGesture::Freehand) => {
                if let Some(pointer) = self.design_pointer(event.position) {
                    let edit_pointer = self.freehand_pointer(pointer);
                    if self
                        .freehand
                        .last()
                        .is_none_or(|last| last.distance(edit_pointer) >= 0.8)
                    {
                        self.freehand.push(edit_pointer);
                    }
                }
                if let Some(recognized) = self.model.insert_gesture(&self.freehand) {
                    self.model.clear_selection();
                    self.status = format!(
                        "Recognized {} · {}% confidence",
                        gesture_kind_name(recognized.kind()),
                        (recognized.confidence() * 100.0).round()
                    );
                } else {
                    self.status = "Gesture was too short — try a longer stroke".to_owned();
                }
                self.freehand.clear();
            }
            None => {}
        }
        cx.notify();
    }

    fn on_canvas_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if canvas_input_blocked(self.modal_visible()) {
            return;
        }
        let delta = event.delta.pixel_delta(px(16.0));
        if event.modifiers.secondary() {
            let vertical = f32::from(delta.y);
            if vertical.abs() > f32::EPSILON {
                let direction = if vertical > 0.0 { -1.0 } else { 1.0 };
                self.set_zoom_about(self.zoom + direction * 0.12, event.position, cx);
            }
            cx.stop_propagation();
            return;
        }
        let Some(transform) = self.canvas_transform.get() else {
            return;
        };
        let scale = transform.scale().max(f32::EPSILON);
        let horizontal = if event.modifiers.shift && f32::from(delta.x).abs() < f32::EPSILON {
            f32::from(delta.y)
        } else {
            f32::from(delta.x)
        };
        let vertical = if event.modifiers.shift {
            0.0
        } else {
            f32::from(delta.y)
        };
        self.pan = self
            .pan
            .offset(Point::new(-horizontal / scale, -vertical / scale));
        self.status = "Pan view · hold ⌘/Ctrl and scroll to zoom".to_owned();
        cx.stop_propagation();
        cx.notify();
    }

    fn on_canvas_pinch(
        &mut self,
        event: &PinchEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if canvas_input_blocked(self.modal_visible()) {
            return;
        }
        if let Some(zoom) = zoom_after_pinch(self.zoom, event.delta) {
            self.set_zoom_about(zoom, event.position, cx);
        }
        cx.stop_propagation();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.to_lowercase();
        let modifiers = event.keystroke.modifiers;

        if key == "escape" && self.modal_visible() {
            if self.show_export {
                self.set_export_visible(false, window);
            } else {
                self.set_settings_visible(false, window);
            }
            self.status = "Cancelled".to_owned();
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if self.modal_visible() {
            cx.stop_propagation();
            return;
        }

        if self
            .component_search_input
            .read(cx)
            .focus_handle()
            .is_focused(window)
        {
            return;
        }

        if key == "escape" {
            if self.tool == Tool::Freehand {
                self.tool = Tool::Select;
                self.freehand.clear();
                self.drag = None;
            } else if self.model.transaction_active() {
                let _ = self.model.cancel_transaction();
                self.drag = None;
            } else {
                self.model.clear_selection();
            }
            self.status = "Cancelled".to_owned();
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if let Some(direction) = control_nudge_direction(&key, modifiers.control) {
            self.nudge(direction, modifiers.shift);
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if modifiers.secondary() {
            match key.as_str() {
                "a" => {
                    self.model.select_all();
                    self.status = "Selected all records".to_owned();
                }
                "i" => {
                    self.model.invert_selection();
                    self.status = "Inverted selection".to_owned();
                }
                "z" if modifiers.shift => {
                    self.model.redo();
                    self.status = "Redo".to_owned();
                }
                "z" => {
                    self.model.undo();
                    self.status = "Undo".to_owned();
                }
                "y" => {
                    self.model.redo();
                    self.status = "Redo".to_owned();
                }
                "x" => {
                    let count = self.model.cut_selected();
                    self.status = format!("Cut {count} record(s)");
                }
                "c" => {
                    let count = self.model.copy_selected();
                    self.status = format!("Copied {count} record(s)");
                }
                "v" => {
                    let count = self.model.paste().len();
                    self.status = format!("Pasted {count} record(s)");
                }
                "s" => self.export_kage(window, cx),
                "o" => self.import_kage(cx),
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        match key.as_str() {
            "delete" | "backspace" => {
                let count = self.model.delete_selected();
                self.status = format!("Deleted {count} record(s)");
            }
            "arrowleft" | "left" => self.nudge(Point::new(-1.0, 0.0), modifiers.shift),
            "arrowright" | "right" => self.nudge(Point::new(1.0, 0.0), modifiers.shift),
            "arrowup" | "up" => self.nudge(Point::new(0.0, -1.0), modifiers.shift),
            "arrowdown" | "down" => self.nudge(Point::new(0.0, 1.0), modifiers.shift),
            "g" => {
                let mut settings = *self.model.settings();
                settings.grid.visible = !settings.grid.visible;
                self.model.set_settings(settings);
            }
            "f" => self.activate_freehand(),
            "v" => self.tool = Tool::Select,
            "0" => {
                self.zoom = 1.0;
                self.pan = Point::new(0.0, 0.0);
                self.status = "View reset".to_owned();
            }
            "/" => {
                self.sidebar = SidebarTab::Components;
                self.component_search_input.read(cx).focus(window);
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn nudge(&mut self, direction: Point, coarse: bool) {
        if canvas::selection_has_transformed_geometry(&self.model) {
            self.status =
                "Transformed outline is read-only until its later type-0 operation is removed"
                    .to_owned();
            return;
        }
        let distance = if coarse { 5.0 } else { 1.0 };
        if self.model.move_selected(
            Point::new(direction.x * distance, direction.y * distance),
            true,
        ) {
            self.status = format!("Nudged selection by {distance:.0}");
        }
    }
}

fn component_preview(source: String, typeface: Typeface, use_curve: bool) -> impl IntoElement {
    let preview = component_preview_model(&source, typeface, use_curve);
    let snapshot = CanvasSnapshot::from_model(
        &preview,
        1.0,
        CanvasOverlay {
            mode: InteractionMode::FinalGlyph,
            ..CanvasOverlay::default()
        },
    );
    let drawing = canvas::canvas_element(snapshot, |_, _, _, _| {});
    view! {
        <div class="h-[82px] bg-[#e9e9ea]">{drawing}</div>
    }
}

/// Builds a local component preview with the document's renderer settings.
fn component_preview_model(source: &str, typeface: Typeface, use_curve: bool) -> EditorModel {
    let mut preview = EditorModel::from_kage(source).unwrap_or_default();
    let mut settings = *preview.settings();
    settings.typeface = match typeface {
        Typeface::Skeleton => Typeface::Mincho,
        rendered => rendered,
    };
    settings.use_curve = use_curve;
    preview.set_settings(settings);
    preview
}

/// Displays `GlyphWiki`'s official thumbnail while KAGE source remains uncached.
fn remote_component_preview(name: String, loading: bool) -> impl IntoElement {
    let url = thumbnail_url(&name);
    view! {
        <div class="h-[82px] relative flex items-center justify-center bg-[#e9e9ea]">
            <div class="relative w-[56px] h-[56px] flex items-center justify-center rounded bg-[#f8f8f6] text-[11px] font-bold text-[#a0a0a3]">
                "GW"
                <img :src={url} class="absolute top-[3px] left-[3px] w-[50px] h-[50px]" />
            </div>
            <div v-if={loading} class="absolute inset-0 flex items-center justify-center bg-[#e9e9eacc] text-[12px] font-semibold text-[#55555a]">
                "Loading…"
            </div>
        </div>
    }
}

/// Converts AppKit/GPUI's per-event magnification into a proportional zoom.
fn zoom_after_pinch(current_zoom: f32, magnification: f32) -> Option<f32> {
    let factor = 1.0 + magnification;
    let next = current_zoom * factor;
    (current_zoom.is_finite()
        && current_zoom > 0.0
        && magnification.is_finite()
        && factor > 0.0
        && next.is_finite())
    .then_some(next)
}

/// Converts the engine's compact `$` separators into copy-ready text lines.
fn format_export_kage(source: &str) -> String {
    source.replace('$', "\n")
}

/// Splits the already formatted export into display lines.
fn export_source_lines(source: &str) -> Vec<String> {
    if source.is_empty() {
        return vec![String::new()];
    }

    source.lines().map(str::to_owned).collect()
}

/// Replaces the search query only when a user edit changed its Unicode value.
fn replace_component_query(current: &mut String, next: &str) -> bool {
    if current == next {
        return false;
    }
    current.clear();
    current.push_str(next);
    true
}

/// Treats the toolbar centerline control as a true show/hide toggle.
///
/// The settings sheet still exposes the full three-state policy. From the
/// toolbar, either visible policy is switched off in one click, while enabling
/// centerlines uses `Always` so the result is immediately visible even when no
/// record is selected.
const fn toggled_toolbar_centerline(current: CenterlineMode) -> CenterlineMode {
    match current {
        CenterlineMode::None => CenterlineMode::Always,
        CenterlineMode::Selection | CenterlineMode::Always => CenterlineMode::None,
    }
}

/// Modal surfaces own all pointer, wheel, gesture, and editing-key input.
const fn canvas_input_blocked(modal_visible: bool) -> bool {
    modal_visible
}

fn stroke_selection_change(was_selected: bool, additive: bool) -> Option<SelectionMode> {
    match (was_selected, additive) {
        (true, false) => None,
        (true, true) => Some(SelectionMode::Remove),
        (false, true) => Some(SelectionMode::Add),
        (false, false) => Some(SelectionMode::Replace),
    }
}

fn apply_record_selection(model: &mut EditorModel, records: &[StrokeId], mode: SelectionMode) {
    if mode == SelectionMode::Replace {
        model.clear_selection();
    }
    let item_mode = match mode {
        SelectionMode::Replace | SelectionMode::Add => SelectionMode::Add,
        SelectionMode::Toggle => SelectionMode::Toggle,
        SelectionMode::Remove => SelectionMode::Remove,
    };
    for &record in records {
        model.select(record, item_mode);
    }
}

fn frame_resize(model: &EditorModel, handle: ResizeHandle) -> Option<FrameResize> {
    if model.selection().len() != 1 {
        return None;
    }
    let stroke = model.stroke(*model.selection().iter().next()?)?;
    if !matches!(
        stroke.kind(),
        StrokeKind::Metadata | StrokeKind::Transform | StrokeKind::Component
    ) {
        return None;
    }
    let [first, second] = stroke.points() else {
        return None;
    };
    let west = if first.x <= second.x {
        FramePoint::First
    } else {
        FramePoint::Second
    };
    let east = if west == FramePoint::First {
        FramePoint::Second
    } else {
        FramePoint::First
    };
    let north = if first.y <= second.y {
        FramePoint::First
    } else {
        FramePoint::Second
    };
    let south = if north == FramePoint::First {
        FramePoint::Second
    } else {
        FramePoint::First
    };
    let x_point = match handle {
        ResizeHandle::NorthWest | ResizeHandle::West | ResizeHandle::SouthWest => Some(west),
        ResizeHandle::NorthEast | ResizeHandle::East | ResizeHandle::SouthEast => Some(east),
        ResizeHandle::North | ResizeHandle::South => None,
    };
    let y_point = match handle {
        ResizeHandle::NorthWest | ResizeHandle::North | ResizeHandle::NorthEast => Some(north),
        ResizeHandle::SouthWest | ResizeHandle::South | ResizeHandle::SouthEast => Some(south),
        ResizeHandle::East | ResizeHandle::West => None,
    };
    Some(FrameResize {
        stroke: stroke.id(),
        first: *first,
        second: *second,
        x_point,
        y_point,
    })
}

fn resize_frame_target(frame: FrameResize, origin: Point, pointer: Point) -> (Point, Point) {
    let delta = Point::new(pointer.x - origin.x, pointer.y - origin.y);
    let mut first = frame.first;
    let mut second = frame.second;
    match frame.x_point {
        Some(FramePoint::First) => first.x += delta.x,
        Some(FramePoint::Second) => second.x += delta.x,
        None => {}
    }
    match frame.y_point {
        Some(FramePoint::First) => first.y += delta.y,
        Some(FramePoint::Second) => second.y += delta.y,
        None => {}
    }
    (first, second)
}

fn control_nudge_direction(key: &str, control: bool) -> Option<Point> {
    if !control {
        return None;
    }
    match key {
        "h" => Some(Point::new(-1.0, 0.0)),
        "j" => Some(Point::new(0.0, 1.0)),
        "k" => Some(Point::new(0.0, -1.0)),
        "l" => Some(Point::new(1.0, 0.0)),
        _ => None,
    }
}

fn resize_target(source: Rect, handle: ResizeHandle, origin: Point, pointer: Point) -> Rect {
    let delta = Point::new(pointer.x - origin.x, pointer.y - origin.y);
    let mut min = source.min;
    let mut max = source.max;
    match handle {
        ResizeHandle::NorthWest => min = min.offset(delta),
        ResizeHandle::North => min.y += delta.y,
        ResizeHandle::NorthEast => {
            min.y += delta.y;
            max.x += delta.x;
        }
        ResizeHandle::East => max.x += delta.x,
        ResizeHandle::SouthEast => max = max.offset(delta),
        ResizeHandle::South => max.y += delta.y,
        ResizeHandle::SouthWest => {
            min.x += delta.x;
            max.y += delta.y;
        }
        ResizeHandle::West => min.x += delta.x,
    }
    let minimum = 20.0;
    if max.x - min.x < minimum {
        if matches!(
            handle,
            ResizeHandle::NorthWest | ResizeHandle::SouthWest | ResizeHandle::West
        ) {
            min.x = max.x - minimum;
        } else {
            max.x = min.x + minimum;
        }
    }
    if max.y - min.y < minimum {
        if matches!(
            handle,
            ResizeHandle::NorthWest | ResizeHandle::North | ResizeHandle::NorthEast
        ) {
            min.y = max.y - minimum;
        } else {
            max.y = min.y + minimum;
        }
    }
    Rect::new(min, max)
}

fn cycle_style_value(choices: &[i32], current: i32, delta: i32) -> i32 {
    let current_index = choices
        .iter()
        .position(|choice| *choice == current)
        .unwrap_or(0);
    let next = if delta < 0 {
        if current_index == 0 {
            choices.len() - 1
        } else {
            current_index - 1
        }
    } else {
        (current_index + 1) % choices.len()
    };
    choices[next]
}

fn kind_name(kind: StrokeKind) -> &'static str {
    match kind {
        StrokeKind::Metadata => "Metadata",
        StrokeKind::Line => "Line",
        StrokeKind::Curve => "Curve",
        StrokeKind::Bend => "Bend",
        StrokeKind::Corner => "Corner",
        StrokeKind::Bezier => "Bézier",
        StrokeKind::Sweep => "Sweep",
        StrokeKind::Transform => "Transform",
        StrokeKind::Component => "Component",
    }
}

const fn gesture_kind_name(kind: GestureKind) -> &'static str {
    match kind {
        GestureKind::Line => "Line",
        GestureKind::Curve => "Curve",
        GestureKind::Bend => "Bend",
        GestureKind::Sweep => "Sweep",
        GestureKind::LeftHook => "Left hook",
        GestureKind::RightHook => "Right hook",
        GestureKind::UpHook => "Up hook",
    }
}

const fn kind_text(kind: StrokeKind) -> UiText {
    match kind {
        StrokeKind::Metadata => UiText::Metadata,
        StrokeKind::Line => UiText::AddLine,
        StrokeKind::Curve => UiText::AddCurve,
        StrokeKind::Bend => UiText::StrokeBend,
        StrokeKind::Corner => UiText::StrokeCorner,
        StrokeKind::Bezier => UiText::StrokeBezier,
        StrokeKind::Sweep => UiText::StrokeSweep,
        StrokeKind::Transform => UiText::Transform,
        StrokeKind::Component => UiText::Component,
    }
}

/// Iterates layer rows in their original KAGE record order.
fn layer_rows(model: &EditorModel) -> impl Iterator<Item = (usize, &model::Stroke)> {
    model.strokes().iter().enumerate()
}

fn kage_transform_name(transform: KageTransform) -> &'static str {
    match transform {
        KageTransform::FlipVertical => "Flip top-bottom",
        KageTransform::FlipHorizontal => "Flip left-right",
        KageTransform::Rotate90 => "Rotate 90°",
        KageTransform::Rotate180 => "Rotate 180°",
        KageTransform::Rotate270 => "Rotate 270°",
    }
}

const fn inspector_transform_sections(
    selection_count: usize,
    can_affine_transform: bool,
) -> (bool, bool) {
    (can_affine_transform, selection_count == 1)
}

fn kage_transform_choices(
    active: Option<KageTransform>,
    flip_horizontal: &str,
    flip_vertical: &str,
) -> [(usize, KageTransform, String, bool); 5] {
    [
        (
            0,
            KageTransform::FlipHorizontal,
            flip_horizontal.to_owned(),
            active == Some(KageTransform::FlipHorizontal),
        ),
        (
            1,
            KageTransform::FlipVertical,
            flip_vertical.to_owned(),
            active == Some(KageTransform::FlipVertical),
        ),
        (
            2,
            KageTransform::Rotate90,
            "↻ 90°".to_owned(),
            active == Some(KageTransform::Rotate90),
        ),
        (
            3,
            KageTransform::Rotate180,
            "↻ 180°".to_owned(),
            active == Some(KageTransform::Rotate180),
        ),
        (
            4,
            KageTransform::Rotate270,
            "↻ 270°".to_owned(),
            active == Some(KageTransform::Rotate270),
        ),
    ]
}

// #region kage_editor_main
/// Opens the native KAGE Editor desktop window.
fn main() {
    let window = WindowConfig::new("KAGE Editor", 1280.0, 820.0)
        .min_size(980.0, 680.0)
        .transparent_titlebar(true)
        .traffic_light_position(14.0, 17.0);
    DesktopApp::new(window)
        .assets(EmbeddedAssets::new().with_file(
            APP_ICON_ASSET,
            include_bytes!("../assets/KageEditorToolbar@2x.png"),
        ))
        .http_client(GlyphWikiHttpClient::new())
        .run_component(|_, cx| KageEditor::new(KageEditorProps::new(), cx));
}
// #endregion kage_editor_main

#[cfg(test)]
mod interaction_tests {
    use super::*;

    #[test]
    fn pinch_magnification_scales_proportionally() {
        assert_eq!(zoom_after_pinch(1.0, 0.25), Some(1.25));
        assert_eq!(zoom_after_pinch(2.0, 0.25), Some(2.5));
        assert_eq!(zoom_after_pinch(2.0, -0.25), Some(1.5));
    }

    #[test]
    fn invalid_pinch_magnification_is_ignored() {
        assert_eq!(zoom_after_pinch(1.0, -1.0), None);
        assert_eq!(zoom_after_pinch(1.0, f32::NAN), None);
        assert_eq!(zoom_after_pinch(f32::INFINITY, 0.1), None);
    }

    #[test]
    fn settings_modal_blocks_canvas_input() {
        assert!(canvas_input_blocked(true));
        assert!(!canvas_input_blocked(false));
    }

    #[test]
    fn toolbar_centerline_is_an_immediate_two_state_toggle() {
        assert_eq!(
            toggled_toolbar_centerline(CenterlineMode::None),
            CenterlineMode::Always
        );
        assert_eq!(
            toggled_toolbar_centerline(CenterlineMode::Selection),
            CenterlineMode::None
        );
        assert_eq!(
            toggled_toolbar_centerline(CenterlineMode::Always),
            CenterlineMode::None
        );
    }

    #[test]
    fn unsupported_single_selection_keeps_only_order_controls() {
        assert_eq!(inspector_transform_sections(1, false), (false, true));
        assert_eq!(inspector_transform_sections(2, false), (false, false));
        assert_eq!(inspector_transform_sections(2, true), (true, false));
    }

    #[test]
    fn kage_transform_choices_label_rotations_and_mark_only_the_active_operation() {
        let choices = kage_transform_choices(
            Some(KageTransform::Rotate180),
            "Flip left-right",
            "Flip top-bottom",
        );
        let labels = choices
            .iter()
            .map(|(_, _, label, _)| label.as_str())
            .collect::<Vec<_>>();
        let active = choices
            .iter()
            .filter_map(|(_, transform, _, active)| active.then_some(*transform))
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            [
                "Flip left-right",
                "Flip top-bottom",
                "↻ 90°",
                "↻ 180°",
                "↻ 270°",
            ]
        );
        assert_eq!(active, [KageTransform::Rotate180]);
        assert_eq!(KageTransform::Rotate180.parameters(), (99, 2));
    }

    #[test]
    fn export_source_uses_newlines_for_both_display_and_copying() {
        let source = "1:0:2:20:40:80:40$2:7:8:66:13:102:23:120:43";
        let exported = format_export_kage(source);
        let lines = export_source_lines(&exported);

        assert_eq!(
            lines,
            [
                "1:0:2:20:40:80:40".to_owned(),
                "2:7:8:66:13:102:23:120:43".to_owned(),
            ]
        );
        assert_eq!(exported, lines.join("\n"));
        assert!(!exported.contains('$'));
        assert_eq!(export_source_lines(""), [String::new()]);
    }

    #[test]
    fn stroke_pointer_selection_matches_add_remove_drag_rules() {
        assert_eq!(stroke_selection_change(true, false), None);
        assert_eq!(
            stroke_selection_change(true, true),
            Some(SelectionMode::Remove)
        );
        assert_eq!(
            stroke_selection_change(false, true),
            Some(SelectionMode::Add)
        );
        assert_eq!(
            stroke_selection_change(false, false),
            Some(SelectionMode::Replace)
        );
    }

    #[test]
    fn ordinary_resize_cannot_flip_or_shrink_below_twenty() {
        let source = Rect::new(Point::new(20.0, 20.0), Point::new(100.0, 100.0));
        let target = resize_target(
            source,
            ResizeHandle::NorthWest,
            Point::new(20.0, 20.0),
            Point::new(140.0, 150.0),
        );
        assert_eq!(target.min, Point::new(80.0, 80.0));
        assert_eq!(target.max, Point::new(100.0, 100.0));
    }

    #[test]
    fn special_frame_resize_can_cross_both_axes() {
        let mut model =
            EditorModel::from_kage("99:0:0:20:30:180:170:u53e3").expect("component fixture");
        let id = model.strokes()[0].id();
        model.select(id, SelectionMode::Replace);
        let frame = frame_resize(&model, ResizeHandle::NorthWest).expect("frame resize");
        let (first, second) =
            resize_frame_target(frame, Point::new(20.0, 30.0), Point::new(220.0, 210.0));
        assert_eq!(first, Point::new(220.0, 210.0));
        assert_eq!(second, Point::new(180.0, 170.0));
    }

    #[test]
    fn control_hjkl_routing_precedes_platform_shortcuts() {
        assert_eq!(
            control_nudge_direction("h", true),
            Some(Point::new(-1.0, 0.0))
        );
        assert_eq!(
            control_nudge_direction("j", true),
            Some(Point::new(0.0, 1.0))
        );
        assert_eq!(control_nudge_direction("h", false), None);
        assert_eq!(control_nudge_direction("c", true), None);
    }

    #[test]
    fn layer_rows_follow_first_to_last_record_order() {
        let model = EditorModel::demo();
        let stored = model
            .strokes()
            .iter()
            .map(model::Stroke::id)
            .collect::<Vec<_>>();
        let displayed = layer_rows(&model)
            .map(|(_, stroke)| stroke.id())
            .collect::<Vec<_>>();

        assert_eq!(displayed, stored);
        assert_eq!(displayed.len(), 7);
    }

    #[test]
    fn component_previews_inherit_curve_rendering() {
        let preview =
            component_preview_model("2:0:0:20:100:100:40:180:100", Typeface::Skeleton, true);

        assert_eq!(preview.settings().typeface, Typeface::Mincho);
        assert!(preview.settings().use_curve);
    }

    #[test]
    fn component_query_sync_preserves_unicode_and_ignores_equal_events() {
        let mut query = "yong".to_owned();
        assert!(replace_component_query(&mut query, "永"));
        assert_eq!(query, "永");
        assert!(!replace_component_query(&mut query, "永"));
    }

    fn recommendation_names(prefix: &str) -> Vec<String> {
        (0..RANDOM_COMPONENT_COUNT)
            .map(|index| format!("{prefix}-{index:02}"))
            .collect()
    }

    #[test]
    fn recommendation_refresh_keeps_old_cards_until_atomic_commit() {
        let old_names = recommendation_names("old");
        let new_names = recommendation_names("new");
        let mut state = ComponentSuggestionsState::Ready {
            names: old_names.clone(),
        };

        begin_component_suggestions(&mut state);
        assert_eq!(state.names(), old_names);
        assert!(complete_component_suggestions(
            &mut state,
            7,
            7,
            Ok(new_names.clone()),
        ));
        assert_eq!(state.names(), new_names);
        assert!(matches!(state, ComponentSuggestionsState::Ready { .. }));
    }

    #[test]
    fn stale_recommendation_response_cannot_replace_newer_generation() {
        let current_names = recommendation_names("current");
        let stale_names = recommendation_names("stale");
        let mut state = ComponentSuggestionsState::Ready {
            names: current_names.clone(),
        };

        assert!(!complete_component_suggestions(
            &mut state,
            3,
            2,
            Ok(stale_names),
        ));
        assert_eq!(state.names(), current_names);
    }

    #[test]
    fn failed_or_incomplete_recommendation_refresh_retains_last_batch() {
        let old_names = recommendation_names("old");
        let mut state = ComponentSuggestionsState::Ready {
            names: old_names.clone(),
        };

        begin_component_suggestions(&mut state);
        assert!(complete_component_suggestions(
            &mut state,
            5,
            5,
            Err("network unavailable".to_owned()),
        ));
        assert_eq!(state.names(), old_names);
        assert!(matches!(
            state,
            ComponentSuggestionsState::Error { ref message, .. }
                if message == "network unavailable"
        ));

        begin_component_suggestions(&mut state);
        assert!(complete_component_suggestions(
            &mut state,
            6,
            6,
            Ok(vec!["only-one".to_owned()]),
        ));
        assert_eq!(state.names(), old_names);
        assert!(matches!(state, ComponentSuggestionsState::Error { .. }));
    }

    #[test]
    fn composing_query_filters_random_names_without_consuming_batch() {
        let names = vec!["u6c38".to_owned(), "u6c38-j".to_owned(), "u6728".to_owned()];

        assert_eq!(
            filter_component_suggestion_names(&names, "6C38"),
            ["u6c38".to_owned(), "u6c38-j".to_owned()]
        );
        assert_eq!(filter_component_suggestion_names(&names, ""), names);
    }

    #[test]
    fn component_search_does_not_reimplement_platform_text_input() {
        let source = include_str!("main.rs");
        let manual_key_character = ["keystroke", ".", "key_char"].concat();
        let manual_query_append = ["component_query", ".", "push_str"].concat();

        assert!(!source.contains(&manual_key_character));
        assert!(!source.contains(&manual_query_append));
        assert!(source.contains("TextInputEvent::Change"));
        assert!(source.contains("TextInputEvent::Submit"));
        assert!(source.contains("TextInputEvent::Escape"));
    }

    #[test]
    fn ordinary_editor_ui_stays_on_the_gpui_vue_template_surface() {
        let imperative_div = ["div", "()"].concat();
        let imperative_child = [".", "child", "("].concat();
        let direct_gpui = ["gpui", "::"].concat();

        for source in [include_str!("main.rs"), include_str!("controls.rs")] {
            assert!(!source.contains(&imperative_div));
            assert!(!source.contains(&imperative_child));
            assert!(!source.contains(&direct_gpui));
        }
    }

    #[test]
    fn branded_icon_is_embedded_through_gpui_vue_assets() {
        let source = include_str!("main.rs");

        assert!(source.contains("EmbeddedAssets::new().with_file"));
        assert!(source.contains("KageEditorToolbar@2x.png"));
        assert!(include_bytes!("../assets/KageEditorToolbar@2x.png").len() > 1024);
    }

    #[test]
    fn style_stepper_cycles_only_valid_family_values() {
        let choices = StrokeKind::Curve.head_shapes();
        assert_eq!(cycle_style_value(choices, 0, 1), 32);
        assert_eq!(cycle_style_value(choices, 0, -1), 27);
        assert_eq!(cycle_style_value(choices, 999, 1), 32);
    }
}
