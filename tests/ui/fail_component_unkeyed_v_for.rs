//! Repeated PascalCase component hosts require a bound item-derived key.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div>
            <Child v-for={item in 0..3} :props={()} />
        </div>
    };
}
