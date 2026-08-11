//! Path/self keywords cannot be turned into raw component builder methods.

use gpui_vue::view;

fn main() {
    let _ = view! { <Child self={1_usize} /> };
}
