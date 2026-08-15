//! Non-click native events do not accept DOM listener modifiers.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div id="surface" @hover.passive={|_, _, _| {}} />
    };
}
