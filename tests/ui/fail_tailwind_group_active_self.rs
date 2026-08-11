//! A native group must not let group-active self-match the same element.

use gpui_vue::view;

fn main() {
    let _ = view! { <div id="self-active" class="group group-active:block" /> };
}
