//! Raw-keyword props and slots must diagnose type errors without panicking.

use gpui_vue::component;

/// Props supplied through the raw-keyword slot.
struct RawSlotProps;

component! {
    /// Component combining raw-keyword property and slot declarations.
    component RawOutletProps {
        props {
            /// Defaulted raw-keyword property whose override is `with_type`.
            r#type: usize = 0,
        }

        slots {
            /// Raw-keyword slot selected as `name="type"`.
            r#type: RawSlotProps;
        }

        template(_this, _window, _cx) {
            <slot name="type" :props={()} />
        }
    }
}

fn main() {}
