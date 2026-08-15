//! PascalCase components receive typed props and slots, not intrinsic `v-text`.

use gpui_vue::view;

fn main() {
    let label = "invalid component directive";
    let _ = view! { <Child v-text={label} /> };
}
