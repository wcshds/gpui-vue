//! Vue-inspired compile-time authoring for GPUI, without a JavaScript runtime or a VDOM.
//!
//! [`view!`] parses an RSX-like template at compile time and emits native GPUI
//! element builders. Tailwind-like class literals are also checked and lowered
//! at compile time, so no class parser or CSS engine ships in the application.

extern crate self as gpui_vue;

pub use gpui;
pub use gpui_vue_macros::{component, view};

pub mod animation;
pub mod assets;
pub mod async_state;
pub mod component;
#[cfg(feature = "desktop")]
pub mod desktop;
pub mod effects;
pub mod http;
pub mod local;
pub mod media;
pub mod overlay;
pub mod paint;
pub mod reactivity;
pub mod slot;
pub mod state;
pub mod text_input;
pub mod ui;
pub mod virtual_list;

pub use assets::EmbeddedAssets;
pub use async_state::{AsyncResource, AsyncState};
pub use component::{
    ComponentElement, ComponentEventElement, ComponentEventMount, ComponentLifecycleHooks,
    ComponentLifecycleMount, ComponentMount, HostedEntity, LifecycleRenderToken, NativeComponent,
    NativeComponentEvents, NativeComponentMount, NativeComponentSlots, PropMissing, PropSet,
    RequiredProp, component_element, component_element_with_events,
};
pub use effects::{
    AsyncContext, AsyncWindowContext, EffectScope, WeakOwner, defer, next_frame, on_release, spawn,
    spawn_in, watch_entity, watch_entity_in, watch_event, watch_event_in,
};
pub use local::{Local, Memo, Revision};
pub use overlay::{
    AnchoredOverlay, DeferredOverlay, OverlayCorner, OverlayFit, OverlayInsets,
    OverlayPositionMode, anchored_overlay, deferred_overlay,
};
pub use reactivity::{ChangeNotifier, Ref, reactive_ref, ref_};
pub use slot::{Slot, SlotContent};
pub use state::{Global, ReadGlobal, UpdateGlobal};
pub use text_input::{
    TextInput, TextInputConfig, TextInputEvent, TextInputHandle, TextInputStyle, TextModelBinding,
    text_input, text_input_with_config,
};

/// Common imports for components built with this crate.
pub mod prelude {
    pub use crate::{
        AsyncResource, AsyncState, ChangeNotifier, ComponentElement, ComponentEventElement,
        ComponentEventMount, ComponentMount, EffectScope, EmbeddedAssets, Global, Local, Memo,
        NativeComponent, NativeComponentEvents, NativeComponentSlots, OverlayCorner, OverlayInsets,
        PropMissing, PropSet, ReadGlobal, Ref, Revision, Slot, SlotContent, TextInput,
        TextInputConfig, TextInputEvent, TextInputHandle, TextInputStyle, TextModelBinding,
        UpdateGlobal, anchored_overlay, component, component_element,
        component_element_with_events, deferred_overlay, reactive_ref, ref_, spawn, spawn_in,
        text_input, text_input_with_config, view,
    };
    pub use gpui::prelude::*;
}
