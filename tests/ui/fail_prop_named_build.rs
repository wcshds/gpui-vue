//! `build` is reserved for the generated props builder terminal method.

use gpui_vue::component;

component! {
    /// Compile-fail fixture with a reserved property name.
    component ReservedBuild {
        props {
            /// Invalid collision with the terminal builder method.
            build: usize,
        }

        template(_this, _window, _cx) {
            view! { <div /> }
        }
    }
}

fn main() {}
