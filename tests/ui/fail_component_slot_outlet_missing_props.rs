//! A non-unit typed outlet requires an exact props expression.

use gpui_vue::component;

/// Props supplied to the action provider.
struct ActionProps {
    /// Current action count.
    count: usize,
}

component! {
    /// Component omitting required scoped-slot props.
    component MissingOutletProps {
        slots {
            /// Typed action content.
            actions: ActionProps;
        }

        template(_this, _window, _cx) {
            <slot name="actions" />
        }
    }
}

fn main() {}
