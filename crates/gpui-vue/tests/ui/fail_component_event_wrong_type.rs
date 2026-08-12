//! A component listener must accept the component's complete generated event enum.

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
    let _ = view! {
        <Emitter @change={
            |_event: &str,
             _window: &mut gpui_vue::gpui::Window,
             _cx: &mut gpui_vue::gpui::App| {}
        } />
    };
}
