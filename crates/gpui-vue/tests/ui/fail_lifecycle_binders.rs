//! Lifecycle hook binders must be distinct.

use gpui_vue::component;

component! {
    /// A component that repeats its unmounted binder.
    component RepeatedLifecycleBinder {
        unmounted(this, this) {}

        template(_this, _window, _cx) {
            view! { <div /> }
        }
    }
}

fn main() {}
