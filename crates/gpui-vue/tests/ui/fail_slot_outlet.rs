//! A standalone `view!` has no receiving component metadata for `<slot>`.

use gpui_vue::view;

fn main() {
    let _ = view! { <slot /> };
}
