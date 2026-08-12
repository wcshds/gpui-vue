//! PascalCase individual props retain typestate errors for missing required values.

use gpui_vue::{component, view};

component! {
    /// Component with one required property deliberately omitted below.
    component Child {
        props {
            /// Required label.
            label: String,
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().label.clone()}</text> }
        }
    }
}

fn main() {
    let _ = view! { <Child /> };
}
