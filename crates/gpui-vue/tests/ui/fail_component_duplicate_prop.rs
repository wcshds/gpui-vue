//! Kebab- and snake-case spellings of one PascalCase prop are duplicates.

use gpui_vue::{component, view};

component! {
    /// Component used to exercise canonical prop-name validation.
    component Child {
        props {
            /// Static display name.
            display_name: &'static str,
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().display_name}</text> }
        }
    }
}

fn main() {
    let _ = view! {
        <Child display-name="first" display_name="second" />
    };
}
