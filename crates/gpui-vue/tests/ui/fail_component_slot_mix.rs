//! Explicit and declarative component slots are mutually exclusive.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <Child :props={()} :slots={()}>
            <text>"inline child"</text>
        </Child>
    };
}
