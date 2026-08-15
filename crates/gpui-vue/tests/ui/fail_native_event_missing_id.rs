//! Native event hosts must provide stable GPUI identity.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div @key-up={|_, _, _| {}} />
    };
}
