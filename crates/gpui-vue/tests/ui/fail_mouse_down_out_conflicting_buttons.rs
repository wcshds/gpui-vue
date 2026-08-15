//! Outside mouse-down routing cannot select more than one button.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div id="surface" @mouse-down-out.left.right={|_, _, _| {}} />
    };
}
