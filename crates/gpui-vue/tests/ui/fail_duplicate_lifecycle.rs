//! A lifecycle section may be declared only once.

use gpui_vue::component;

component! {
    /// A component with two mounted sections.
    component DuplicateLifecycle {
        mounted(_this, _window, _cx) {}
        mounted(_this, _window, _cx) {}

        template(_this, _window, _cx) {
            view! { <div /> }
        }
    }
}

fn main() {}
