//! Outside mouse-down routing needs one explicit native mouse button.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div id="surface" @mouse-down-out={|_, _, _| {}} />
    };
}
