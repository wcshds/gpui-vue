//! Components without a slots declaration cannot accept declarative children.

use gpui_vue::{component, view};

component! {
    /// Component intentionally declaring no slots.
    component NoSlotsChild {
        template(_this, _window, _cx) {
            gpui_vue::gpui::div()
        }
    }
}

fn main() {
    let _ = view! {
        <NoSlotsChild><text>"inline child"</text></NoSlotsChild>
    };
}
