//! Slot outlets have no host on which to apply `v-show`.

use gpui_vue::component;

component! {
    /// Component attempting host visibility on an outlet.
    component VisibleOutlet {
        slots {
            /// Default unit content.
            default: ();
        }

        template(_this, _window, _cx) {
            <slot v-show={true} />
        }
    }
}

fn main() {}
