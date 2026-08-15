//! Every stateful drag/drop host must provide stable native identity.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div @drop={drop_handler} />
    };
}
