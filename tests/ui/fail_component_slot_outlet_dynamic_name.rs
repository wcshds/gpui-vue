//! Heterogeneous typed slots cannot be selected by a runtime string.

use gpui_vue::component;

component! {
    /// Component attempting a dynamic outlet lookup.
    component DynamicOutletName {
        slots {
            /// Default unit content.
            default: ();
        }

        template(_this, _window, _cx) {
            <slot :name={"default"} />
        }
    }
}

fn main() {}
