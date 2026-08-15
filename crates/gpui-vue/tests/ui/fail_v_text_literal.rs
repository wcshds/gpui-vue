//! `v-text` accepts a typed Rust expression, not an HTML-style literal value.

use gpui_vue::view;

fn main() {
    let _ = view! { <span v-text="literal" /> };
}
