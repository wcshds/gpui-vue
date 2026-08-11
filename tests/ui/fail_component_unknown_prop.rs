//! Unknown PascalCase props are diagnosed by the generated Rust builder API.

use gpui_vue::{component, view};

component! {
    /// Component used to exercise Rust's unknown-setter diagnostic.
    component Child {
        props {
            /// Known display name with a default.
            display_name: &'static str = "known",
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().display_name}</text> }
        }
    }
}

fn main() {
    let _ = view! {
        <Child unknown={41} />
    };
}
