//! Retained slot providers cannot capture a non-static parent borrow.

use gpui_vue::gpui::IntoElement;
use gpui_vue::{component, view};

component! {
    /// Component accepting one retained default provider.
    component Child {
        slots {
            /// Parent-owned body content.
            default: ();
        }

        template(_this, _window, _cx) {
            gpui_vue::gpui::div()
        }
    }
}

fn borrowed_provider(borrowed: &str) -> impl IntoElement + '_ {
    view! {
        <Child>
            <text>{borrowed.to_owned()}</text>
        </Child>
    }
}

fn main() {}
