//! A component without slot declarations cannot contain an outlet.

use gpui_vue::component;

component! {
    /// Component with no typed slots.
    component NoDeclaredSlots {
        template(_this, _window, _cx) {
            <slot />
        }
    }
}

fn main() {}
