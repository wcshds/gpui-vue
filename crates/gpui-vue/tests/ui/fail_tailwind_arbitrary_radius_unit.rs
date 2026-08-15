//! Arbitrary radii accept physical px/rem lengths, not percentages.

use gpui_vue::view;

fn main() {
    let _ = view! { <div class="rounded-[50%]" /> };
}
