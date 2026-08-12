//! Lifecycle hook bodies must evaluate to unit.

use gpui_vue::{component, view};

component! {
    /// A component whose mounted hook returns a value.
    component NonUnitLifecycle {
        mounted(_this, _window, _cx) {
            7usize
        }

        template(_this, _window, _cx) {
            view! { <div /> }
        }
    }
}

fn main() {}
