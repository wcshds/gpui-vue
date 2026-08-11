//! PascalCase component events reject every modifier in the initial typed lane.

use gpui_vue::{component, view};

component! {
    /// Emits one typed event.
    component Emitter {
        emits {
            /// Reports a change.
            change();
        }

        template(_this, _window, _cx) {
            gpui_vue::gpui::div()
        }
    }
}

fn main() {
    let handler = || {};
    let _ = view! { <Emitter @change.stop={handler} /> };
}
