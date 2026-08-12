//! Unknown slot names remain ordinary generated-builder method errors.

use gpui_vue::{component, view};

component! {
    /// Component exposing only its default slot.
    component Child {
        slots {
            /// The only accepted content provider.
            default: ();
        }

        template(_this, _window, _cx) {
            gpui_vue::gpui::div()
        }
    }
}

fn main() {
    let _ = view! {
        <Child>
            <template #missing><text>"unknown"</text></template>
        </Child>
    };
}
