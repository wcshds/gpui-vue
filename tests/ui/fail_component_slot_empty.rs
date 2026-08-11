//! A present named slot provider must produce an actual child node.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <Child :props={()}>
            <template #actions />
        </Child>
    };
}
