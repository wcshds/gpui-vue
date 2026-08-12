//! Named slot providers may occur only once per component site.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <Child :props={()}>
            <template #actions><text>"first"</text></template>
            <template #actions><text>"second"</text></template>
        </Child>
    };
}
