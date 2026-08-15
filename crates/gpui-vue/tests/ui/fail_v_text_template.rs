//! A structural template is flattened and therefore cannot own `v-text`.

use gpui_vue::view;

fn main() {
    let label = "invalid structural child";
    let _ = view! { <template v-text={label} /> };
}
