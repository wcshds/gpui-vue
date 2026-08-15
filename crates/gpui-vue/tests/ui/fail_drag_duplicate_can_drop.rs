//! GPUI exposes one type-erased drop predicate per native host.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div id="target" :can-drop={first} :can-drop={second} />
    };
}
