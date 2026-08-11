//! DOM-only passive listener registration must not acquire fake GPUI semantics.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <button id="save" @click.passive={|_, _, _| {}}>"Save"</button>
    };
}
