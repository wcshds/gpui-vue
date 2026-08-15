//! Loading replacements must return GPUI's exact type-erased element.

use gpui_vue::view;

fn main() {
    let _ = view! { <img src="images/avatar.png" :loading={|| 42_usize} /> };
}
