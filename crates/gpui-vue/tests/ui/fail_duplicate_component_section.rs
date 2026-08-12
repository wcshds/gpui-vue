//! Each component section may occur at most once.

use gpui_vue::component;

component! {
    /// A component containing an intentionally duplicated section.
    component DuplicateSections {
        props {}
        props {}

        template(this, _window, _cx) {
            unimplemented!()
        }
    }
}

fn main() {}
