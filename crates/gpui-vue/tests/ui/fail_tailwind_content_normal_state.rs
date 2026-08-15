//! The native alignment reset cannot clear a base field from a state refinement.

use gpui_vue::view;

fn main() {
    let _ = view! { <div class="hover:content-normal" /> };
}
