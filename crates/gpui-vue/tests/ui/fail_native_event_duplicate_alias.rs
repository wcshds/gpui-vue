//! At-sign and on-colon spellings share one canonical native event slot.

use gpui_vue::view;

fn main() {
    let _ = view! {
        <div
            id="editor"
            @modifiers-changed={|_, _, _| {}}
            on:modifiers-changed={|_, _, _| {}}
        />
    };
}
