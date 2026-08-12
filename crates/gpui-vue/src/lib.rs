//! Vue-inspired compile-time authoring for GPUI, without a JavaScript runtime or a VDOM.
//!
//! [`view!`] parses an RSX-like template at compile time and emits native GPUI
//! element builders. Tailwind-like class literals are also checked and lowered
//! at compile time, so no class parser or CSS engine ships in the application.

extern crate self as gpui_vue;

pub use gpui;
pub use gpui_vue_macros::{component, view};

pub mod component;
pub mod local;
pub mod reactivity;
pub mod slot;

pub use component::{
    ComponentElement, ComponentEventElement, ComponentEventMount, ComponentLifecycleHooks,
    ComponentLifecycleMount, ComponentMount, HostedEntity, LifecycleRenderToken, NativeComponent,
    NativeComponentEvents, NativeComponentMount, NativeComponentSlots, PropMissing, PropSet,
    RequiredProp, component_element, component_element_with_events,
};
pub use local::{Local, Memo, Revision};
pub use reactivity::{ChangeNotifier, Ref, reactive_ref, ref_};
pub use slot::{Slot, SlotContent};

/// Common imports for components built with this crate.
pub mod prelude {
    pub use crate::{
        ChangeNotifier, ComponentElement, ComponentEventElement, ComponentEventMount,
        ComponentMount, Local, Memo, NativeComponent, NativeComponentEvents, NativeComponentSlots,
        PropMissing, PropSet, Ref, Revision, Slot, SlotContent, component, component_element,
        component_element_with_events, reactive_ref, ref_, view,
    };
    pub use gpui::prelude::*;
}
