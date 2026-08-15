//! Curated native UI primitives for `gpui-vue` applications.
//!
//! The view macros remain the preferred authoring surface. These primitives
//! cover typed runtime values and the occasional builder seam needed by
//! reusable helpers without making applications import GPUI directly.

pub use gpui::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, DragMoveEvent, ElementId, Entity,
    ExternalPaths, FocusHandle, Font, FontWeight, Hsla, IntoElement, KeyDownEvent, KeyUpEvent,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent,
    Pixels, Render, Rgba, ScrollWheelEvent, SharedString, StyleRefinement, Subscription,
    TouchPhase, Window,
};

use gpui::AppContext as _;

/// A screen-space point measured in logical pixels.
pub type ScreenPoint = gpui::Point<gpui::Pixels>;

/// Builds an unstyled native container.
#[must_use]
pub fn div() -> gpui::Div {
    gpui::div()
}

/// Builds an image backed by GPUI's asset or HTTP image pipeline.
#[must_use]
pub fn image(source: impl Into<gpui::ImageSource>) -> gpui::Img {
    crate::media::image(source)
}

/// Applies GPUI's typed object-fit policy to an image intrinsic.
///
/// This is an implementation seam for `view!`; applications normally author
/// `:object-fit={ObjectFit::Cover}` instead.
#[doc(hidden)]
#[must_use]
pub fn image_object_fit<Image>(image: Image, object_fit: gpui::ObjectFit) -> Image
where
    Image: gpui::StyledImage,
{
    gpui::StyledImage::object_fit(image, object_fit)
}

/// Installs GPUI's lazy loading replacement on an image intrinsic.
#[doc(hidden)]
#[must_use]
pub fn image_loading<Image, Loading>(image: Image, loading: Loading) -> Image
where
    Image: gpui::StyledImage,
    Loading: Fn() -> AnyElement + 'static,
{
    gpui::StyledImage::with_loading(image, loading)
}

/// Installs GPUI's lazy error replacement on an image intrinsic.
#[doc(hidden)]
#[must_use]
pub fn image_fallback<Image, Fallback>(image: Image, fallback: Fallback) -> Image
where
    Image: gpui::StyledImage,
    Fallback: Fn() -> AnyElement + 'static,
{
    gpui::StyledImage::with_fallback(image, fallback)
}

/// Converts a logical-pixel scalar into the native pixel type.
#[must_use]
pub const fn px(value: f32) -> gpui::Pixels {
    gpui::px(value)
}

/// Converts a packed `0xRRGGBB` value into an opaque native color.
#[must_use]
pub fn rgb(value: u32) -> gpui::Rgba {
    gpui::rgb(value)
}

/// Converts a packed `0xRRGGBBAA` value into a native color.
#[must_use]
pub fn rgba(value: u32) -> gpui::Rgba {
    gpui::rgba(value)
}

/// Builds a native HSLA color from normalized channel values.
#[must_use]
pub fn hsla(hue: f32, saturation: f32, lightness: f32, alpha: f32) -> gpui::Hsla {
    gpui::hsla(hue, saturation, lightness, alpha)
}

/// Writes plain text to the platform clipboard.
///
/// This semantic helper keeps application code independent of GPUI's
/// multi-entry [`ClipboardItem`] representation.
pub fn write_clipboard_text(app: &App, text: impl Into<String>) {
    app.write_to_clipboard(text_clipboard_item(text.into()));
}

/// Reads all text entries currently available on the platform clipboard.
///
/// Returns `None` when the clipboard is empty or contains no textual entry.
#[must_use]
pub fn read_clipboard_text(app: &App) -> Option<String> {
    clipboard_item_text(app.read_from_clipboard())
}

/// Wraps one text payload in the native clipboard representation.
fn text_clipboard_item(text: String) -> ClipboardItem {
    ClipboardItem::new_string(text)
}

/// Extracts the concatenated text entries from one native clipboard read.
fn clipboard_item_text(item: Option<ClipboardItem>) -> Option<String> {
    let mut text = String::new();
    let mut found_text = false;
    for entry in item?.into_entries() {
        if let gpui::ClipboardEntry::String(entry) = entry {
            found_text = true;
            text.push_str(entry.text());
        }
    }
    found_text.then_some(text)
}

/// Applies one typed runtime style refinement to an existing native element.
///
/// This is the implementation seam used by the intrinsic `:style` binding.
/// The callback receives a fresh [`StyleRefinement`], so stateful element-only
/// operations cannot accidentally bypass `view!`'s identity validation.
#[doc(hidden)]
#[must_use]
pub fn apply_style_refinement<Element, Refiner>(mut element: Element, refiner: Refiner) -> Element
where
    Element: gpui::Styled,
    Refiner: FnOnce(StyleRefinement) -> StyleRefinement,
{
    use gpui::Refineable as _;

    let refinement = refiner(StyleRefinement::default());
    element.style().refine(&refinement);
    element
}

/// Gives an intrinsic click handler its native higher-ranked input contract.
///
/// Modifier lowering stores a handler before wrapping it. Routing the value
/// through this identity function preserves contextual typing for otherwise
/// unannotated Rust closures without boxing or allocation.
#[doc(hidden)]
pub fn type_click_handler<Handler>(handler: Handler) -> Handler
where
    Handler: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    handler
}

/// Gives a native mouse-down filter its higher-ranked handler contract.
#[doc(hidden)]
pub fn type_mouse_down_handler<Handler>(handler: Handler) -> Handler
where
    Handler: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    handler
}

/// Gives a `:drag-preview` constructor its native typed entity contract.
///
/// This is the implementation seam used by `view!` when the preview appears
/// before its paired `:drag-payload`. Applications normally use the binding
/// directly; the helper keeps contextual closure typing within `gpui-vue`'s
/// curated UI surface.
#[doc(hidden)]
pub fn type_drag_preview<Payload, Preview, Constructor>(constructor: Constructor) -> Constructor
where
    Payload: 'static,
    Preview: Render + 'static,
    Constructor: Fn(&Payload, ScreenPoint, &mut Window, &mut App) -> Entity<Preview> + 'static,
{
    constructor
}

/// Type-erased exact focus or blur callback retained by the native bridge.
#[doc(hidden)]
pub type FocusEventHandler = Box<dyn FnMut(&mut Window, &mut App) + 'static>;

/// Contextually types and erases one exact focus or blur callback.
#[doc(hidden)]
pub fn boxed_focus_handler<Handler>(handler: Handler) -> FocusEventHandler
where
    Handler: FnMut(&mut Window, &mut App) + 'static,
{
    Box::new(handler)
}

/// Adds exact focus-handle subscriptions around a completed native element.
#[doc(hidden)]
pub fn focus_events<Element>(
    element: Element,
    focus_handle: &FocusHandle,
    on_focus: Option<FocusEventHandler>,
    on_blur: Option<FocusEventHandler>,
) -> FocusEventElement<Element>
where
    Element: gpui::Element,
{
    FocusEventElement {
        element,
        focus_handle: focus_handle.clone(),
        on_focus,
        on_blur,
    }
}

/// Element wrapper that retains exact focus subscriptions with its stable ID.
#[doc(hidden)]
pub struct FocusEventElement<Element> {
    element: Element,
    focus_handle: FocusHandle,
    on_focus: Option<FocusEventHandler>,
    on_blur: Option<FocusEventHandler>,
}

/// Retained callback owner used by [`FocusEventElement`].
struct FocusEventObserver {
    /// Exact native handle currently observed by the subscriptions.
    focus_handle: FocusHandle,
    /// Current callback invoked for exact focus acquisition.
    on_focus: Option<FocusEventHandler>,
    /// Current callback invoked for exact focus loss.
    on_blur: Option<FocusEventHandler>,
    /// Native subscriptions whose drop lifetime follows this retained entity.
    subscriptions: Vec<Subscription>,
}

impl FocusEventObserver {
    /// Creates one observer and subscribes through GPUI's exact Context API.
    fn new(
        focus_handle: &FocusHandle,
        on_focus: Option<FocusEventHandler>,
        on_blur: Option<FocusEventHandler>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let mut observer = Self {
            focus_handle: focus_handle.clone(),
            on_focus,
            on_blur,
            subscriptions: Vec::with_capacity(2),
        };
        observer.subscribe(window, cx);
        observer
    }

    /// Replaces callbacks and follows a changed explicit focus handle.
    fn reconcile(
        &mut self,
        focus_handle: &FocusHandle,
        on_focus: Option<FocusEventHandler>,
        on_blur: Option<FocusEventHandler>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let subscriptions_changed = self.focus_handle != *focus_handle
            || self.on_focus.is_some() != on_focus.is_some()
            || self.on_blur.is_some() != on_blur.is_some();
        self.focus_handle = focus_handle.clone();
        self.on_focus = on_focus;
        self.on_blur = on_blur;
        if subscriptions_changed {
            self.subscriptions.clear();
            self.subscribe(window, cx);
        }
    }

    /// Registers whichever lifecycle callbacks are present on this observer.
    fn subscribe(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.on_focus.is_some() {
            self.subscriptions.push(cx.on_focus(
                &self.focus_handle,
                window,
                |observer, window, cx| {
                    if let Some(handler) = observer.on_focus.as_mut() {
                        handler(window, cx);
                    }
                },
            ));
        }
        if self.on_blur.is_some() {
            self.subscriptions.push(cx.on_blur(
                &self.focus_handle,
                window,
                |observer, window, cx| {
                    if let Some(handler) = observer.on_blur.as_mut() {
                        handler(window, cx);
                    }
                },
            ));
        }
    }
}

impl<Element> gpui::Element for FocusEventElement<Element>
where
    Element: gpui::Element,
{
    type RequestLayoutState = Element::RequestLayoutState;
    type PrepaintState = Element::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.element.id()
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        self.element.source_location()
    }

    fn request_layout(
        &mut self,
        global_id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        self.element
            .request_layout(global_id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.element
            .prepaint(global_id, inspector_id, bounds, request_layout, window, cx)
    }

    fn paint(
        &mut self,
        global_id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.element.paint(
            global_id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );

        let Some(global_id) = global_id else {
            debug_assert!(false, "focus event hosts require a stable element id");
            return;
        };
        let on_focus = self.on_focus.take();
        let on_blur = self.on_blur.take();
        let focus_handle = &self.focus_handle;
        window.with_element_state::<gpui::Entity<FocusEventObserver>, _>(
            global_id,
            |observer, window| {
                let observer = if let Some(observer) = observer {
                    observer.update(cx, |observer, observer_cx| {
                        observer.reconcile(focus_handle, on_focus, on_blur, window, observer_cx);
                    });
                    observer
                } else {
                    cx.new(|observer_cx| {
                        FocusEventObserver::new(
                            focus_handle,
                            on_focus,
                            on_blur,
                            window,
                            observer_cx,
                        )
                    })
                };
                ((), observer)
            },
        );
    }
}

impl<Element> IntoElement for FocusEventElement<Element>
where
    Element: gpui::Element,
{
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Styled as _;

    #[test]
    fn curated_constructors_preserve_native_values() {
        assert_eq!(px(12.5), gpui::px(12.5));
        assert_eq!(rgb(0x12_34_56), gpui::rgb(0x12_34_56));
        assert_eq!(rgba(0x12_34_56_78), gpui::rgba(0x12_34_56_78));
        let _ = div();
        let _ = image("https://example.invalid/image.png");
    }

    #[test]
    fn runtime_style_refinement_overrides_existing_base_slots() {
        let mut element = apply_style_refinement(div().text_color(rgb(0x11_22_33)), |style| {
            style.text_color(rgb(0xAA_BB_CC)).w(px(42.0))
        });

        let style = element.style();
        assert_eq!(
            style.text.as_ref().and_then(|text| text.color),
            Some(rgb(0xAA_BB_CC).into()),
        );
        assert_eq!(style.size.width, Some(px(42.0).into()));
    }

    #[test]
    fn clipboard_text_conversion_preserves_unicode_multiline_and_empty_values() {
        for source in ["永\nKAGE".to_owned(), String::new()] {
            let item = text_clipboard_item(source.clone());
            assert_eq!(clipboard_item_text(Some(item)), Some(source));
        }
        assert_eq!(clipboard_item_text(None), None);
    }
}
