//! Complete and individual PascalCase props modes are mutually exclusive.

use gpui_vue::{component, view};

component! {
    /// Component used to exercise props construction mode validation.
    component Child {
        props {
            /// Required label.
            label: String,
        }

        template(this, _window, _cx) {
            view! { <text>{this.props().label.clone()}</text> }
        }
    }
}

fn main() {
    let props = ChildProps::new("complete".to_owned());
    let _ = view! {
        <Child :props={props} label={"individual".to_owned()} />
    };
}
