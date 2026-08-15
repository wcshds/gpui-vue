//! A native drag source must pair its payload with an entity preview constructor.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div id="source" :drag-payload={payload} />
    };
}
