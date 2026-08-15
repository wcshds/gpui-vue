//! Public contract coverage for the native single-line text input.

use gpui_vue::gpui::{Context, EventEmitter, Focusable, Render};
use gpui_vue::ui::{px, rgb};
use gpui_vue::{
    Local, TextInput, TextInputConfig, TextInputEvent, TextInputHandle, TextInputStyle,
    TextModelBinding, text_input, text_input_with_config,
};

/// A parent type used only to type-check the convenience constructor.
struct Parent;

/// Parent shape used to compile-check local model synchronization.
struct ControlledParent {
    /// Canonical parent-owned value.
    value: Local<String>,
}

/// The public input type remains a native retained, focusable event emitter.
#[test]
fn text_input_public_contract_is_typed() {
    fn assert_control<T>()
    where
        T: Render + Focusable + EventEmitter<TextInputEvent>,
    {
    }

    assert_control::<TextInput>();
    let _: Option<TextInputHandle> = None;
}

/// External callers can construct and perform controlled updates without GPUI
/// input-handler types appearing in their signatures.
#[allow(dead_code)]
fn public_usage_type_checks(parent_cx: &mut Context<'_, Parent>) {
    let handle = text_input("搜尋部件", parent_cx);
    handle.update(parent_cx, |input, cx| {
        input.set_placeholder("Search components", cx);
        input.set_text("永", cx);
        assert_eq!(input.text(), "永");
        assert_eq!(input.selected_range(), 3..3);
        assert!(!input.is_composing());
        input.clear(cx);
    });
}

/// Typed style, configuration, and local model binding stay GPUI-builder free.
#[allow(dead_code)]
fn configured_and_bound_usage_type_checks(
    parent_cx: &mut Context<'_, ControlledParent>,
) -> TextModelBinding {
    let style = TextInputStyle::default()
        .width(px(260.0))
        .height(px(36.0))
        .padding_x(px(10.0))
        .background_color(rgb(0x18_18_1b))
        .text_color(rgb(0xf4_f4_f5))
        .placeholder_color(rgb(0x71_71_7a))
        .border_color(rgb(0x3f_3f_46))
        .focus_border_color(rgb(0x3b_82_f6))
        .selection_color(rgb(0x1d_4e_d8))
        .caret_color(rgb(0x60_a5_fa))
        .border_width(px(1.0))
        .corner_radius(px(8.0))
        .font_family(".SystemUIFont")
        .font_size(px(14.0))
        .disabled_opacity(0.4);
    let input = text_input_with_config(
        TextInputConfig::new("搜尋")
            .value("永")
            .style(style)
            .max_length(24),
        parent_cx,
    );
    TextModelBinding::bind(
        &input,
        String::new(),
        parent_cx,
        |parent| parent.value.get(),
        |parent, value, cx| {
            parent.value.set(value, cx);
        },
    )
}

/// Events carry owned values suitable for deferred subscriptions.
#[test]
fn text_input_events_are_owned_and_comparable() {
    assert_eq!(
        TextInputEvent::Change("永".to_owned()),
        TextInputEvent::Change("永".to_owned()),
    );
    assert_eq!(
        TextInputEvent::Submit("glyph".to_owned()),
        TextInputEvent::Submit("glyph".to_owned()),
    );
    assert_eq!(TextInputEvent::Escape, TextInputEvent::Escape);
}
