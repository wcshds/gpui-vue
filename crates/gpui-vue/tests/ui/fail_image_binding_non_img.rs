//! Native image policy bindings must not silently attach to container hosts.

use gpui_vue::view;

fn main() {
    let _ = view! { <div :loading={|| unreachable!()} /> };
}
