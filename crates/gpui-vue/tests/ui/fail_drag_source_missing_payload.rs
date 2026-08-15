//! A native drag preview must have one typed payload on the same host.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div id="source" :drag-preview={preview} />
    };
}
