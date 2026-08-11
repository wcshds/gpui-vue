//! A direct component outlet must select one declared typed slot.

use gpui_vue::component;

component! {
    /// Component selecting an undeclared outlet.
    component UnknownOutlet {
        slots {
            /// The only declared content slot.
            default: ();
        }

        template(_this, _window, _cx) {
            <slot name="missing" />
        }
    }
}

fn main() {}
