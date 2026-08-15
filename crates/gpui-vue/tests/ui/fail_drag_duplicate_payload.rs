//! A host can retain only one native drag-source payload.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div
            id="source"
            :drag-payload={first}
            :drag-payload={second}
            :drag-preview={preview}
        />
    };
}
