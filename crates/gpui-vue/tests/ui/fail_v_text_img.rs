//! Images have no native parent-element child lane for `v-text`.

use gpui_vue::view;

fn main() {
    let label = "invalid image child";
    let _ = view! { <img src="icon.png" v-text={label} /> };
}
