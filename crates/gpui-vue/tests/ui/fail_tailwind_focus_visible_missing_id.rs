//! Focus-visible styling needs stable retained focus identity.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div class="focus-visible:block" />
    };
}
