//! Repeated outlets cannot safely share one provider identity in P0.

use gpui_vue::component;

component! {
    /// Component invoking one provider twice under the same GPUI ancestor.
    component DuplicateOutlet {
        slots {
            /// Default unit content.
            default: ();
        }

        template(_this, _window, _cx) {
            <div>
                <slot />
                <slot />
            </div>
        }
    }
}

fn main() {}
