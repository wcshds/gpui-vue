//! Focus-visible and hover conflicts must preserve the Tailwind winner.

use gpui_vue::view;

fn main() {
    let _ = view! { <div id="state-winner" class="focus-visible:block hover:flex" /> };
}
