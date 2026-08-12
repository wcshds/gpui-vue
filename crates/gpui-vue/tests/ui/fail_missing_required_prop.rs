//! A typestate props builder must not expose `build` while a required prop is missing.

use gpui_vue::{component, view};

component! {
    /// Compile-fail fixture with one required property.
    component RequiredBuilder {
        props {
            /// Required label deliberately omitted by `main`.
            label: String,
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().label.clone()}</text> }
        }
    }
}

fn main() {
    let _ = RequiredBuilderProps::builder().build();
}
