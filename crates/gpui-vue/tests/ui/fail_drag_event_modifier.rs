//! Typed drag/drop events reject DOM-style event modifiers.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div id="target" on:drop.stop={drop_handler} />
    };
}
