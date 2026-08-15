//! `v-text` owns the intrinsic's sole native child lane.

use gpui_vue::view;

fn main() {
    let label = "replacement";
    let _ = view! {
        <div v-text={label}>"existing child"</div>
    };
}
