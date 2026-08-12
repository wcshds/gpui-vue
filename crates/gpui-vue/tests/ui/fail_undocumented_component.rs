//! User-authored component declarations must carry API documentation.

use gpui_vue::component;

component! {
    pub component Undocumented {
        template(this, _window, _cx) {
            unimplemented!()
        }
    }
}

fn main() {}
