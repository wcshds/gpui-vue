//! Object fit is a typed GPUI enum, not a DOM/CSS string attribute.

use gpui_vue::view;

fn main() {
    let _ = view! { <img src="images/avatar.png" :object-fit="cover" /> };
}
