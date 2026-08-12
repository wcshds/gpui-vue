//! Repeated roots require a dynamic key that namespaces GPUI element state.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div>
            <span v-for={item in 0..3}>{item}</span>
        </div>
    };
}
