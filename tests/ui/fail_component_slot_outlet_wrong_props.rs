//! Slot outlet props keep the exact declared Rust type.

use gpui_vue::component;

/// Props supplied to the action provider.
struct ActionProps {
    /// Current action count.
    count: usize,
}

component! {
    /// Component passing the wrong scoped-slot props type.
    component WrongOutletProps {
        slots {
            /// Typed action content.
            actions: ActionProps;
        }

        template(_this, _window, _cx) {
            <slot name="actions" :props={7_usize} />
        }
    }
}

fn main() {}
