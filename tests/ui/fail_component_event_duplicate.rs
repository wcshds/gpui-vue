//! Kebab and snake spellings of one component event are canonical duplicates.

use gpui_vue::{component, view};

component! {
    /// Emits one typed event.
    component Emitter {
        emits {
            /// Reports a value change.
            value_change();
        }

        template(_this, _window, _cx) {
            gpui_vue::gpui::div()
        }
    }
}

fn main() {
    let first = || {};
    let second = || {};
    let _ = view! {
        <Emitter @value-change={first} on:value_change={second} />
    };
}
