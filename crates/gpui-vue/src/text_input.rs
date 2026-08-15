//! Native single-line text input with platform IME composition.
//!
//! [`TextInput`] owns UTF-8 text and selection state while implementing GPUI's
//! UTF-16 platform input contract internally. Applications interact with the
//! typed [`TextInputHandle`] and [`TextInputEvent`] surface; they never need to
//! register an input handler or reach into GPUI's low-level painting API.

use std::{borrow::Cow, ops::Range};

use gpui::{
    App, AppContext as _, Bounds, Context, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, Font, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, KeyDownEvent, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement as _, Pixels, Point,
    Render, ShapedLine, Style, Styled as _, Subscription, TextRun, UTF16Selection, UnderlineStyle,
    Window, fill, font, hsla, point, px, relative, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::{Ref, ui};

/// A retained handle to one native [`TextInput`].
///
/// The alias intentionally hides the underlying host entity type from
/// application state declarations.
pub type TextInputHandle = Entity<TextInput>;

/// Creates a retained native single-line text input in a parent context.
#[must_use]
pub fn text_input<Parent: 'static>(
    placeholder: impl Into<String>,
    cx: &mut Context<'_, Parent>,
) -> TextInputHandle {
    text_input_with_config(TextInputConfig::new(placeholder), cx)
}

/// Creates a retained native single-line input from a typed configuration.
///
/// Use this constructor when the initial value, visual treatment, editing
/// policy, or grapheme limit is known at mount time. Later controlled updates
/// remain available through the corresponding [`TextInput`] setters.
#[must_use]
pub fn text_input_with_config<Parent: 'static>(
    config: TextInputConfig,
    cx: &mut Context<'_, Parent>,
) -> TextInputHandle {
    cx.new(|cx| TextInput::with_config(config, cx))
}

/// Typed visual treatment for a native [`TextInput`].
///
/// The style owns ordinary data rather than a GPUI element builder, so it can
/// be stored in component state and reused without exposing input-handler
/// internals. Dimensions are logical pixels. A missing width fills the
/// available horizontal space.
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputStyle {
    /// Fixed width, or `None` to fill the available width.
    width: Option<Pixels>,
    /// Outer control height.
    height: Pixels,
    /// Horizontal space between the border and editable line.
    padding_x: Pixels,
    /// Vertical space between the border and editable line.
    padding_y: Pixels,
    /// Resting control fill.
    background_color: gpui::Hsla,
    /// Committed and marked text color.
    text_color: gpui::Hsla,
    /// Empty-value placeholder color.
    placeholder_color: gpui::Hsla,
    /// Resting border color.
    border_color: gpui::Hsla,
    /// Exact-focus border color.
    focus_border_color: gpui::Hsla,
    /// Selected text background color.
    selection_color: gpui::Hsla,
    /// Collapsed selection caret color.
    caret_color: gpui::Hsla,
    /// Uniform border width.
    border_width: Pixels,
    /// Uniform corner radius.
    corner_radius: Pixels,
    /// Font family, weight, style, features, and fallbacks.
    font: Font,
    /// Shaping size for the editable line.
    font_size: Pixels,
    /// Opacity multiplier applied when the control is disabled.
    disabled_opacity: f32,
}

impl TextInputStyle {
    /// Uses a fixed logical-pixel width instead of filling the parent.
    #[must_use]
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width.max(px(0.0)));
        self
    }

    /// Restores fill-width layout.
    #[must_use]
    pub fn fill_width(mut self) -> Self {
        self.width = None;
        self
    }

    /// Sets the outer control height in logical pixels.
    #[must_use]
    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height.max(px(0.0));
        self
    }

    /// Sets equal horizontal and vertical padding.
    #[must_use]
    pub fn padding(mut self, padding: Pixels) -> Self {
        let padding = padding.max(px(0.0));
        self.padding_x = padding;
        self.padding_y = padding;
        self
    }

    /// Sets horizontal padding.
    #[must_use]
    pub fn padding_x(mut self, padding: Pixels) -> Self {
        self.padding_x = padding.max(px(0.0));
        self
    }

    /// Sets vertical padding.
    #[must_use]
    pub fn padding_y(mut self, padding: Pixels) -> Self {
        self.padding_y = padding.max(px(0.0));
        self
    }

    /// Sets the resting control fill.
    #[must_use]
    pub fn background_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.background_color = color.into();
        self
    }

    /// Sets the editable text color.
    #[must_use]
    pub fn text_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.text_color = color.into();
        self
    }

    /// Sets the empty-value placeholder color.
    #[must_use]
    pub fn placeholder_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.placeholder_color = color.into();
        self
    }

    /// Sets the resting border color.
    #[must_use]
    pub fn border_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.border_color = color.into();
        self
    }

    /// Sets the border color while this exact input owns focus.
    #[must_use]
    pub fn focus_border_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.focus_border_color = color.into();
        self
    }

    /// Sets the selection background color.
    #[must_use]
    pub fn selection_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.selection_color = color.into();
        self
    }

    /// Sets the collapsed selection caret color.
    #[must_use]
    pub fn caret_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.caret_color = color.into();
        self
    }

    /// Sets the uniform border width.
    #[must_use]
    pub fn border_width(mut self, width: Pixels) -> Self {
        self.border_width = width.max(px(0.0));
        self
    }

    /// Sets the uniform corner radius.
    #[must_use]
    pub fn corner_radius(mut self, radius: Pixels) -> Self {
        self.corner_radius = radius.max(px(0.0));
        self
    }

    /// Sets the complete native font description.
    #[must_use]
    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    /// Replaces only the native font family.
    #[must_use]
    pub fn font_family(mut self, family: impl Into<gpui::SharedString>) -> Self {
        self.font.family = family.into();
        self
    }

    /// Sets the line shaping size in logical pixels.
    #[must_use]
    pub fn font_size(mut self, size: Pixels) -> Self {
        self.font_size = size.max(px(1.0));
        self
    }

    /// Sets the opacity multiplier used while disabled.
    ///
    /// Finite values are clamped to `0.0..=1.0`. A non-finite value leaves the
    /// previous setting unchanged.
    #[must_use]
    pub fn disabled_opacity(mut self, opacity: f32) -> Self {
        if opacity.is_finite() {
            self.disabled_opacity = opacity.clamp(0.0, 1.0);
        }
        self
    }
}

impl Default for TextInputStyle {
    fn default() -> Self {
        Self {
            width: None,
            height: px(30.0),
            padding_x: px(8.0),
            padding_y: px(0.0),
            background_color: rgba(0x2929_2dff).into(),
            text_color: rgba(0xe2e2_e5ff).into(),
            placeholder_color: hsla(0.0, 0.0, 0.52, 1.0),
            border_color: rgba(0x3a3a_3fff).into(),
            focus_border_color: rgba(0x0a84_ffff).into(),
            selection_color: rgba(0x0a84_ff50).into(),
            caret_color: rgba(0x0a84_ffff).into(),
            border_width: px(1.0),
            corner_radius: px(6.0),
            font: font(".SystemUIFont"),
            font_size: px(13.0),
            disabled_opacity: 0.45,
        }
    }
}

/// Initial value, appearance, and editing policy for a [`TextInput`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextInputConfig {
    /// Empty-state copy.
    placeholder: String,
    /// Initial controlled value.
    value: String,
    /// Typed visual treatment.
    style: TextInputStyle,
    /// Whether the control rejects focus and all interaction.
    disabled: bool,
    /// Whether the control permits selection/copy but rejects user edits.
    read_only: bool,
    /// Maximum Unicode grapheme clusters accepted from user edits, if bounded.
    max_length: Option<usize>,
}

impl TextInputConfig {
    /// Creates an enabled, editable, empty input with default styling.
    #[must_use]
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            ..Self::default()
        }
    }

    /// Sets the initial controlled value.
    #[must_use]
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Sets the typed visual treatment.
    #[must_use]
    pub fn style(mut self, style: TextInputStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets whether the input rejects focus and all user interaction.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Sets whether selection and copy remain available while edits are rejected.
    #[must_use]
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Limits user-committed content by Unicode grapheme clusters.
    ///
    /// Active marked composition may temporarily exceed the limit so phonetic
    /// input is not cut off before the platform commits it.
    #[must_use]
    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// Removes a previously configured content limit.
    #[must_use]
    pub fn unlimited(mut self) -> Self {
        self.max_length = None;
        self
    }
}

/// Owned two-way synchronization between a parent model and a [`TextInput`].
///
/// One subscription maps user [`TextInputEvent::Change`] values into the
/// parent; the other observes parent notifications and silently reconciles the
/// input. Equal text is ignored by [`TextInput::set_text`], which is essential:
/// the echo of an intermediate IME value does not clear its marked range.
/// Dropping the binding cancels both directions.
#[derive(Debug)]
pub struct TextModelBinding {
    /// Input-event subscription retained for the binding lifetime.
    input_to_model: Subscription,
    /// Parent-notification observer retained for the binding lifetime.
    model_to_input: Subscription,
}

impl TextModelBinding {
    /// Binds an input to arbitrary text state owned by its parent entity.
    ///
    /// `initial_value` is installed before callbacks are attached. `read`
    /// supplies the current canonical parent value after every parent
    /// notification. `write` receives user and IME changes; it should mutate
    /// the parent model and notify `cx` when the canonical value changed.
    ///
    /// A [`crate::Local<String>`] can be bound with `|owner| owner.title.get()` and
    /// `|owner, value, cx| { owner.title.set(value, cx); }`.
    #[must_use]
    pub fn bind<Owner, Read, Write>(
        input: &TextInputHandle,
        initial_value: impl Into<String>,
        cx: &mut Context<'_, Owner>,
        mut read: Read,
        mut write: Write,
    ) -> Self
    where
        Owner: 'static,
        Read: FnMut(&Owner) -> String + 'static,
        Write: FnMut(&mut Owner, String, &mut Context<'_, Owner>) + 'static,
    {
        let initial_value = initial_value.into();
        input.update(cx, |input, input_cx| {
            input.set_text(initial_value, input_cx);
        });

        let input_to_model = cx.subscribe(input, move |owner, _input, event, owner_cx| {
            if let TextInputEvent::Change(value) = event {
                write(owner, value.clone(), owner_cx);
            }
        });
        let observed_input = input.clone();
        let model_to_input = cx.observe_self(move |owner, owner_cx| {
            let value = read(owner);
            observed_input.update(owner_cx, |input, input_cx| {
                input.set_text(value, input_cx);
            });
        });

        Self {
            input_to_model,
            model_to_input,
        }
    }

    /// Binds an input to a shared [`Ref<String>`] read by the parent.
    ///
    /// Every external mutation of the ref must use the parent context as its
    /// notifier so the parent-to-input observer runs.
    #[must_use]
    pub fn bind_ref<Owner: 'static>(
        input: &TextInputHandle,
        model: Ref<String>,
        cx: &mut Context<'_, Owner>,
    ) -> Self {
        let initial_value = model.get();
        let read_model = model.clone();
        Self::bind(
            input,
            initial_value,
            cx,
            move |_owner| read_model.get(),
            move |_owner, value, owner_cx| {
                model.set(value, owner_cx);
            },
        )
    }

    /// Detaches both directions so they continue until their entities release.
    ///
    /// Prefer storing and dropping the binding with its parent. Detachment is
    /// intended only when the callbacks deliberately outlive that owner slot.
    pub fn detach(self) {
        self.input_to_model.detach();
        self.model_to_input.detach();
    }
}

/// Typed events emitted by [`TextInput`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextInputEvent {
    /// The visible value changed, including an intermediate IME composition.
    Change(String),
    /// Return or Enter was pressed after platform composition handling.
    Submit(String),
    /// Escape was pressed and the input released focus.
    Escape,
}

/// Effective editing mode, including the policy restored after re-enabling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextInputMode {
    /// Focus, selection, and editing are enabled.
    Editable,
    /// Focus, selection, and copy are enabled while edits are rejected.
    ReadOnly,
    /// Interaction is disabled and re-enabling restores editable mode.
    DisabledEditable,
    /// Interaction is disabled and re-enabling restores read-only mode.
    DisabledReadOnly,
}

impl TextInputMode {
    /// Resolves two independent configuration flags with disabled precedence.
    const fn from_flags(disabled: bool, read_only: bool) -> Self {
        match (disabled, read_only) {
            (false, false) => Self::Editable,
            (false, true) => Self::ReadOnly,
            (true, false) => Self::DisabledEditable,
            (true, true) => Self::DisabledReadOnly,
        }
    }

    /// Returns whether all user interaction is disabled.
    const fn is_disabled(self) -> bool {
        matches!(self, Self::DisabledEditable | Self::DisabledReadOnly)
    }

    /// Returns whether the current or restored enabled policy is read-only.
    const fn is_read_only(self) -> bool {
        matches!(self, Self::ReadOnly | Self::DisabledReadOnly)
    }

    /// Changes disabled state without losing the enabled editing policy.
    const fn with_disabled(self, disabled: bool) -> Self {
        Self::from_flags(disabled, self.is_read_only())
    }

    /// Changes the current or restored read-only policy.
    const fn with_read_only(self, read_only: bool) -> Self {
        Self::from_flags(self.is_disabled(), read_only)
    }
}

/// A native, single-line, IME-aware text input.
///
/// The control includes its own background, border, padding, placeholder,
/// selection, caret, and marked-text underline. It can be inserted directly as
/// a Rust expression in [`crate::view!`]. Use [`TextInput::set_text`] for
/// controlled updates and subscribe to [`TextInputEvent`] for user edits.
pub struct TextInput {
    /// Native focus identity used both by the view shell and platform input.
    focus_handle: FocusHandle,
    /// Editable text, selection, and composition state.
    buffer: TextInputBuffer,
    /// Empty-state copy rendered without becoming part of the editable value.
    placeholder: String,
    /// Current typed visual treatment.
    style: TextInputStyle,
    /// Effective editing policy and its state after re-enabling.
    mode: TextInputMode,
    /// Maximum Unicode grapheme clusters accepted from user edits, if bounded.
    max_length: Option<usize>,
    /// Last shaped non-wrapping line used for pointer and IME geometry.
    last_layout: Option<ShapedLine>,
    /// Last window-local logical text origin, including horizontal caret scrolling.
    last_text_origin: Option<Point<Pixels>>,
    /// Last content bounds supplied to the low-level text element.
    last_bounds: Option<Bounds<Pixels>>,
    /// Horizontal amount hidden to keep the active caret visible.
    scroll_x: Pixels,
    /// Whether a primary-button pointer selection is in progress.
    is_selecting: bool,
    /// Focus lifecycle hook that clears platform composition after blur.
    focus_out_subscription: Option<Subscription>,
    /// Whether the next painted geometry must refresh the platform IME panel.
    ime_geometry_dirty: bool,
}

impl TextInput {
    /// Creates an empty input with the supplied placeholder.
    #[must_use]
    pub fn new(placeholder: impl Into<String>, cx: &mut Context<'_, Self>) -> Self {
        Self::with_config(TextInputConfig::new(placeholder), cx)
    }

    /// Creates an input from a typed initial configuration.
    #[must_use]
    pub fn with_config(config: TextInputConfig, cx: &mut Context<'_, Self>) -> Self {
        let TextInputConfig {
            placeholder,
            value,
            style,
            disabled,
            read_only,
            max_length,
        } = config;
        let value = normalize_single_line(&value).into_owned();
        let mut buffer = TextInputBuffer::default();
        buffer.set_text(value);
        let focus_handle = cx.focus_handle().tab_index(0).tab_stop(!disabled);
        let mode = TextInputMode::from_flags(disabled, read_only);

        Self {
            focus_handle,
            buffer,
            placeholder,
            style,
            mode,
            max_length,
            last_layout: None,
            last_text_origin: None,
            last_bounds: None,
            scroll_x: px(0.0),
            is_selecting: false,
            focus_out_subscription: None,
            ime_geometry_dirty: true,
        }
    }

    /// Returns the current value, including any active marked composition.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.buffer.text
    }

    /// Returns the current typed visual treatment.
    #[must_use]
    pub const fn style(&self) -> &TextInputStyle {
        &self.style
    }

    /// Replaces the complete typed visual treatment.
    ///
    /// An equal style is ignored. Layout and IME geometry are recomputed for
    /// any effective change because font and padding are part of the style.
    pub fn set_style(&mut self, style: TextInputStyle, cx: &mut Context<'_, Self>) {
        if self.style == style {
            return;
        }
        self.style = style;
        self.reset_layout_cache();
        cx.notify();
    }

    /// Returns whether focus and every user interaction are disabled.
    #[must_use]
    pub const fn is_disabled(&self) -> bool {
        self.mode.is_disabled()
    }

    /// Enables or disables focus and every user interaction.
    ///
    /// Programmatic controlled updates remain available while disabled. If a
    /// focused input becomes disabled, its next render releases focus.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<'_, Self>) {
        if self.is_disabled() == disabled {
            return;
        }
        self.mode = self.mode.with_disabled(disabled);
        self.focus_handle = self.focus_handle.clone().tab_index(0).tab_stop(!disabled);
        if disabled {
            self.is_selecting = false;
            let (_unmarked, truncated) = self.unmark_and_enforce_limit();
            if truncated {
                cx.emit(TextInputEvent::Change(self.buffer.text.clone()));
            }
        }
        cx.notify();
    }

    /// Returns whether selection and copy are allowed but user edits are not.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.mode.is_read_only()
    }

    /// Enables or disables read-only editing policy.
    ///
    /// Enter/Return still emits [`TextInputEvent::Submit`], and pointer or
    /// keyboard selection plus copy remain available.
    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<'_, Self>) {
        if self.is_read_only() == read_only {
            return;
        }
        self.mode = self.mode.with_read_only(read_only);
        if read_only {
            let (_unmarked, truncated) = self.unmark_and_enforce_limit();
            if truncated {
                cx.emit(TextInputEvent::Change(self.buffer.text.clone()));
            }
        }
        cx.notify();
    }

    /// Returns the maximum grapheme count accepted from user edits, if bounded.
    #[must_use]
    pub const fn max_length(&self) -> Option<usize> {
        self.max_length
    }

    /// Sets or removes the Unicode grapheme limit for later user edits.
    ///
    /// This does not rewrite an existing controlled value. A marked
    /// composition can temporarily exceed the limit and is constrained when
    /// the platform commits or unmarks it.
    pub fn set_max_length(&mut self, max_length: Option<usize>, cx: &mut Context<'_, Self>) {
        if self.max_length == max_length {
            return;
        }
        self.max_length = max_length;
        cx.notify();
    }

    /// Replaces the value without emitting a user-originated change event.
    ///
    /// An equal value is ignored so a controlled parent render does not reset
    /// the user's caret or active composition.
    pub fn set_text(&mut self, value: impl Into<String>, cx: &mut Context<'_, Self>) {
        let value = value.into();
        let Some(value) = controlled_replacement(&self.buffer.text, &value) else {
            return;
        };
        self.buffer.set_text(value);
        self.reset_layout_cache();
        cx.notify();
    }

    /// Clears the value without emitting a user-originated change event.
    pub fn clear(&mut self, cx: &mut Context<'_, Self>) {
        self.set_text(String::new(), cx);
    }

    /// Updates the placeholder without disturbing value, selection, or IME state.
    ///
    /// An equal placeholder is ignored, making this safe to call from every
    /// render after a runtime language change.
    pub fn set_placeholder(&mut self, placeholder: impl Into<String>, cx: &mut Context<'_, Self>) {
        let placeholder = placeholder.into();
        if self.placeholder == placeholder {
            return;
        }
        self.placeholder = placeholder;
        if self.buffer.text.is_empty() {
            self.reset_layout_cache();
            cx.notify();
        }
    }

    /// Returns this input's native focus handle.
    #[must_use]
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Moves keyboard and platform text input focus to this control.
    pub fn focus(&self, window: &mut Window) {
        if !self.is_disabled() {
            self.focus_handle.focus(window);
        }
    }

    /// Releases keyboard and platform text input focus from this control.
    pub fn blur(&self, window: &mut Window) {
        if self.focus_handle.is_focused(window) {
            window.blur();
        }
    }

    /// Returns whether the platform currently owns a marked composition range.
    #[must_use]
    pub fn is_composing(&self) -> bool {
        self.buffer.marked_range.is_some()
    }

    /// Returns the ordered selection as UTF-8 byte offsets.
    #[must_use]
    pub fn selected_range(&self) -> Range<usize> {
        self.buffer.selected_range.clone()
    }

    /// Invalidates geometry retained exclusively for platform input queries.
    fn reset_layout_cache(&mut self) {
        self.last_layout = None;
        self.last_text_origin = None;
        self.last_bounds = None;
        self.scroll_x = px(0.0);
        self.ime_geometry_dirty = true;
    }

    /// Ends active marked text and applies the user-edit limit to that commit.
    ///
    /// The returned pair reports whether a mark existed and whether enforcing
    /// the limit changed the visible value.
    fn unmark_and_enforce_limit(&mut self) -> (bool, bool) {
        let unmarked = self.buffer.unmark();
        let truncated = unmarked && self.buffer.truncate_to_graphemes(self.max_length);
        if unmarked {
            self.ime_geometry_dirty = true;
        }
        if truncated {
            self.reset_layout_cache();
        }
        (unmarked, truncated)
    }

    /// Finishes one user edit and emits the public value event when necessary.
    fn finish_edit(
        &mut self,
        value_changed: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.ime_geometry_dirty = true;
        if value_changed {
            cx.emit(TextInputEvent::Change(self.buffer.text.clone()));
        }
        window.invalidate_character_coordinates();
        cx.notify();
    }

    /// Moves the caret and collapses the current selection.
    fn move_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.buffer.move_to(offset);
        self.ime_geometry_dirty = true;
        window.invalidate_character_coordinates();
        cx.notify();
    }

    /// Extends the selection head to one UTF-8 byte boundary.
    fn select_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.buffer.select_to(offset);
        self.ime_geometry_dirty = true;
        window.invalidate_character_coordinates();
        cx.notify();
    }

    /// Handles editing and navigation keys while leaving printable text to IME.
    fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.is_disabled() {
            return;
        }
        let key = event.keystroke.key.to_lowercase();
        let modifiers = event.keystroke.modifiers;
        // AltGr is commonly reported as Ctrl+Alt on Windows; it must remain
        // printable platform input instead of becoming a clipboard shortcut.
        let handled = if modifiers.secondary() && !modifiers.alt {
            self.handle_secondary_key(&key, window, cx)
        } else {
            self.handle_editing_key(&key, modifiers.shift, window, cx)
        };

        if handled {
            cx.stop_propagation();
        }
    }

    /// Handles semantic clipboard and whole-field shortcuts.
    fn handle_secondary_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        match key {
            "a" => {
                self.buffer.select_all();
                window.invalidate_character_coordinates();
                cx.notify();
            }
            "c" => self.copy_selection(cx),
            "v" if !self.is_read_only() => {
                if let Some(text) = ui::read_clipboard_text(cx) {
                    let changed =
                        self.buffer
                            .replace_committed_limited(None, &text, self.max_length);
                    self.finish_edit(changed, window, cx);
                }
            }
            "x" if !self.is_read_only() => {
                self.copy_selection(cx);
                if !self.buffer.selected_range.is_empty() {
                    let changed = self
                        .buffer
                        .replace_committed_limited(None, "", self.max_length);
                    self.finish_edit(changed, window, cx);
                }
            }
            "v" | "x" => {}
            _ => return false,
        }
        true
    }

    /// Handles non-printable single-line editing and lifecycle keys.
    fn handle_editing_key(
        &mut self,
        key: &str,
        extend_selection: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        match key {
            "backspace" | "delete" => {
                if !self.is_read_only() {
                    self.delete_from_key(key, window, cx);
                }
            }
            "left" | "arrowleft" => self.move_horizontal(false, extend_selection, window, cx),
            "right" | "arrowright" => self.move_horizontal(true, extend_selection, window, cx),
            "home" => self.move_edge(false, extend_selection, window, cx),
            "end" => self.move_edge(true, extend_selection, window, cx),
            "enter" | "return" => cx.emit(TextInputEvent::Submit(self.buffer.text.clone())),
            "escape" => {
                let (_unmarked, truncated) = self.unmark_and_enforce_limit();
                self.blur(window);
                if truncated {
                    cx.emit(TextInputEvent::Change(self.buffer.text.clone()));
                }
                cx.emit(TextInputEvent::Escape);
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    /// Deletes the current selection or one adjacent grapheme cluster.
    fn delete_from_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.buffer.selected_range.is_empty() {
            let cursor = self.buffer.cursor_offset();
            let boundary = if key == "backspace" {
                self.buffer.previous_boundary(cursor)
            } else {
                self.buffer.next_boundary(cursor)
            };
            self.buffer.select_to(boundary);
        }
        let changed = self
            .buffer
            .replace_committed_limited(None, "", self.max_length);
        self.finish_edit(changed, window, cx);
    }

    /// Moves or extends the selection by one grapheme cluster.
    fn move_horizontal(
        &mut self,
        right: bool,
        extend_selection: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let cursor = self.buffer.cursor_offset();
        let offset = if extend_selection || self.buffer.selected_range.is_empty() {
            if right {
                self.buffer.next_boundary(cursor)
            } else {
                self.buffer.previous_boundary(cursor)
            }
        } else if right {
            self.buffer.selected_range.end
        } else {
            self.buffer.selected_range.start
        };
        if extend_selection {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx);
        }
    }

    /// Moves or extends the selection to the beginning or end.
    fn move_edge(
        &mut self,
        end: bool,
        extend_selection: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let offset = if end { self.buffer.text.len() } else { 0 };
        if extend_selection {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx);
        }
    }

    /// Writes the selected substring to the native clipboard when non-empty.
    fn copy_selection(&self, cx: &Context<'_, Self>) {
        if !self.buffer.selected_range.is_empty() {
            ui::write_clipboard_text(
                cx,
                self.buffer.text[self.buffer.selected_range.clone()].to_owned(),
            );
        }
    }

    /// Focuses the field and starts or extends a pointer selection.
    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.is_disabled() {
            return;
        }
        self.focus(window);
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx);
        }
        cx.stop_propagation();
    }

    /// Extends an active pointer selection over the latest shaped line.
    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.is_disabled() && self.is_selecting {
            let offset = self.index_for_mouse_position(event.position);
            self.select_to(offset, window, cx);
            cx.stop_propagation();
        }
    }

    /// Ends an active pointer selection both inside and outside the field.
    fn on_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.is_selecting {
            self.is_selecting = false;
            cx.stop_propagation();
        }
    }

    /// Maps a window-local logical point to the closest editable byte boundary.
    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.buffer.text.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(origin), Some(line)) = (
            self.last_bounds,
            self.last_text_origin,
            self.last_layout.as_ref(),
        ) else {
            return self.buffer.cursor_offset();
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.buffer.text.len();
        }
        line.closest_index_for_x(position.x - origin.x)
            .min(self.buffer.text.len())
    }
}

impl EventEmitter<TextInputEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        if self.focus_out_subscription.is_none() {
            self.focus_out_subscription =
                Some(cx.on_blur(&self.focus_handle, window, |this, window, cx| {
                    let (unmarked, truncated) = this.unmark_and_enforce_limit();
                    if unmarked || truncated {
                        window.invalidate_character_coordinates();
                        if truncated {
                            cx.emit(TextInputEvent::Change(this.buffer.text.clone()));
                        }
                        cx.notify();
                    }
                }));
        }
        if self.is_disabled() && self.focus_handle.is_focused(window) {
            window.blur();
        }
        let focus_handle = self.focus_handle.clone();
        let input_id = ("gpui-vue-text-input", cx.entity_id());
        let text_element = TextInputElement { input: cx.entity() };
        let key_down = cx.listener(Self::on_key_down);
        let mouse_down = cx.listener(Self::on_mouse_down);
        let mouse_move = cx.listener(Self::on_mouse_move);
        let mouse_up = cx.listener(Self::on_mouse_up);
        let mouse_up_out = cx.listener(Self::on_mouse_up);
        let style = self.style.clone();
        let focus_border_color = style.focus_border_color;
        let mut shell = gpui::div()
            .id(input_id)
            .track_focus(&focus_handle)
            .key_context("GpuiVueTextInput")
            .on_key_down(key_down)
            .on_mouse_down(MouseButton::Left, mouse_down)
            .on_mouse_move(mouse_move)
            .on_mouse_up(MouseButton::Left, mouse_up)
            .on_mouse_up_out(MouseButton::Left, mouse_up_out)
            .h(style.height)
            .px(style.padding_x)
            .py(style.padding_y)
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .rounded(style.corner_radius)
            .border(style.border_width)
            .border_color(style.border_color)
            .bg(style.background_color)
            .font(style.font.clone())
            .text_size(style.font_size)
            .text_color(style.text_color)
            .focus(move |focus_style| focus_style.border_color(focus_border_color));
        shell = if let Some(width) = style.width {
            shell.w(width)
        } else {
            shell.w(relative(1.0))
        };
        shell = if self.is_disabled() {
            shell.cursor_not_allowed().opacity(style.disabled_opacity)
        } else {
            shell.tab_index(0).cursor_text()
        };
        shell.child(text_element)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<String> {
        let range = utf16_range_to_utf8(&self.buffer.text, range_utf16);
        adjusted_range.replace(utf8_range_to_utf16(&self.buffer.text, range.clone()));
        self.buffer.text.get(range).map(ToOwned::to_owned)
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<UTF16Selection> {
        if !ignore_disabled_input && (self.is_disabled() || self.is_read_only()) {
            return None;
        }
        Some(UTF16Selection {
            range: utf8_range_to_utf16(&self.buffer.text, self.buffer.selected_range.clone()),
            reversed: self.buffer.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Range<usize>> {
        self.buffer
            .marked_range
            .clone()
            .map(|range| utf8_range_to_utf16(&self.buffer.text, range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let (unmarked, truncated) = self.unmark_and_enforce_limit();
        if unmarked || truncated {
            window.invalidate_character_coordinates();
            if truncated {
                cx.emit(TextInputEvent::Change(self.buffer.text.clone()));
            }
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.is_disabled() || self.is_read_only() {
            return;
        }
        let range = range_utf16.map(|range| utf16_range_to_utf8(&self.buffer.text, range));
        let changed = self
            .buffer
            .replace_committed_limited(range, text, self.max_length);
        self.finish_edit(changed, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.is_disabled() || self.is_read_only() {
            return;
        }
        let range = range_utf16.map(|range| self.buffer.marked_replacement_range_utf16(range));
        let changed = self
            .buffer
            .replace_marked(range, new_text, new_selected_range);
        self.finish_edit(changed, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let origin = self.last_text_origin?;
        let range = utf16_range_to_utf8(&self.buffer.text, range_utf16);
        Some(Bounds::from_corners(
            point(
                origin.x + line.x_for_index(range.start),
                element_bounds.top(),
            ),
            point(
                origin.x + line.x_for_index(range.end),
                element_bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<usize> {
        let utf8 = self.index_for_mouse_position(point);
        Some(utf8_offset_to_utf16(&self.buffer.text, utf8))
    }
}

/// Editable text and byte-oriented selection state independent of a window.
#[derive(Debug, Default)]
struct TextInputBuffer {
    /// Current single-line UTF-8 value.
    text: String,
    /// Ordered UTF-8 byte range, including a collapsed caret.
    selected_range: Range<usize>,
    /// Whether the logical selection head is at `selected_range.start`.
    selection_reversed: bool,
    /// Active platform composition as a UTF-8 byte range.
    marked_range: Option<Range<usize>>,
}

impl TextInputBuffer {
    /// Replaces all state with one externally controlled value.
    fn set_text(&mut self, text: String) {
        let end = text.len();
        self.text = text;
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    /// Returns the logical caret or selection head.
    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    /// Collapses the selection at a valid byte boundary.
    fn move_to(&mut self, offset: usize) {
        let offset = clamp_utf8_offset(&self.text, offset);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    /// Extends the selection head and preserves an ordered stored range.
    fn select_to(&mut self, offset: usize) {
        let offset = clamp_utf8_offset(&self.text, offset);
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.marked_range = None;
    }

    /// Selects the complete value with the head at the end.
    fn select_all(&mut self) {
        self.selected_range = 0..self.text.len();
        self.selection_reversed = false;
        self.marked_range = None;
    }

    /// Returns the preceding extended grapheme boundary.
    fn previous_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    /// Returns the following extended grapheme boundary.
    fn next_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.text.len())
    }

    /// Chooses the explicit, marked, or selected replacement range.
    fn replacement_range(&self, explicit: Option<Range<usize>>) -> Range<usize> {
        let range = explicit
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        clamp_utf8_range(&self.text, range)
    }

    /// Resolves a marked-text replacement range to document UTF-8 bytes.
    ///
    /// During an active composition, platform ranges are relative to the old
    /// marked text. Without a mark they are absolute document UTF-16 ranges.
    fn marked_replacement_range_utf16(&self, range_utf16: Range<usize>) -> Range<usize> {
        let Some(marked) = self.marked_range.clone() else {
            return utf16_range_to_utf8(&self.text, range_utf16);
        };
        let marked = clamp_utf8_range(&self.text, marked);
        let relative = utf16_range_to_utf8(&self.text[marked.clone()], range_utf16);
        marked.start.saturating_add(relative.start)..marked.start.saturating_add(relative.end)
    }

    /// Commits text while preserving surrounding content under a grapheme cap.
    fn replace_committed_limited(
        &mut self,
        explicit: Option<Range<usize>>,
        text: &str,
        max_length: Option<usize>,
    ) -> bool {
        let replacement = self.replacement_range(explicit);
        let text = normalize_single_line(text);
        let text = limit_replacement_graphemes(&self.text, replacement.clone(), &text, max_length);
        self.replace_normalized(replacement, &text)
    }

    /// Replaces one validated range with already normalized text.
    fn replace_normalized(&mut self, replacement: Range<usize>, text: &str) -> bool {
        let changed = self.text[replacement.clone()] != *text;
        let caret = replacement.start + text.len();
        self.text.replace_range(replacement, text);
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        changed
    }

    /// Replaces and marks platform composition text with a relative selection.
    fn replace_marked(
        &mut self,
        explicit: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> bool {
        let replacement = self.replacement_range(explicit);
        let text = normalize_single_line(text);
        let changed = self.text[replacement.clone()] != *text;
        let insertion_start = replacement.start;
        let relative_selection = selected_utf16.map_or_else(
            || text.len()..text.len(),
            |range| utf16_range_to_utf8(&text, range),
        );
        self.text.replace_range(replacement, &text);
        self.marked_range = (!text.is_empty())
            .then_some(insertion_start..insertion_start.saturating_add(text.len()));
        self.selected_range = insertion_start.saturating_add(relative_selection.start)
            ..insertion_start.saturating_add(relative_selection.end);
        self.selection_reversed = false;
        changed
    }

    /// Clears marked composition and reports whether state changed.
    fn unmark(&mut self) -> bool {
        self.marked_range.take().is_some()
    }

    /// Truncates content at an extended grapheme boundary when bounded.
    fn truncate_to_graphemes(&mut self, max_length: Option<usize>) -> bool {
        let truncated = truncate_graphemes(&self.text, max_length);
        let Cow::Owned(truncated) = truncated else {
            return false;
        };
        self.set_text(truncated);
        true
    }
}

/// A custom element that shapes text and registers the native input handler.
struct TextInputElement {
    /// Retained input entity receiving platform callbacks.
    input: TextInputHandle,
}

/// Geometry calculated once between the custom element's prepaint and paint.
struct TextInputPrepaint {
    /// Shaped display line, which may contain the placeholder.
    line: Option<ShapedLine>,
    /// Selection background painted beneath glyphs.
    selection: Option<PaintQuad>,
    /// Focused collapsed caret painted above glyphs.
    caret: Option<PaintQuad>,
    /// Text origin after horizontal caret scrolling.
    origin: Point<Pixels>,
    /// Horizontal caret-scroll value retained for the next frame.
    scroll_x: Pixels,
}

impl IntoElement for TextInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextInputElement {
    type RequestLayoutState = ();
    type PrepaintState = TextInputPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let has_text = !input.buffer.text.is_empty();
        let display_text = if has_text {
            input.buffer.text.clone()
        } else {
            input.placeholder.clone()
        };
        let text_style = window.text_style();
        let base_run = TextRun {
            len: display_text.len(),
            font: text_style.font(),
            color: if has_text {
                input.style.text_color
            } else {
                input.style.placeholder_color
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if has_text {
            marked_runs(base_run, input.buffer.marked_range.as_ref())
        } else {
            vec![base_run]
        };
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text.into(), font_size, &runs, None);
        let caret_x = if has_text {
            line.x_for_index(input.buffer.cursor_offset())
        } else {
            px(0.0)
        };
        let mut scroll_x = input.scroll_x;
        let visible_width = bounds.size.width.max(px(1.0));
        if caret_x - scroll_x > visible_width - px(2.0) {
            scroll_x = caret_x - visible_width + px(2.0);
        } else if caret_x < scroll_x {
            scroll_x = caret_x;
        }
        if !has_text {
            scroll_x = px(0.0);
        }
        let origin = point(bounds.left() - scroll_x, bounds.top());
        let selection = (has_text && !input.buffer.selected_range.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(
                        origin.x + line.x_for_index(input.buffer.selected_range.start),
                        bounds.top(),
                    ),
                    point(
                        origin.x + line.x_for_index(input.buffer.selected_range.end),
                        bounds.bottom(),
                    ),
                ),
                input.style.selection_color,
            )
        });
        let caret = input.buffer.selected_range.is_empty().then(|| {
            fill(
                Bounds::new(
                    point(origin.x + caret_x, bounds.top() + px(4.0)),
                    size(px(1.5), (bounds.size.height - px(8.0)).max(px(1.0))),
                ),
                input.style.caret_color,
            )
        });
        TextInputPrepaint {
            line: Some(line),
            selection,
            caret,
            origin,
            scroll_x,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let input = self.input.read(cx);
        let focus_handle = input.focus_handle.clone();
        let disabled = input.is_disabled();
        if !disabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        if let Some(line) = prepaint.line.take() {
            let _ = line.paint(prepaint.origin, bounds.size.height, window, cx);
            let refresh_ime_geometry = self.input.update(cx, |input, _cx| {
                input.last_layout = Some(line);
                input.last_text_origin = Some(prepaint.origin);
                input.last_bounds = Some(bounds);
                input.scroll_x = prepaint.scroll_x;
                std::mem::take(&mut input.ime_geometry_dirty)
            });
            if refresh_ime_geometry && focus_handle.is_focused(window) {
                window.invalidate_character_coordinates();
                window.request_animation_frame();
            }
        }
        if !disabled
            && focus_handle.is_focused(window)
            && let Some(caret) = prepaint.caret.take()
        {
            window.paint_quad(caret);
        }
    }
}

/// Builds shaped-text runs with an underline over active marked composition.
fn marked_runs(base: TextRun, marked: Option<&Range<usize>>) -> Vec<TextRun> {
    let Some(marked) = marked else {
        return vec![base];
    };
    let lengths = [
        (marked.start, false),
        (marked.end.saturating_sub(marked.start), true),
        (base.len.saturating_sub(marked.end), false),
    ];
    lengths
        .into_iter()
        .filter(|(len, _underline)| *len > 0)
        .map(|(len, underline)| TextRun {
            len,
            underline: underline.then(|| UnderlineStyle {
                color: Some(base.color),
                thickness: px(1.0),
                wavy: false,
            }),
            ..base.clone()
        })
        .collect()
}

/// Converts one UTF-8 byte offset into a platform UTF-16 code-unit offset.
fn utf8_offset_to_utf16(text: &str, offset: usize) -> usize {
    let offset = clamp_utf8_offset(text, offset);
    text[..offset].encode_utf16().count()
}

/// Converts a UTF-16 offset to the containing scalar's starting byte boundary.
fn utf16_offset_to_utf8_floor(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    for (utf8, character) in text.char_indices() {
        if utf16 >= offset {
            return utf8;
        }
        if utf16 + character.len_utf16() > offset {
            return utf8;
        }
        utf16 += character.len_utf16();
    }
    text.len()
}

/// Converts a UTF-16 offset to the containing scalar's ending byte boundary.
fn utf16_offset_to_utf8_ceil(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    for (utf8, character) in text.char_indices() {
        if utf16 >= offset {
            return utf8;
        }
        utf16 += character.len_utf16();
        if utf16 >= offset {
            return utf8 + character.len_utf8();
        }
    }
    text.len()
}

/// Converts an ordered UTF-8 byte range into platform UTF-16 code units.
fn utf8_range_to_utf16(text: &str, range: Range<usize>) -> Range<usize> {
    let range = clamp_utf8_range(text, range);
    utf8_offset_to_utf16(text, range.start)..utf8_offset_to_utf16(text, range.end)
}

/// Converts an ordered platform UTF-16 range into UTF-8 byte boundaries.
fn utf16_range_to_utf8(text: &str, range: Range<usize>) -> Range<usize> {
    if range.is_empty() {
        let offset = utf16_offset_to_utf8_floor(text, range.start);
        return offset..offset;
    }
    let start = utf16_offset_to_utf8_floor(text, range.start);
    let end = utf16_offset_to_utf8_ceil(text, range.end);
    start.min(end)..start.max(end)
}

/// Clamps an arbitrary byte offset down to a valid character boundary.
fn clamp_utf8_offset(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Clamps and orders an arbitrary UTF-8 byte range.
fn clamp_utf8_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = clamp_utf8_offset(text, range.start);
    let end = clamp_utf8_offset(text, range.end);
    start.min(end)..start.max(end)
}

/// Replaces line separators with one space without allocating ordinary input.
fn normalize_single_line(text: &str) -> Cow<'_, str> {
    if !text.contains(['\r', '\n']) {
        return Cow::Borrowed(text);
    }
    let mut normalized = String::with_capacity(text.len());
    let mut previous_was_carriage_return = false;
    for character in text.chars() {
        match character {
            '\r' => {
                normalized.push(' ');
                previous_was_carriage_return = true;
            }
            '\n' if previous_was_carriage_return => {
                previous_was_carriage_return = false;
            }
            '\n' => {
                normalized.push(' ');
                previous_was_carriage_return = false;
            }
            _ => {
                normalized.push(character);
                previous_was_carriage_return = false;
            }
        }
    }
    Cow::Owned(normalized)
}

/// Resolves one controlled update without disturbing an equal marked value.
fn controlled_replacement(current: &str, requested: &str) -> Option<String> {
    let requested = normalize_single_line(requested);
    // A model echoes marked IME text through `Change`; an equal echo must not
    // unmark it. Controlled values are trusted even when longer than the
    // user-edit limit, matching the parent-owned model contract.
    if current == requested {
        return None;
    }
    let requested = requested.into_owned();
    (current != requested).then_some(requested)
}

/// Truncates a complete value at an extended grapheme boundary when needed.
fn truncate_graphemes(text: &str, max_length: Option<usize>) -> Cow<'_, str> {
    let Some(max_length) = max_length else {
        return Cow::Borrowed(text);
    };
    let Some((end, _grapheme)) = text.grapheme_indices(true).nth(max_length) else {
        return Cow::Borrowed(text);
    };
    Cow::Owned(text[..end].to_owned())
}

/// Limits inserted text against the grapheme count of the complete candidate.
///
/// Counting the concatenated value is necessary because a combining mark can
/// join a grapheme on either side of the replacement boundary.
fn limit_replacement_graphemes<'a>(
    current: &str,
    replacement: Range<usize>,
    text: &'a str,
    max_length: Option<usize>,
) -> Cow<'a, str> {
    let Some(max_length) = max_length else {
        return Cow::Borrowed(text);
    };
    let candidate_count = |inserted: &str| {
        let mut candidate =
            String::with_capacity(current.len() - replacement.len() + inserted.len());
        candidate.push_str(&current[..replacement.start]);
        candidate.push_str(inserted);
        candidate.push_str(&current[replacement.end..]);
        candidate.graphemes(true).count()
    };
    if candidate_count(text) <= max_length {
        return Cow::Borrowed(text);
    }

    let mut allowed_end = 0;
    for end in text
        .grapheme_indices(true)
        .map(|(index, _grapheme)| index)
        .skip(1)
        .chain(std::iter::once(text.len()))
    {
        if candidate_count(&text[..end]) <= max_length {
            allowed_end = end;
        }
    }
    Cow::Owned(text[..allowed_end].to_owned())
}

#[cfg(test)]
mod tests {
    //! Pure buffer tests for Unicode offsets and platform composition semantics.

    use super::*;

    /// Builds a buffer whose caret starts at the end of `text`.
    fn buffer(text: &str) -> TextInputBuffer {
        let mut buffer = TextInputBuffer::default();
        buffer.set_text(text.to_owned());
        buffer
    }

    #[test]
    fn utf8_utf16_round_trips_cjk_emoji_and_combining_text() {
        let text = "A永😀e\u{301}";
        for utf8 in text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
        {
            let utf16 = utf8_offset_to_utf16(text, utf8);
            assert_eq!(utf16_offset_to_utf8_floor(text, utf16), utf8);
        }
        assert_eq!(utf8_range_to_utf16(text, 1..4), 1..2);
        assert_eq!(utf8_range_to_utf16(text, 4..8), 2..4);
    }

    #[test]
    fn utf16_ranges_expand_around_split_surrogate_pairs() {
        let text = "😀永";
        let expanded = utf16_range_to_utf8(text, 1..2);
        assert_eq!(expanded, 0..4);
        assert_eq!(utf8_range_to_utf16(text, expanded), 0..2);
        assert_eq!(utf16_range_to_utf8(text, 1..1), 0..0);
    }

    #[test]
    fn composition_replaces_prior_mark_without_duplicating_intermediate_text() {
        let mut buffer = buffer("");
        assert!(buffer.replace_marked(None, "y", Some(1..1)));
        assert_eq!(buffer.text, "y");
        assert_eq!(buffer.marked_range, Some(0..1));
        assert!(buffer.replace_marked(None, "yo", Some(2..2)));
        assert_eq!(buffer.text, "yo");
        assert_eq!(buffer.marked_range, Some(0..2));
        assert!(buffer.replace_committed_limited(None, "永", None));
        assert_eq!(buffer.text, "永");
        assert_eq!(buffer.selected_range, 3..3);
        assert_eq!(buffer.marked_range, None);
    }

    #[test]
    fn marked_relative_selection_is_based_at_insertion_start() {
        let mut buffer = buffer("AB");
        buffer.selected_range = 1..1;
        assert!(buffer.replace_marked(None, "😀永", Some(2..3)));
        assert_eq!(buffer.text, "A😀永B");
        assert_eq!(buffer.marked_range, Some(1..8));
        assert_eq!(buffer.selected_range, 5..8);
    }

    #[test]
    fn marked_replacement_range_is_relative_at_a_nonzero_document_offset() {
        let mut buffer = buffer("abXYZcd");
        buffer.marked_range = Some(2..5);
        buffer.selected_range = 5..5;
        let replacement = buffer.marked_replacement_range_utf16(1..2);
        assert_eq!(replacement, 3..4);
        assert!(buffer.replace_marked(Some(replacement), "1", None));
        assert_eq!(buffer.text, "abX1Zcd");
        assert_eq!(buffer.marked_range, Some(3..4));
    }

    #[test]
    fn grapheme_navigation_keeps_combining_sequences_atomic() {
        let buffer = buffer("ae\u{301}永");
        assert_eq!(buffer.previous_boundary(buffer.text.len()), 4);
        assert_eq!(buffer.previous_boundary(4), 1);
        assert_eq!(buffer.next_boundary(1), 4);
    }

    #[test]
    fn committed_replacement_uses_selection_and_normalizes_newlines() {
        let mut buffer = buffer("hello");
        buffer.selected_range = 1..4;
        assert!(buffer.replace_committed_limited(None, "永\r\n字", None));
        assert_eq!(buffer.text, "h永 字o");
        assert_eq!(buffer.selected_range, 8..8);
    }

    #[test]
    fn equal_commit_can_move_selection_without_reporting_value_change() {
        let mut buffer = buffer("永");
        buffer.selected_range = 0..3;
        assert!(!buffer.replace_committed_limited(None, "永", None));
        assert_eq!(buffer.selected_range, 3..3);
    }

    #[test]
    fn marked_runs_cover_every_utf8_byte_exactly_once() {
        let base = TextRun {
            len: 8,
            font: gpui::Font::default(),
            color: hsla(0.0, 0.0, 1.0, 1.0),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = marked_runs(base, Some(&(1..7)));
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), 8);
        assert!(runs[1].underline.is_some());
    }

    #[test]
    fn single_line_normalization_collapses_crlf_once() {
        assert_eq!(normalize_single_line("a\r\nb\nc\rd"), "a b c d");
        assert!(matches!(normalize_single_line("永"), Cow::Borrowed("永")));
    }

    #[test]
    fn grapheme_limit_keeps_emoji_families_and_combining_sequences_atomic() {
        let text = "A👨‍👩‍👧‍👦e\u{301}永";
        assert_eq!(truncate_graphemes(text, Some(3)), "A👨‍👩‍👧‍👦e\u{301}");
        assert!(matches!(
            truncate_graphemes(text, Some(4)),
            Cow::Borrowed(_)
        ));
        assert_eq!(truncate_graphemes(text, Some(0)), "");
    }

    #[test]
    fn limited_commit_preserves_surrounding_graphemes() {
        let mut buffer = buffer("A永");
        buffer.selected_range = 1..1;
        assert!(!buffer.replace_committed_limited(None, "😀", Some(2)));
        assert_eq!(buffer.text, "A永");

        buffer.selected_range = 1..4;
        assert!(buffer.replace_committed_limited(None, "😀", Some(2)));
        assert_eq!(buffer.text, "A😀");
    }

    #[test]
    fn replacement_limit_counts_clusters_across_join_boundaries() {
        let mut buffer = buffer("aX");
        buffer.selected_range = 1..2;
        assert!(buffer.replace_committed_limited(None, "\u{301}", Some(1)));
        assert_eq!(buffer.text, "a\u{301}");
        assert_eq!(buffer.text.graphemes(true).count(), 1);
    }

    #[test]
    fn marked_composition_may_exceed_limit_until_commit() {
        let mut buffer = buffer("");
        assert!(buffer.replace_marked(None, "yong", Some(4..4)));
        assert_eq!(buffer.text, "yong");
        assert_eq!(controlled_replacement("yong", "yong"), None);
        assert!(buffer.replace_committed_limited(None, "永", Some(1)));
        assert_eq!(buffer.text, "永");
        assert_eq!(buffer.marked_range, None);
    }

    #[test]
    fn controlled_updates_are_normalized_but_not_user_length_limited() {
        assert_eq!(controlled_replacement("", "yong"), Some("yong".to_owned()),);
        assert_eq!(
            controlled_replacement("", "永\r\n字"),
            Some("永 字".to_owned()),
        );
    }

    #[test]
    fn style_builder_clamps_dimensions_and_opacity() {
        let style = TextInputStyle::default()
            .height(px(-4.0))
            .padding(px(-2.0))
            .font_size(px(0.0))
            .disabled_opacity(2.0);
        assert_eq!(style.height, px(0.0));
        assert_eq!(style.padding_x, px(0.0));
        assert_eq!(style.padding_y, px(0.0));
        assert_eq!(style.font_size, px(1.0));
        assert!((style.disabled_opacity - 1.0).abs() < f32::EPSILON);
    }
}
