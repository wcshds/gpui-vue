//! Lazy, typed slot content for generated components.

use gpui::{AnyElement, App, IntoElement, Window};

/// The erased renderer stored by a [`Slot`].
type SlotRenderer<Props> = dyn Fn(Props, &mut Window, &mut App) -> SlotContent + 'static;

/// One lazily produced GPUI element.
///
/// A slot may return any concrete [`IntoElement`] type. The concrete type is
/// erased exactly once when the slot is invoked so generated components can
/// store heterogeneous slot providers without introducing a virtual DOM.
pub struct SlotContent {
    /// The single type-erased GPUI element produced by the slot.
    element: AnyElement,
}

impl SlotContent {
    /// Erases one concrete GPUI element at the slot boundary.
    #[must_use]
    pub fn new(element: impl IntoElement) -> Self {
        Self {
            element: element.into_any_element(),
        }
    }

    /// Returns the underlying type-erased GPUI element.
    #[must_use]
    pub fn into_inner(self) -> AnyElement {
        self.element
    }
}

impl IntoElement for SlotContent {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        self.element
    }

    fn into_any_element(self) -> AnyElement {
        self.element
    }
}

/// An optional, lazy slot provider with a statically checked props type.
///
/// `Slot<Props>` owns a Rust closure and invokes it only when the receiving
/// component requests the slot. Captured values must therefore be `'static`,
/// matching GPUI's retained entity lifetime. An empty slot stores no closure.
pub struct Slot<Props> {
    /// The provider, absent when a parent did not supply this slot.
    renderer: Option<Box<SlotRenderer<Props>>>,
}

impl<Props> Slot<Props> {
    /// Creates an empty slot.
    #[must_use]
    pub const fn empty() -> Self {
        Self { renderer: None }
    }

    /// Creates a slot from a lazy typed Rust closure.
    #[must_use]
    pub fn new<Element>(
        renderer: impl Fn(Props, &mut Window, &mut App) -> Element + 'static,
    ) -> Self
    where
        Element: IntoElement,
    {
        Self {
            renderer: Some(Box::new(move |props, window, cx| {
                SlotContent::new(renderer(props, window, cx))
            })),
        }
    }

    /// Creates a slot whose provider does not need GPUI render context.
    #[must_use]
    pub fn from_fn<Element>(renderer: impl Fn(Props) -> Element + 'static) -> Self
    where
        Element: IntoElement,
    {
        Self::new(move |props, _window, _cx| renderer(props))
    }

    /// Returns whether a provider was supplied.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        self.renderer.is_some()
    }

    /// Invokes the provider when present.
    #[must_use]
    pub fn render(&self, props: Props, window: &mut Window, cx: &mut App) -> Option<SlotContent> {
        self.renderer
            .as_ref()
            .map(|renderer| renderer(props, window, cx))
    }

    /// Invokes the provider, or lazily builds one fallback element.
    #[must_use]
    pub fn render_or_else<Element>(
        &self,
        props: Props,
        window: &mut Window,
        cx: &mut App,
        fallback: impl FnOnce(Props, &mut Window, &mut App) -> Element,
    ) -> SlotContent
    where
        Element: IntoElement,
    {
        match &self.renderer {
            Some(renderer) => renderer(props, window, cx),
            None => SlotContent::new(fallback(props, window, cx)),
        }
    }
}

impl<Props> Default for Slot<Props> {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    //! Runtime behavior and representation checks for typed slots.

    use std::mem::size_of;

    use gpui::{AnyElement, App, ParentElement, Window, div};

    use super::{Slot, SlotContent, SlotRenderer};

    /// Providers remain lazy until a render context invokes them.
    #[test]
    fn construction_does_not_invoke_provider() {
        let slot = Slot::new(|_value: usize, _window, _cx| -> gpui::Div {
            panic!("slot construction must stay lazy")
        });
        assert!(slot.is_present());

        let empty = Slot::<usize>::empty();
        assert!(!empty.is_present());

        let context_free = Slot::from_fn(|value: usize| div().child(value.to_string()));
        assert!(context_free.is_present());

        let _: fn(&Slot<usize>, usize, &mut Window, &mut App) -> Option<SlotContent> =
            Slot::<usize>::render;
    }

    /// The common one-node result has no collection wrapper.
    #[test]
    fn one_node_content_matches_any_element_representation() {
        assert_eq!(size_of::<SlotContent>(), size_of::<AnyElement>());
        assert_eq!(
            size_of::<Slot<()>>(),
            size_of::<Option<Box<SlotRenderer<()>>>>()
        );
    }
}
