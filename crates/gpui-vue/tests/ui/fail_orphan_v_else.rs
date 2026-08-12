//! A conditional fallback must be adjacent to a preceding conditional branch.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div>
            <span v-else>"orphan"</span>
        </div>
    };
}
