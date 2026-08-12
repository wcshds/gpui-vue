//! Procedural macros for the `gpui-vue` runtime crate.

use proc_macro::TokenStream;

/// Typed component item parsing and native GPUI entity lowering.
mod component;
/// Compile-time Tailwind candidate parsing and native GPUI style lowering.
mod tailwind;
/// Vue-shaped template parsing, validation, and native GPUI lowering.
mod view;

/// Build a native GPUI element tree from a Vue/RSX-like template.
///
/// The template and every `class="..."` literal are parsed at compile time.
/// The expansion contains only ordinary GPUI element-builder calls.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    view::expand(&input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declare a typed GPUI component with props, state, events, slots, setup, and a template.
///
/// The macro emits ordinary Rust props and component structs, an entity
/// constructor backed by `AppContext::new`, and a native GPUI [`Render`]
/// implementation. An `emits` section becomes a typed event enum backed directly
/// by GPUI's [`EventEmitter`] and `Context::emit` protocol. A `slots` section
/// becomes a typed collection of lazy `gpui_vue::Slot` renderers. The template
/// accepts either its original Rust block (including `view!`) or direct markup;
/// direct markup can invoke typed child outlets with `<slot />` and named
/// `<slot name="..." :props={...}>fallback</slot>` syntax. State initializers
/// and the optional setup hook run exactly once for each constructed entity;
/// the template runs on every GPUI render.
///
/// [`Render`]: https://docs.rs/gpui/0.2.2/gpui/trait.Render.html
/// [`EventEmitter`]: https://docs.rs/gpui/0.2.2/gpui/trait.EventEmitter.html
#[proc_macro]
pub fn component(input: TokenStream) -> TokenStream {
    component::expand(&input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
