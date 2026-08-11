//! An unknown Tailwind candidate must fail during macro expansion.

use gpui_vue::view;

fn main() {
    let _ = view! { <div class="definitely-not-a-tailwind-utility" /> };
}
