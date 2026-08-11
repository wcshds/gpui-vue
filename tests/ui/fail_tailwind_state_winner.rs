//! Conflicting simultaneous state winners must fail at the class literal.

use gpui_vue::view;

fn main() {
    let _ = view! { <div id="state-winner" class="focus:block hover:flex" /> };
}
