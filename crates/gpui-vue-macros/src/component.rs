//! Parser and Rust item lowering for the `component!` macro.

use std::collections::HashSet;

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Block, Expr, Ident, Result, Token, Type, Visibility, braced, parenthesized};

/// Keywords used by the component item grammar.
mod keyword {
    syn::custom_keyword!(component);
    syn::custom_keyword!(emits);
    syn::custom_keyword!(mounted);
    syn::custom_keyword!(props);
    syn::custom_keyword!(slots);
    syn::custom_keyword!(state);
    syn::custom_keyword!(setup);
    syn::custom_keyword!(template);
    syn::custom_keyword!(unmounted);
    syn::custom_keyword!(updated);
}

/// Parses one component declaration and emits its ordinary Rust items.
pub(crate) fn expand(input: &TokenStream) -> Result<TokenStream> {
    let definition = syn::parse2::<ComponentDefinition>(input.clone())?;
    definition.validate()?;
    definition.lower()
}

/// One complete component declaration.
struct ComponentDefinition {
    /// Attributes copied to the generated component type.
    attributes: Vec<Attribute>,
    /// Visibility shared by the component, props type, and constructors.
    visibility: Visibility,
    /// Component type name.
    name: Ident,
    /// Typed construction properties.
    properties: Vec<Property>,
    /// Typed entity-local state fields.
    state: Vec<StateField>,
    /// Typed events emitted through the native GPUI event channel.
    emissions: Vec<Emission>,
    /// Lazy typed content providers supplied by a parent component.
    slots: Vec<SlotDeclaration>,
    /// Optional one-shot setup hook.
    setup: Option<SetupHook>,
    /// Optional first-render visual lifecycle hook.
    mounted: Option<RenderedLifecycleHook>,
    /// Optional dirty-render visual lifecycle hook.
    updated: Option<RenderedLifecycleHook>,
    /// Optional visual-host teardown hook.
    unmounted: Option<UnmountedLifecycleHook>,
    /// Required render hook.
    template: TemplateHook,
}

impl Parse for ComponentDefinition {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let attributes = input.call(Attribute::parse_outer)?;
        let visibility = input.parse()?;
        input.parse::<keyword::component>()?;
        let name = Ident::parse_any(input)?;

        let content;
        braced!(content in input);
        let mut properties = None;
        let mut state = None;
        let mut emissions = None;
        let mut slots = None;
        let mut setup = None;
        let mut mounted = None;
        let mut updated = None;
        let mut unmounted = None;
        let mut template = None;

        while !content.is_empty() {
            if content.peek(keyword::props) {
                let keyword = content.parse::<keyword::props>()?;
                set_section(
                    &mut properties,
                    parse_properties(&content)?,
                    keyword.span,
                    "props",
                )?;
            } else if content.peek(keyword::state) {
                let keyword = content.parse::<keyword::state>()?;
                set_section(&mut state, parse_state(&content)?, keyword.span, "state")?;
            } else if content.peek(keyword::emits) {
                let keyword = content.parse::<keyword::emits>()?;
                set_section(
                    &mut emissions,
                    parse_emissions(&content)?,
                    keyword.span,
                    "emits",
                )?;
            } else if content.peek(keyword::slots) {
                let keyword = content.parse::<keyword::slots>()?;
                set_section(&mut slots, parse_slots(&content)?, keyword.span, "slots")?;
            } else if content.peek(keyword::setup) {
                let keyword = content.parse::<keyword::setup>()?;
                set_section(
                    &mut setup,
                    SetupHook::parse_after_keyword(&content)?,
                    keyword.span,
                    "setup",
                )?;
            } else if content.peek(keyword::mounted) {
                let span = content.parse::<keyword::mounted>()?.span;
                parse_rendered_lifecycle(&content, &mut mounted, span, "mounted")?;
            } else if content.peek(keyword::updated) {
                let span = content.parse::<keyword::updated>()?.span;
                parse_rendered_lifecycle(&content, &mut updated, span, "updated")?;
            } else if content.peek(keyword::unmounted) {
                let span = content.parse::<keyword::unmounted>()?.span;
                parse_unmounted_lifecycle(&content, &mut unmounted, span)?;
            } else if content.peek(keyword::template) {
                let keyword = content.parse::<keyword::template>()?;
                set_section(
                    &mut template,
                    TemplateHook::parse_after_keyword(&content)?,
                    keyword.span,
                    "template",
                )?;
            } else {
                return Err(content.error(
                    "expected a props, state, emits, slots, setup, mounted, updated, unmounted, or template component section",
                ));
            }
        }

        let template = template.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "a component requires template(this, window, cx) { ... }",
            )
        })?;

        Ok(Self {
            attributes,
            visibility,
            name,
            properties: properties.unwrap_or_default(),
            state: state.unwrap_or_default(),
            emissions: emissions.unwrap_or_default(),
            slots: slots.unwrap_or_default(),
            setup,
            mounted,
            updated,
            unmounted,
            template,
        })
    }
}

impl ComponentDefinition {
    /// Checks documentation, names, and hook binders before code generation.
    fn validate(&self) -> Result<()> {
        require_documentation(&self.attributes, self.name.span(), "component")?;

        let mut property_names = HashSet::new();
        for property in &self.properties {
            require_documentation(&property.attributes, property.name.span(), "property")?;
            validate_field_name(&property.name)?;
            if unraw_name(&property.name) == "build" {
                return Err(syn::Error::new(
                    property.name.span(),
                    "`build` is reserved for the generated props builder terminal method",
                ));
            }
            if !property_names.insert(property.name.to_string()) {
                return Err(syn::Error::new(
                    property.name.span(),
                    format!("duplicate property `{}`", property.name),
                ));
            }
        }

        let mut state_names = HashSet::new();
        for field in &self.state {
            require_documentation(&field.attributes, field.name.span(), "state field")?;
            validate_field_name(&field.name)?;
            if field.name == "props" {
                return Err(syn::Error::new(
                    field.name.span(),
                    "`props` is reserved for the generated component property field",
                ));
            }
            if !self.slots.is_empty() && field.name == "slots" {
                return Err(syn::Error::new(
                    field.name.span(),
                    "`slots` is reserved for the generated component slot field",
                ));
            }
            if !state_names.insert(field.name.to_string()) {
                return Err(syn::Error::new(
                    field.name.span(),
                    format!("duplicate state field `{}`", field.name),
                ));
            }
        }

        let mut emission_names = HashSet::new();
        for emission in &self.emissions {
            require_documentation(&emission.attributes, emission.name.span(), "event")?;
            validate_field_name(&emission.name)?;
            let emission_name = unraw_name(&emission.name);
            if !emission_names.insert(emission_name) {
                return Err(syn::Error::new(
                    emission.name.span(),
                    format!("duplicate event `{}`", emission.name),
                ));
            }

            let mut payload_names = HashSet::new();
            for payload in &emission.payloads {
                validate_field_name(&payload.name)?;
                let payload_name = unraw_name(&payload.name);
                if !payload_names.insert(payload_name) {
                    return Err(syn::Error::new(
                        payload.name.span(),
                        format!(
                            "duplicate payload `{}` in event `{}`",
                            payload.name, emission.name
                        ),
                    ));
                }
            }
        }

        let mut slot_names = HashSet::new();
        for slot in &self.slots {
            require_documentation(&slot.attributes, slot.name.span(), "slot")?;
            validate_field_name(&slot.name)?;
            let slot_name = unraw_name(&slot.name);
            if !slot_names.insert(slot_name) {
                return Err(syn::Error::new(
                    slot.name.span(),
                    format!("duplicate slot `{}`", slot.name),
                ));
            }
        }

        if let Some(setup) = &self.setup {
            validate_distinct_binders(
                [&setup.this, &setup.props, &setup.context],
                "setup hook binders must have distinct names",
            )?;
        }
        self.validate_lifecycle_binders()?;
        validate_distinct_binders(
            [
                &self.template.this,
                &self.template.window,
                &self.template.context,
            ],
            "template hook binders must have distinct names",
        )
    }

    /// Checks that each lifecycle section gives its inputs distinct bindings.
    fn validate_lifecycle_binders(&self) -> Result<()> {
        if let Some(hook) = &self.mounted {
            validate_distinct_binders(
                [&hook.this, &hook.window, &hook.context],
                "mounted hook binders must have distinct names",
            )?;
        }
        if let Some(hook) = &self.updated {
            validate_distinct_binders(
                [&hook.this, &hook.window, &hook.context],
                "updated hook binders must have distinct names",
            )?;
        }
        if let Some(hook) = &self.unmounted {
            validate_distinct_binders(
                [&hook.this, &hook.context],
                "unmounted hook binders must have distinct names",
            )?;
        }
        Ok(())
    }

    /// Lowers the declaration to props, component, constructor, and render items.
    fn lower(&self) -> Result<TokenStream> {
        let crate_path = runtime_crate_path();
        let name = &self.name;
        let props_name = format_ident!("{name}Props");
        let draft_name = format_ident!("{name}StateDraft");
        let event_name = format_ident!("{name}Event");
        let slots_name = format_ident!("{name}Slots");
        let input_name = format_ident!("{name}Input");
        let props = self.lower_props(&crate_path, &props_name);
        let events = self.lower_events(&crate_path, &event_name);
        let slots = self.lower_slots(&crate_path, &slots_name);
        let input = self.lower_input(
            &props_name,
            &input_name,
            (!self.slots.is_empty()).then_some(&slots_name),
        );
        let component = self.lower_component(
            &crate_path,
            &props_name,
            &input_name,
            &draft_name,
            &event_name,
            (!self.slots.is_empty()).then_some(&slots_name),
        )?;
        Ok(quote!(#props #events #slots #input #component))
    }

    /// Emits the complete host input passed to native component reconciliation.
    fn lower_input(
        &self,
        props_name: &Ident,
        input_name: &Ident,
        slots_name: Option<&Ident>,
    ) -> TokenStream {
        let visibility = &self.visibility;
        let component_name = &self.name;
        let documentation =
            format!("Complete mount and reconciliation input for [`{component_name}`].");
        let slots_field = slots_name.map(|slots_name| quote!(slots: #slots_name,));
        let slots_initializer = slots_name.map(|slots_name| quote!(slots: #slots_name::new(),));
        let slots_setter = slots_name.map(|slots_name| {
            quote! {
                /// Replaces the initially empty typed slot collection.
                #[must_use]
                #visibility fn with_slots(mut self, slots: #slots_name) -> Self {
                    self.slots = slots;
                    self
                }
            }
        });

        quote! {
            #[doc = #documentation]
            #visibility struct #input_name {
                /// Comparable ordinary properties for this render.
                props: #props_name,
                #slots_field
            }

            impl #input_name {
                /// Creates input with the supplied props and empty slots, when declared.
                #[must_use]
                #visibility fn new(props: #props_name) -> Self {
                    Self {
                        props,
                        #slots_initializer
                    }
                }

                #slots_setter
            }
        }
    }

    /// Emits a typed slot collection with empty defaults and fluent providers.
    fn lower_slots(&self, crate_path: &TokenStream, slots_name: &Ident) -> TokenStream {
        if self.slots.is_empty() {
            return quote!();
        }

        let visibility = &self.visibility;
        let component_name = &self.name;
        let fields = self
            .slots
            .iter()
            .map(|slot| slot.field_tokens(crate_path, visibility));
        let initializers = self.slots.iter().map(|slot| {
            let name = &slot.name;
            quote!(#name: #crate_path::Slot::empty())
        });
        let setters = self
            .slots
            .iter()
            .map(|slot| slot.setter_tokens(crate_path, visibility));
        let documentation = format!("Typed slots accepted by [`{component_name}`].");

        quote! {
            #[doc = #documentation]
            #visibility struct #slots_name {
                #(#fields),*
            }

            impl #slots_name {
                /// Creates a collection in which no slot provider is present.
                #[must_use]
                #visibility const fn new() -> Self {
                    Self { #(#initializers),* }
                }

                #(#setters)*
            }

            impl ::core::default::Default for #slots_name {
                fn default() -> Self {
                    Self::new()
                }
            }
        }
    }

    /// Emits a typed event enum and its native GPUI emitter marker implementation.
    fn lower_events(&self, crate_path: &TokenStream, event_name: &Ident) -> TokenStream {
        if self.emissions.is_empty() {
            return quote!();
        }

        let visibility = &self.visibility;
        let component_name = &self.name;
        let variants = self.emissions.iter().map(Emission::variant_tokens);
        let has_multiple_variants = self.emissions.len() > 1;
        let dispatchers = self.emissions.iter().map(|emission| {
            emission.dispatcher_tokens(crate_path, visibility, has_multiple_variants)
        });
        let documentation = format!("Events emitted by [`{component_name}`].");

        quote! {
            #[doc = #documentation]
            #visibility enum #event_name {
                #(#variants),*
            }

            impl #crate_path::gpui::EventEmitter<#event_name> for #component_name {}

            impl #crate_path::NativeComponentEvents for #component_name {
                type Event = #event_name;
            }

            impl #event_name {
                #(#dispatchers)*
            }
        }
    }

    /// Emits the typed props struct, required constructor, default setters, and `Default`.
    fn lower_props(&self, crate_path: &TokenStream, props_name: &Ident) -> TokenStream {
        let visibility = &self.visibility;
        let name = &self.name;
        let property_fields = self.properties.iter().map(Property::field_tokens);
        let required = self
            .properties
            .iter()
            .filter(|property| property.default.is_none())
            .collect::<Vec<_>>();
        let constructor_parameters = required.iter().map(|property| {
            let name = &property.name;
            let ty = &property.ty;
            quote!(#name: #ty)
        });
        let property_initializers = self.properties.iter().map(|property| {
            let name = &property.name;
            if let Some(default) = &property.default {
                quote!(#name: #default)
            } else {
                quote!(#name)
            }
        });
        let default_setters = self
            .properties
            .iter()
            .filter(|property| property.default.is_some())
            .map(|property| property.setter_tokens(visibility));
        let props_documentation = format!(
            "Typed construction properties for [`{name}`]. Every field type must implement `PartialEq` so persistent hosts can suppress unchanged notifications."
        );
        let default_impl = self.default_props_impl(props_name, required.is_empty());
        let builder_name = format_ident!("{props_name}Builder");
        let (builder_factory, builder) =
            self.lower_props_builder(crate_path, props_name, &builder_name, &required);

        quote! {
            #[doc = #props_documentation]
            #[derive(::core::cmp::PartialEq)]
            #visibility struct #props_name {
                #(#property_fields),*
            }

            impl #props_name {
                /// Creates properties, requiring every field without a declared default.
                #[must_use]
                #visibility fn new(#(#constructor_parameters),*) -> Self {
                    Self { #(#property_initializers),* }
                }

                #(#default_setters)*

                #builder_factory
            }

            #default_impl
            #builder
        }
    }

    /// Emits a typestate builder factory and its generated builder type.
    fn lower_props_builder(
        &self,
        crate_path: &TokenStream,
        props_name: &Ident,
        builder_name: &Ident,
        required: &[&Property],
    ) -> (TokenStream, TokenStream) {
        let visibility = &self.visibility;
        let state_parameters = required
            .iter()
            .enumerate()
            .map(|(index, _)| format_ident!("RequiredState{index}"))
            .collect::<Vec<_>>();
        let declaration_generics = generic_arguments(&state_parameters);
        let missing_states = vec![quote!(#crate_path::PropMissing); required.len()];
        let missing_generics = generic_arguments(&missing_states);
        let set_states = vec![quote!(#crate_path::PropSet); required.len()];
        let set_generics = generic_arguments(&set_states);
        let factory = self.props_builder_factory(builder_name, &missing_generics, crate_path);
        let fields = self.properties.iter().map(|property| {
            let state = required
                .iter()
                .position(|required_property| required_property.name == property.name)
                .map(|index| &state_parameters[index]);
            property.builder_field_tokens(crate_path, state)
        });
        let setters = self.properties.iter().map(|property| {
            let required_index = required
                .iter()
                .position(|required_property| required_property.name == property.name);
            self.props_builder_setter(
                property,
                required_index,
                builder_name,
                &state_parameters,
                crate_path,
            )
        });
        let build = self.props_builder_build(props_name, builder_name, &set_generics);
        let documentation = format!("Typestate builder for [`{props_name}`].");

        let builder = quote! {
            #[doc = #documentation]
            #visibility struct #builder_name #declaration_generics {
                #(#fields,)*
            }

            #(#setters)*
            #build
        };
        (factory, builder)
    }

    /// Emits `Props::builder` with absent required values and declared defaults.
    fn props_builder_factory(
        &self,
        builder_name: &Ident,
        missing_generics: &TokenStream,
        crate_path: &TokenStream,
    ) -> TokenStream {
        let visibility = &self.visibility;
        let initializers = self.properties.iter().map(|property| {
            let name = &property.name;
            property.default.as_ref().map_or_else(
                || quote!(#name: #crate_path::RequiredProp::missing()),
                |default| quote!(#name: #default),
            )
        });

        quote! {
            /// Starts a typestate builder with declared defaults and missing required properties.
            #[must_use]
            #visibility fn builder() -> #builder_name #missing_generics {
                #builder_name {
                    #(#initializers,)*
                }
            }
        }
    }

    /// Emits one exact-typed consuming property setter on the builder.
    fn props_builder_setter(
        &self,
        property: &Property,
        required_index: Option<usize>,
        builder_name: &Ident,
        state_parameters: &[Ident],
        crate_path: &TokenStream,
    ) -> TokenStream {
        let visibility = &self.visibility;
        let name = &property.name;
        let ty = &property.ty;
        let declaration_generics = generic_arguments(state_parameters);

        let Some(required_index) = required_index else {
            let documentation = format!(
                "Overrides the declared default for the `{}` property.",
                unraw_name(name)
            );
            return quote! {
                impl #declaration_generics #builder_name #declaration_generics {
                    #[doc = #documentation]
                    #[must_use]
                    #visibility fn #name(mut self, value: #ty) -> Self {
                        self.#name = value;
                        self
                    }
                }
            };
        };

        let return_states = state_parameters
            .iter()
            .enumerate()
            .map(|(index, state)| {
                if index == required_index {
                    quote!(#crate_path::PropSet)
                } else {
                    quote!(#state)
                }
            })
            .collect::<Vec<_>>();
        let return_generics = generic_arguments(&return_states);
        let initializers = self.properties.iter().map(|candidate| {
            let candidate_name = &candidate.name;
            if candidate.name == property.name {
                quote!(#candidate_name: self.#candidate_name.set(value))
            } else {
                quote!(#candidate_name: self.#candidate_name)
            }
        });
        let documentation = format!(
            "Sets the required `{}` property; repeated calls replace the previous value.",
            unraw_name(name)
        );

        quote! {
            impl #declaration_generics #builder_name #declaration_generics {
                #[doc = #documentation]
                #[must_use]
                #visibility fn #name(
                    self,
                    value: #ty,
                ) -> #builder_name #return_generics {
                    #builder_name {
                        #(#initializers,)*
                    }
                }
            }
        }
    }

    /// Emits `build` only for the all-required-properties-set state.
    fn props_builder_build(
        &self,
        props_name: &Ident,
        builder_name: &Ident,
        set_generics: &TokenStream,
    ) -> TokenStream {
        let visibility = &self.visibility;
        let field_names = self
            .properties
            .iter()
            .map(|property| &property.name)
            .collect::<Vec<_>>();
        let required_extractions = self
            .properties
            .iter()
            .filter(|property| property.default.is_none())
            .map(|property| {
                let name = &property.name;
                quote!(let #name = #name.into_value();)
            });
        let documentation = format!(
            "Builds [`{props_name}`] after every required property has reached `PropSet`. The runtime's sealed required-property storage guarantees every extraction contains a value."
        );

        quote! {
            impl #builder_name #set_generics {
                #[doc = #documentation]
                #[must_use]
                #visibility fn build(self) -> #props_name {
                    let #builder_name {
                        #(#field_names,)*
                    } = self;
                    #(#required_extractions)*
                    #props_name { #(#field_names),* }
                }
            }
        }
    }

    /// Emits `Default` only when every declared property has a default expression.
    fn default_props_impl(&self, props_name: &Ident, all_defaulted: bool) -> TokenStream {
        if !all_defaulted {
            return quote!();
        }
        let property_initializers = self.properties.iter().map(|property| {
            let name = &property.name;
            let default = property
                .default
                .as_ref()
                .expect("all properties were checked to have defaults");
            quote!(#name: #default)
        });
        quote! {
            impl ::core::default::Default for #props_name {
                fn default() -> Self {
                    Self { #(#property_initializers),* }
                }
            }
        }
    }

    /// Emits the component struct, entity constructor, property accessor, and render trait.
    fn lower_component(
        &self,
        crate_path: &TokenStream,
        props_name: &Ident,
        input_name: &Ident,
        draft_name: &Ident,
        event_name: &Ident,
        slots_name: Option<&Ident>,
    ) -> Result<TokenStream> {
        let attributes = &self.attributes;
        let visibility = &self.visibility;
        let name = &self.name;
        let state_fields = self.state.iter().map(StateField::field_tokens);
        let constructor = self.lower_constructor(crate_path, props_name, input_name, slots_name);
        let native_component =
            self.lower_native_component(crate_path, props_name, input_name, draft_name, slots_name);
        let emit_helpers = self
            .emissions
            .iter()
            .map(|emission| emission.helper_tokens(crate_path, event_name, visibility));
        let render = self.lower_render(crate_path)?;
        let lifecycle = self.lower_lifecycle(crate_path);
        let slots_field = slots_name.map(|slots_name| {
            quote! {
                /// Lazy typed slots supplied when this entity was constructed.
                slots: #slots_name,
            }
        });
        let slots_accessor = slots_name.map(|slots_name| {
            quote! {
                /// Returns the component's current reconciled typed slots.
                #[must_use]
                #visibility const fn slots(&self) -> &#slots_name {
                    &self.slots
                }
            }
        });

        Ok(quote! {
            #(#attributes)*
            #visibility struct #name {
                /// Current comparable properties supplied by the parent host.
                props: #props_name,
                #slots_field
                #(#state_fields),*
            }

            impl #name {
                #constructor

                #(#emit_helpers)*

                #slots_accessor

                /// Returns the component's current reconciled properties.
                #[must_use]
                #visibility const fn props(&self) -> &#props_name {
                    &self.props
                }
            }

            #render
            #native_component
            #lifecycle
        })
    }

    /// Emits statically dispatched lifecycle hook bodies when any are present.
    fn lower_lifecycle(&self, crate_path: &TokenStream) -> TokenStream {
        if self.mounted.is_none() && self.updated.is_none() && self.unmounted.is_none() {
            return quote!();
        }

        let name = &self.name;
        let has_mounted = self.mounted.is_some();
        let has_updated = self.updated.is_some();
        let has_unmounted = self.unmounted.is_some();
        let mounted = self.mounted.as_ref().map(|hook| {
            let this = &hook.this;
            let window = &hook.window;
            let context = &hook.context;
            let body = &hook.body;
            quote! {
                fn mounted(
                    gpui_vue_internal_component: &mut Self,
                    gpui_vue_internal_window: &mut #crate_path::gpui::Window,
                    gpui_vue_internal_cx: &mut #crate_path::gpui::Context<Self>,
                ) {
                    let #this = gpui_vue_internal_component;
                    let #window = gpui_vue_internal_window;
                    let #context = gpui_vue_internal_cx;
                    let (): () = #body;
                }
            }
        });
        let updated = self.updated.as_ref().map(|hook| {
            let this = &hook.this;
            let window = &hook.window;
            let context = &hook.context;
            let body = &hook.body;
            quote! {
                fn updated(
                    gpui_vue_internal_component: &mut Self,
                    gpui_vue_internal_window: &mut #crate_path::gpui::Window,
                    gpui_vue_internal_cx: &mut #crate_path::gpui::Context<Self>,
                ) {
                    let #this = gpui_vue_internal_component;
                    let #window = gpui_vue_internal_window;
                    let #context = gpui_vue_internal_cx;
                    let (): () = #body;
                }
            }
        });
        let unmounted = self.unmounted.as_ref().map(|hook| {
            let this = &hook.this;
            let context = &hook.context;
            let body = &hook.body;
            quote! {
                fn unmounted(
                    gpui_vue_internal_component: &mut Self,
                    gpui_vue_internal_cx: &mut #crate_path::gpui::App,
                ) {
                    let #this = gpui_vue_internal_component;
                    let #context = gpui_vue_internal_cx;
                    let (): () = #body;
                }
            }
        });

        quote! {
            impl #crate_path::ComponentLifecycleHooks for #name {
                const HAS_MOUNTED: bool = #has_mounted;
                const TRACK_UPDATES: bool = #has_updated;
                const HAS_UNMOUNTED: bool = #has_unmounted;

                #mounted
                #updated
                #unmounted
            }
        }
    }

    /// Emits generic `AppContext` constructors around native component input.
    fn lower_constructor(
        &self,
        crate_path: &TokenStream,
        props_name: &Ident,
        input_name: &Ident,
        slots_name: Option<&Ident>,
    ) -> TokenStream {
        let visibility = &self.visibility;
        let name = &self.name;
        let constructor_documentation = format!(
            "Creates a `{name}` entity and runs its setup hook exactly once inside `AppContext::new`."
        );
        let construct = |input: TokenStream| {
            quote! {
                #crate_path::gpui::AppContext::new(cx, move |gpui_vue_internal_cx| {
                    <Self as #crate_path::NativeComponent>::construct(
                        #input,
                        gpui_vue_internal_cx,
                    )
                })
            }
        };

        if let Some(slots_name) = slots_name {
            let default_construction = construct(quote!(#input_name::new(props)));
            let slots_construction = construct(quote!(#input_name::new(props).with_slots(slots)));
            let slots_documentation = format!(
                "Creates a `{name}` entity with explicit typed slots and runs setup exactly once."
            );
            return quote! {
                #[doc = #constructor_documentation]
                #visibility fn new<ContextType>(
                    props: #props_name,
                    cx: &mut ContextType,
                ) -> <ContextType as #crate_path::gpui::AppContext>::Result<
                    #crate_path::gpui::Entity<Self>,
                >
                where
                    ContextType: #crate_path::gpui::AppContext,
                {
                    #default_construction
                }

                #[doc = #slots_documentation]
                #visibility fn new_with_slots<ContextType>(
                    props: #props_name,
                    slots: #slots_name,
                    cx: &mut ContextType,
                ) -> <ContextType as #crate_path::gpui::AppContext>::Result<
                    #crate_path::gpui::Entity<Self>,
                >
                where
                    ContextType: #crate_path::gpui::AppContext,
                {
                    #slots_construction
                }
            };
        }

        let construction = construct(quote!(#input_name::new(props)));
        quote! {
            #[doc = #constructor_documentation]
            #visibility fn new<ContextType>(
                props: #props_name,
                cx: &mut ContextType,
            ) -> <ContextType as #crate_path::gpui::AppContext>::Result<
                #crate_path::gpui::Entity<Self>,
            >
            where
                ContextType: #crate_path::gpui::AppContext,
            {
                #construction
            }
        }
    }

    /// Emits the associated typed-slot input adapter for a slotted component.
    fn lower_native_slots(
        &self,
        crate_path: &TokenStream,
        input_name: &Ident,
        slots_name: &Ident,
    ) -> TokenStream {
        let name = &self.name;
        quote! {
            impl #crate_path::NativeComponentSlots for #name {
                type Slots = #slots_name;

                fn slots(&self) -> &Self::Slots {
                    &self.slots
                }

                fn input_with_slots(
                    props: Self::Props,
                    slots: Self::Slots,
                ) -> Self::Input {
                    #input_name::new(props).with_slots(slots)
                }
            }
        }
    }

    /// Emits persistent construction and per-frame input reconciliation.
    fn lower_native_component(
        &self,
        crate_path: &TokenStream,
        props_name: &Ident,
        input_name: &Ident,
        draft_name: &Ident,
        slots_name: Option<&Ident>,
    ) -> TokenStream {
        let name = &self.name;
        let draft_fields = self.state.iter().map(|field| {
            let name = &field.name;
            let ty = &field.ty;
            quote! {
                #[doc = "Typed state held while the one-shot setup hook runs."]
                #name: #ty
            }
        });
        let draft_initializers = self.state.iter().map(|field| {
            let name = &field.name;
            let initializer = &field.initializer;
            quote! {
                #name: {
                    let props = &gpui_vue_internal_props;
                    let cx = &mut *gpui_vue_internal_cx;
                    #initializer
                }
            }
        });
        let state_names = self
            .state
            .iter()
            .map(|field| &field.name)
            .collect::<Vec<_>>();
        let setup = self.setup_tokens();
        let state_binding = if self.setup.is_some() {
            quote!(let mut gpui_vue_internal_state)
        } else {
            quote!(let gpui_vue_internal_state)
        };
        let input_destructure = if slots_name.is_some() {
            quote! {
                let #input_name {
                    props: gpui_vue_internal_props,
                    slots: gpui_vue_internal_slots,
                } = input;
            }
        } else {
            quote! {
                let #input_name {
                    props: gpui_vue_internal_props,
                } = input;
            }
        };
        let slots_initializer = slots_name.map(|_| quote!(slots: gpui_vue_internal_slots,));
        let reconcile_slots = slots_name.map(|_| {
            quote! {
                self.slots = gpui_vue_internal_slots;
            }
        });
        let native_slots = slots_name
            .map(|slots_name| self.lower_native_slots(crate_path, input_name, slots_name));
        let mount_state = self.lifecycle_mount_state(crate_path);
        let host_input_changed = Self::host_input_changed(slots_name);

        quote! {
            impl #crate_path::NativeComponent for #name {
                type Props = #props_name;
                type Input = #input_name;
                type MountState = #mount_state;

                fn construct(
                    input: Self::Input,
                    gpui_vue_internal_cx: &mut #crate_path::gpui::Context<Self>,
                ) -> Self {
                    #input_destructure

                    #[doc = "Typed state draft used only during component construction."]
                    struct #draft_name {
                        #(#draft_fields),*
                    }

                    #state_binding = #draft_name {
                        #(#draft_initializers),*
                    };
                    #setup
                    let #draft_name { #(#state_names),* } = gpui_vue_internal_state;

                    Self {
                        props: gpui_vue_internal_props,
                        #slots_initializer
                        #(#state_names),*
                    }
                }

                fn reconcile_input(
                    &mut self,
                    input: Self::Input,
                    gpui_vue_internal_cx: &mut #crate_path::gpui::Context<Self>,
                ) -> bool {
                    #input_destructure
                    let gpui_vue_internal_props_changed =
                        self.props != gpui_vue_internal_props;
                    self.props = gpui_vue_internal_props;
                    #reconcile_slots
                    if gpui_vue_internal_props_changed {
                        gpui_vue_internal_cx.notify();
                    }
                    #host_input_changed
                }
            }

            #native_slots
        }
    }

    /// Selects unit storage or the hook-bearing visual lifecycle mount.
    fn lifecycle_mount_state(&self, crate_path: &TokenStream) -> TokenStream {
        if self.mounted.is_some() || self.updated.is_some() || self.unmounted.is_some() {
            quote!(#crate_path::ComponentLifecycleMount<Self>)
        } else {
            quote!(())
        }
    }

    /// Conservatively treats every opaque slot-provider replacement as dirty.
    fn host_input_changed(slots_name: Option<&Ident>) -> TokenStream {
        if slots_name.is_some() {
            quote!(true)
        } else {
            quote!(gpui_vue_internal_props_changed)
        }
    }

    /// Emits the optional setup block with typed state, props, and context bindings.
    fn setup_tokens(&self) -> TokenStream {
        self.setup.as_ref().map_or_else(
            || quote!(),
            |hook| {
                let this = &hook.this;
                let props = &hook.props;
                let context = &hook.context;
                let body = &hook.body;
                quote! {
                    {
                        let #this = &mut gpui_vue_internal_state;
                        let #props = &gpui_vue_internal_props;
                        let #context = &mut *gpui_vue_internal_cx;
                        let (): () = #body;
                    }
                }
            },
        )
    }

    /// Emits the native GPUI `Render` implementation for the template body.
    fn lower_render(&self, crate_path: &TokenStream) -> Result<TokenStream> {
        let name = &self.name;
        let template_this = &self.template.this;
        let template_window = &self.template.window;
        let template_context = &self.template.context;
        match &self.template.body {
            TemplateBody::Rust(body) => Ok(quote! {
                impl #crate_path::gpui::Render for #name {
                    fn render(
                        &mut self,
                        #template_window: &mut #crate_path::gpui::Window,
                        #template_context: &mut #crate_path::gpui::Context<Self>,
                    ) -> impl #crate_path::gpui::IntoElement {
                        let #template_this = self;
                        #body
                    }
                }
            }),
            TemplateBody::Markup(markup) => {
                let slots = self
                    .slots
                    .iter()
                    .map(|slot| {
                        crate::view::ComponentSlotMetadata::new(
                            slot.name.clone(),
                            slot.props.clone(),
                        )
                    })
                    .collect();
                let context = crate::view::ComponentTemplateContext::new(
                    template_this.clone(),
                    template_window.clone(),
                    template_context.clone(),
                    slots,
                );
                let template_body = crate::view::expand_component_template(markup, &context)?;
                let internal_window = format_ident!(
                    "gpui_vue_internal_template_window",
                    span = Span::mixed_site()
                );
                let internal_context = format_ident!(
                    "gpui_vue_internal_template_context",
                    span = Span::mixed_site()
                );
                let this_lint = implicit_template_binding_lint();
                let window_lint = implicit_template_binding_lint();
                let context_lint = implicit_template_binding_lint();

                Ok(quote! {
                    impl #crate_path::gpui::Render for #name {
                        fn render(
                            &mut self,
                            #internal_window: &mut #crate_path::gpui::Window,
                            #internal_context: &mut #crate_path::gpui::Context<Self>,
                        ) -> impl #crate_path::gpui::IntoElement {
                            // Keep lint exemptions on only the three implicit aliases;
                            // all user-authored patterns in the template retain their
                            // surrounding crate's normal lint levels.
                            #this_lint
                            let #template_this = self;
                            #window_lint
                            let #template_window = &mut *#internal_window;
                            #context_lint
                            let #template_context = &mut *#internal_context;
                            #template_body
                        }
                    }
                })
            }
        }
    }
}

/// Emits a narrow lint exemption for one implicit direct-template alias.
fn implicit_template_binding_lint() -> TokenStream {
    quote_spanned!(Span::mixed_site()=>
        #[allow(
            unused_variables,
            clippy::no_effect_underscore_binding,
            clippy::used_underscore_binding,
        )]
    )
}

/// A construction property, with absence of a default meaning required.
struct Property {
    /// Attributes copied to the generated props field.
    attributes: Vec<Attribute>,
    /// Visibility of the generated props field.
    visibility: Visibility,
    /// Property name.
    name: Ident,
    /// Property type.
    ty: Type,
    /// Optional default expression.
    default: Option<Expr>,
}

impl Property {
    /// Emits this property as a field in the generated props type.
    fn field_tokens(&self) -> TokenStream {
        let attributes = &self.attributes;
        let visibility = &self.visibility;
        let name = &self.name;
        let ty = &self.ty;
        quote!(#(#attributes)* #visibility #name: #ty)
    }

    /// Emits private inline storage for this property in a typestate builder.
    fn builder_field_tokens(&self, crate_path: &TokenStream, state: Option<&Ident>) -> TokenStream {
        let name = &self.name;
        let ty = &self.ty;
        let documentation = format!(
            "Inline builder storage for the `{}` property.",
            unraw_name(name)
        );
        if let Some(state) = state {
            quote!(#[doc = #documentation] #name: #crate_path::RequiredProp<#ty, #state>)
        } else {
            quote!(#[doc = #documentation] #name: #ty)
        }
    }

    /// Emits a fluent override method for a property that has a default.
    fn setter_tokens(&self, visibility: &Visibility) -> TokenStream {
        let name = &self.name;
        let ty = &self.ty;
        let setter = format_ident!("with_{}", name.unraw(), span = name.span());
        let documentation = format!("Overrides the default value of [`Self::{name}`].");
        quote! {
            #[doc = #documentation]
            #[must_use]
            #visibility fn #setter(mut self, value: #ty) -> Self {
                self.#name = value;
                self
            }
        }
    }
}

/// A state field initialized once for each entity construction.
struct StateField {
    /// Attributes copied to the generated component field.
    attributes: Vec<Attribute>,
    /// Visibility of the generated component field.
    visibility: Visibility,
    /// State field name.
    name: Ident,
    /// State field type.
    ty: Type,
    /// Typed initializer evaluated inside the entity constructor.
    initializer: Expr,
}

impl StateField {
    /// Emits this state field in the generated component type.
    fn field_tokens(&self) -> TokenStream {
        let attributes = &self.attributes;
        let visibility = &self.visibility;
        let name = &self.name;
        let ty = &self.ty;
        quote!(#(#attributes)* #visibility #name: #ty)
    }
}

/// One typed event declaration lowered to a variant and an emission helper.
struct Emission {
    /// Attributes copied to the generated event variant.
    attributes: Vec<Attribute>,
    /// Snake-case event name used to derive the variant and helper names.
    name: Ident,
    /// Typed payload fields carried by this event.
    payloads: Vec<EventPayload>,
}

impl Emission {
    /// Emits this event as a unit or named-field enum variant.
    fn variant_tokens(&self) -> TokenStream {
        let attributes = &self.attributes;
        let variant = event_variant_ident(&self.name);
        if self.payloads.is_empty() {
            return quote!(#(#attributes)* #variant);
        }

        let payloads = self.payloads.iter().map(EventPayload::field_tokens);
        quote! {
            #(#attributes)*
            #variant {
                #(#payloads),*
            }
        }
    }

    /// Emits a hidden typed dispatcher used by `PascalCase` template listeners.
    fn dispatcher_tokens(
        &self,
        crate_path: &TokenStream,
        visibility: &Visibility,
        has_multiple_variants: bool,
    ) -> TokenStream {
        let event = unraw_name(&self.name);
        let dispatcher = format_ident!("__gpui_vue_dispatch_{event}", span = self.name.span());
        let variant = event_variant_ident(&self.name);
        let matching_pattern = if self.payloads.is_empty() {
            quote!(Self::#variant)
        } else {
            quote!(Self::#variant { .. })
        };
        let dispatch = if has_multiple_variants {
            quote! {
                match self {
                    #matching_pattern => handler(self, window, cx),
                    _ => {}
                }
            }
        } else {
            quote!(handler(self, window, cx);)
        };

        quote! {
            #[doc(hidden)]
            #visibility fn #dispatcher<Handler>(
                &self,
                handler: &mut Handler,
                window: &mut #crate_path::gpui::Window,
                cx: &mut #crate_path::gpui::App,
            )
            where
                Handler: ::core::ops::FnMut(
                    &Self,
                    &mut #crate_path::gpui::Window,
                    &mut #crate_path::gpui::App,
                ),
            {
                #dispatch
            }
        }
    }

    /// Emits the zero-cost helper that forwards directly to `Context::emit`.
    fn helper_tokens(
        &self,
        crate_path: &TokenStream,
        event_name: &Ident,
        visibility: &Visibility,
    ) -> TokenStream {
        let event = unraw_name(&self.name);
        let helper = format_ident!("emit_{event}", span = self.name.span());
        let variant = event_variant_ident(&self.name);
        let parameters = self.payloads.iter().map(|payload| {
            let name = &payload.name;
            let ty = &payload.ty;
            quote!(#name: #ty)
        });
        let payload_names = self
            .payloads
            .iter()
            .map(|payload| &payload.name)
            .collect::<Vec<_>>();
        let event_value = if payload_names.is_empty() {
            quote!(#event_name::#variant)
        } else {
            quote!(#event_name::#variant { #(#payload_names),* })
        };
        let documentation = format!(
            "Emits [`{event_name}::{variant}`] through this component's native GPUI event channel."
        );

        quote! {
            #[doc = #documentation]
            #visibility fn #helper(
                #(#parameters,)*
                gpui_vue_internal_cx: &mut #crate_path::gpui::Context<Self>,
            ) {
                gpui_vue_internal_cx.emit(#event_value);
            }
        }
    }
}

/// One named field in an emitted event's payload.
struct EventPayload {
    /// Optional user attributes copied to the generated enum field.
    attributes: Vec<Attribute>,
    /// Payload field name.
    name: Ident,
    /// Payload field type.
    ty: Type,
}

impl EventPayload {
    /// Emits a documented named enum field.
    fn field_tokens(&self) -> TokenStream {
        let attributes = &self.attributes;
        let name = &self.name;
        let ty = &self.ty;
        let generated_documentation = if has_documentation(attributes) {
            quote!()
        } else {
            let documentation = format!("The `{}` event payload.", unraw_name(name));
            quote!(#[doc = #documentation])
        };
        quote!(#generated_documentation #(#attributes)* #name: #ty)
    }
}

/// One named lazy slot and its statically checked props type.
struct SlotDeclaration {
    /// Attributes copied to the generated slot field.
    attributes: Vec<Attribute>,
    /// Slot name used for the field and fluent provider method.
    name: Ident,
    /// Props passed by the receiving component whenever it invokes the slot.
    props: Type,
}

impl SlotDeclaration {
    /// Emits a documented slot field with the component's visibility.
    fn field_tokens(&self, crate_path: &TokenStream, visibility: &Visibility) -> TokenStream {
        let attributes = &self.attributes;
        let name = &self.name;
        let props = &self.props;
        quote!(#(#attributes)* #visibility #name: #crate_path::Slot<#props>)
    }

    /// Emits a fluent setter for one already type-erased slot provider.
    fn setter_tokens(&self, crate_path: &TokenStream, visibility: &Visibility) -> TokenStream {
        let name = &self.name;
        let props = &self.props;
        let setter = format_ident!("with_{}", name.unraw(), span = name.span());
        let documentation = format!("Supplies the [`Self::{name}`] slot provider.");
        quote! {
            #[doc = #documentation]
            #[must_use]
            #visibility fn #setter(mut self, slot: #crate_path::Slot<#props>) -> Self {
                self.#name = slot;
                self
            }
        }
    }
}

/// The optional one-shot setup hook and its user-selected bindings.
struct SetupHook {
    /// Mutable typed state-draft binding.
    this: Ident,
    /// Shared props binding.
    props: Ident,
    /// Mutable GPUI entity context binding.
    context: Ident,
    /// Hook body, required to evaluate to unit.
    body: Block,
}

impl SetupHook {
    /// Parses binders and a block after the `setup` keyword.
    fn parse_after_keyword(input: ParseStream<'_>) -> Result<Self> {
        let bindings;
        parenthesized!(bindings in input);
        let this = Ident::parse_any(&bindings)?;
        bindings.parse::<Token![,]>()?;
        let props = Ident::parse_any(&bindings)?;
        bindings.parse::<Token![,]>()?;
        let context = Ident::parse_any(&bindings)?;
        if !bindings.is_empty() {
            return Err(bindings.error("setup accepts exactly three bindings"));
        }
        let body = input.parse()?;
        Ok(Self {
            this,
            props,
            context,
            body,
        })
    }
}

/// A lifecycle hook that runs after delegated rendering and receives a window.
struct RenderedLifecycleHook {
    /// Mutable component binding.
    this: Ident,
    /// Mutable GPUI window binding.
    window: Ident,
    /// Mutable GPUI entity-context binding.
    context: Ident,
    /// Hook body, required to evaluate to unit.
    body: Block,
}

impl RenderedLifecycleHook {
    /// Parses three binders and a block after `mounted` or `updated`.
    fn parse_after_keyword(input: ParseStream<'_>, hook_name: &str) -> Result<Self> {
        let bindings;
        parenthesized!(bindings in input);
        let this = Ident::parse_any(&bindings)?;
        bindings.parse::<Token![,]>()?;
        let window = Ident::parse_any(&bindings)?;
        bindings.parse::<Token![,]>()?;
        let context = Ident::parse_any(&bindings)?;
        if !bindings.is_empty() {
            return Err(bindings.error(format!("{hook_name} accepts exactly three bindings")));
        }
        let body = input.parse()?;
        Ok(Self {
            this,
            window,
            context,
            body,
        })
    }
}

/// A visual teardown hook that runs without a window context.
struct UnmountedLifecycleHook {
    /// Mutable component binding.
    this: Ident,
    /// Mutable application-context binding.
    context: Ident,
    /// Hook body, required to evaluate to unit.
    body: Block,
}

impl UnmountedLifecycleHook {
    /// Parses two binders and a block after `unmounted`.
    fn parse_after_keyword(input: ParseStream<'_>) -> Result<Self> {
        let bindings;
        parenthesized!(bindings in input);
        let this = Ident::parse_any(&bindings)?;
        bindings.parse::<Token![,]>()?;
        let context = Ident::parse_any(&bindings)?;
        if !bindings.is_empty() {
            return Err(bindings.error("unmounted accepts exactly two bindings"));
        }
        let body = input.parse()?;
        Ok(Self {
            this,
            context,
            body,
        })
    }
}

/// The required render template hook and its user-selected bindings.
struct TemplateHook {
    /// Mutable component binding.
    this: Ident,
    /// Mutable GPUI window binding.
    window: Ident,
    /// Mutable GPUI entity context binding.
    context: Ident,
    /// Rust render block or component-aware direct markup.
    body: TemplateBody,
}

/// One of the two component-template authoring forms.
enum TemplateBody {
    /// An ordinary Rust block retained for backwards compatibility.
    Rust(Block),
    /// Vue-shaped markup lowered with component and slot metadata in scope.
    Markup(TokenStream),
}

impl TemplateHook {
    /// Parses binders and a block after the `template` keyword.
    fn parse_after_keyword(input: ParseStream<'_>) -> Result<Self> {
        let bindings;
        parenthesized!(bindings in input);
        let this = Ident::parse_any(&bindings)?;
        bindings.parse::<Token![,]>()?;
        let window = Ident::parse_any(&bindings)?;
        bindings.parse::<Token![,]>()?;
        let context = Ident::parse_any(&bindings)?;
        if !bindings.is_empty() {
            return Err(bindings.error("template accepts exactly three bindings"));
        }
        let body_content;
        braced!(body_content in input);
        let body_tokens = body_content.parse::<TokenStream>()?;
        let starts_with_markup = matches!(
            body_tokens.clone().into_iter().next(),
            Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == '<'
        );
        let body = if starts_with_markup {
            match crate::view::validate_template(&body_tokens) {
                Ok(()) => TemplateBody::Markup(body_tokens),
                Err(mut markup_error) => match syn::parse2::<Block>(quote!({ #body_tokens })) {
                    Ok(block) => TemplateBody::Rust(block),
                    Err(rust_error) => {
                        markup_error.combine(rust_error);
                        return Err(markup_error);
                    }
                },
            }
        } else {
            TemplateBody::Rust(syn::parse2(quote!({ #body_tokens }))?)
        };
        Ok(Self {
            this,
            window,
            context,
            body,
        })
    }
}

/// Parses a comma-separated property section.
fn parse_properties(input: ParseStream<'_>) -> Result<Vec<Property>> {
    let content;
    braced!(content in input);
    let mut properties = Vec::new();
    while !content.is_empty() {
        let attributes = content.call(Attribute::parse_outer)?;
        let visibility = content.parse()?;
        let name = Ident::parse_any(&content)?;
        content.parse::<Token![:]>()?;
        let ty = content.parse()?;
        let default = if content.peek(Token![=]) {
            content.parse::<Token![=]>()?;
            Some(content.parse()?)
        } else {
            None
        };
        properties.push(Property {
            attributes,
            visibility,
            name,
            ty,
            default,
        });
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    Ok(properties)
}

/// Parses a comma-separated state section with mandatory initializers.
fn parse_state(input: ParseStream<'_>) -> Result<Vec<StateField>> {
    let content;
    braced!(content in input);
    let mut state = Vec::new();
    while !content.is_empty() {
        let attributes = content.call(Attribute::parse_outer)?;
        let visibility = content.parse()?;
        let name = Ident::parse_any(&content)?;
        content.parse::<Token![:]>()?;
        let ty = content.parse()?;
        if !content.peek(Token![=]) {
            return Err(syn::Error::new(
                name.span(),
                "a state field requires a typed initializer: name: Type = expression",
            ));
        }
        content.parse::<Token![=]>()?;
        let initializer = content.parse()?;
        state.push(StateField {
            attributes,
            visibility,
            name,
            ty,
            initializer,
        });
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    Ok(state)
}

/// Parses semicolon-terminated typed event declarations.
fn parse_emissions(input: ParseStream<'_>) -> Result<Vec<Emission>> {
    let content;
    braced!(content in input);
    let mut emissions = Vec::new();
    while !content.is_empty() {
        let attributes = content.call(Attribute::parse_outer)?;
        let name = Ident::parse_any(&content)?;
        let parameters;
        parenthesized!(parameters in content);
        let mut payloads = Vec::new();
        while !parameters.is_empty() {
            let attributes = parameters.call(Attribute::parse_outer)?;
            let name = Ident::parse_any(&parameters)?;
            parameters.parse::<Token![:]>()?;
            let ty = parameters.parse()?;
            payloads.push(EventPayload {
                attributes,
                name,
                ty,
            });
            if parameters.is_empty() {
                break;
            }
            parameters.parse::<Token![,]>()?;
        }
        content.parse::<Token![;]>()?;
        emissions.push(Emission {
            attributes,
            name,
            payloads,
        });
    }
    Ok(emissions)
}

/// Parses semicolon-terminated typed slot declarations.
fn parse_slots(input: ParseStream<'_>) -> Result<Vec<SlotDeclaration>> {
    let content;
    braced!(content in input);
    let mut slots = Vec::new();
    while !content.is_empty() {
        let attributes = content.call(Attribute::parse_outer)?;
        let name = Ident::parse_any(&content)?;
        content.parse::<Token![:]>()?;
        let props = content.parse()?;
        content.parse::<Token![;]>()?;
        slots.push(SlotDeclaration {
            attributes,
            name,
            props,
        });
    }
    Ok(slots)
}

/// Parses and installs one `mounted` or `updated` lifecycle section.
fn parse_rendered_lifecycle(
    input: ParseStream<'_>,
    destination: &mut Option<RenderedLifecycleHook>,
    span: Span,
    name: &str,
) -> Result<()> {
    let hook = RenderedLifecycleHook::parse_after_keyword(input, name)?;
    set_section(destination, hook, span, name)
}

/// Parses and installs one `unmounted` lifecycle section.
fn parse_unmounted_lifecycle(
    input: ParseStream<'_>,
    destination: &mut Option<UnmountedLifecycleHook>,
    span: Span,
) -> Result<()> {
    let hook = UnmountedLifecycleHook::parse_after_keyword(input)?;
    set_section(destination, hook, span, "unmounted")
}

/// Installs a uniquely occurring component section.
fn set_section<T>(destination: &mut Option<T>, section: T, span: Span, name: &str) -> Result<()> {
    if destination.is_some() {
        return Err(syn::Error::new(span, format!("duplicate {name} section")));
    }
    *destination = Some(section);
    Ok(())
}

/// Requires a Rust doc attribute on a user-authored declaration.
fn require_documentation(attributes: &[Attribute], span: Span, kind: &str) -> Result<()> {
    if has_documentation(attributes) {
        Ok(())
    } else {
        Err(syn::Error::new(
            span,
            format!("every {kind} declaration requires a `///` documentation comment"),
        ))
    }
}

/// Reports whether attributes contain at least one Rust documentation attribute.
fn has_documentation(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.path().is_ident("doc"))
}

/// Wraps a non-empty sequence in Rust generic argument brackets.
fn generic_arguments<Item>(items: &[Item]) -> TokenStream
where
    Item: quote::ToTokens,
{
    if items.is_empty() {
        quote!()
    } else {
        quote!(<#(#items),*>)
    }
}

/// Returns an identifier's source spelling without a raw-identifier prefix.
fn unraw_name(name: &Ident) -> String {
    name.unraw().to_string()
}

/// Converts a Rust event identifier into an upper-camel-case enum variant.
fn event_variant_ident(name: &Ident) -> Ident {
    let source = unraw_name(name);
    let mut uppercase_next = true;
    let mut variant = String::with_capacity(source.len());
    for character in source.chars() {
        if character == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            variant.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            variant.push(character);
        }
    }
    Ident::new(&variant, name.span())
}

/// Rejects names reserved for hygienic macro implementation details.
fn validate_field_name(name: &Ident) -> Result<()> {
    if name.to_string().starts_with("gpui_vue_internal_") {
        Err(syn::Error::new(
            name.span(),
            "field names beginning with `gpui_vue_internal_` are reserved",
        ))
    } else {
        Ok(())
    }
}

/// Ensures a hook does not accidentally shadow one of its own bindings.
fn validate_distinct_binders<const N: usize>(binders: [&Ident; N], message: &str) -> Result<()> {
    let mut names = HashSet::new();
    for binder in binders {
        if !names.insert(binder.to_string()) {
            return Err(syn::Error::new(binder.span(), message));
        }
    }
    Ok(())
}

/// Resolves the runtime crate when clients rename the package dependency.
fn runtime_crate_path() -> TokenStream {
    match crate_name("gpui-vue") {
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
        Ok(FoundCrate::Itself) | Err(_) => quote!(::gpui_vue),
    }
}

#[cfg(test)]
mod tests {
    //! Parser, validation, and one-shot lowering tests for `component!`.

    use quote::quote;

    use super::*;

    /// Places setup exactly once inside the generated entity constructor.
    #[test]
    fn setup_is_emitted_once_inside_app_context_new() {
        let expanded = expand(&quote! {
            /// A documented fixture.
            pub component Fixture {
                props {
                    /// Required label.
                    pub label: String,
                }
                state {
                    /// Local counter.
                    pub count: usize = 0,
                }
                setup(this, props, _cx) {
                    this.count = props.label.len() + SETUP_MARKER;
                }
                template(this, _window, _cx) {
                    gpui_vue::gpui::div().child(this.count.to_string())
                }
            }
        })
        .expect("the fixture should expand")
        .to_string();

        assert_eq!(expanded.matches("SETUP_MARKER").count(), 1);
        assert!(expanded.contains("AppContext :: new"));
        assert!(expanded.contains("pub struct FixtureInput"));
        assert!(expanded.contains("impl :: gpui_vue :: NativeComponent for Fixture"));
        assert!(expanded.contains("type Props = FixtureProps"));
        assert!(expanded.contains("< Self as :: gpui_vue :: NativeComponent > :: construct"));
        assert!(expanded.contains("impl :: gpui_vue :: gpui :: Render"));
    }

    /// Compares ordinary props exactly once and replaces non-comparable slots without extra notify.
    #[test]
    fn native_reconciliation_distinguishes_props_from_slots() {
        let expanded = expand(&quote! {
            /// A documented fixture.
            pub component Fixture {
                props {
                    /// Comparable value reconciled by the persistent host.
                    pub value: usize,
                }
                slots {
                    /// Non-comparable lazy child content.
                    default: ();
                }
                template(_this, _window, _cx) {
                    gpui_vue::gpui::div()
                }
            }
        })
        .expect("the host fixture should expand")
        .to_string();

        assert!(expanded.contains("derive (:: core :: cmp :: PartialEq)"));
        assert!(expanded.contains("pub struct FixtureInput"));
        assert!(expanded.contains("pub fn with_slots"));
        assert_eq!(
            expanded
                .matches("self . props != gpui_vue_internal_props")
                .count(),
            1
        );
        assert_eq!(
            expanded
                .matches("self . slots = gpui_vue_internal_slots")
                .count(),
            1
        );
        assert_eq!(
            expanded
                .matches("self . props = gpui_vue_internal_props")
                .count(),
            1
        );
        assert_eq!(
            expanded.matches("gpui_vue_internal_cx . notify ()").count(),
            1
        );
        assert!(expanded.contains(
            "gpui_vue_internal_props_changed { gpui_vue_internal_cx . notify () ; } true"
        ));
        let slots_assignment = expanded
            .find("self . slots = gpui_vue_internal_slots")
            .expect("slots should be replaced every parent render");
        let props_assignment = expanded
            .find("self . props = gpui_vue_internal_props")
            .expect("props should be replaced every parent render");
        let notify = expanded
            .find("gpui_vue_internal_cx . notify ()")
            .expect("changed props should notify once");
        assert!(props_assignment < slots_assignment);
        assert!(slots_assignment < notify);
    }

    /// Rejects undocumented generated API inputs before Rust's later lint pass.
    #[test]
    fn public_component_and_fields_require_docs() {
        let component_error = expand(&quote! {
            pub component MissingDocs {
                template(this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("an undocumented component should fail");
        assert!(
            component_error
                .to_string()
                .contains("component declaration")
        );

        let field_error = expand(&quote! {
            /// A documented fixture.
            component MissingFieldDocs {
                props { value: usize }
                template(this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("an undocumented property should fail");
        assert!(field_error.to_string().contains("property declaration"));
    }

    /// Rejects duplicate sections and uninitialized state with targeted errors.
    #[test]
    fn malformed_component_sections_are_rejected() {
        let duplicate = expand(&quote! {
            /// A documented fixture.
            component Duplicate {
                props {}
                props {}
                template(this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("duplicate props should fail");
        assert!(duplicate.to_string().contains("duplicate props"));

        let missing_initializer = expand(&quote! {
            /// A documented fixture.
            component MissingInitializer {
                state {
                    /// State must initialize once.
                    value: usize,
                }
                template(this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("state without an initializer should fail");
        assert!(
            missing_initializer
                .to_string()
                .contains("requires a typed initializer")
        );
    }

    /// Lowers lifecycle sections to one monomorphic visual-mount implementation.
    #[test]
    fn lifecycle_hooks_select_typed_mount_state_and_static_bodies() {
        let expanded = expand(&quote! {
            /// A lifecycle fixture.
            pub component LifecycleFixture {
                updated(this, window, cx) {
                    let _ = (&mut *this, &mut *window, cx.entity_id());
                }
                unmounted(this, cx) {
                    let _ = (&mut *this, cx);
                }
                mounted(this, window, cx) {
                    let _ = (&mut *this, &mut *window, cx.entity_id());
                }
                template(_this, _window, _cx) {
                    gpui_vue::gpui::div()
                }
            }
        })
        .expect("lifecycle sections should expand in any order")
        .to_string();

        assert!(
            expanded.contains("type MountState = :: gpui_vue :: ComponentLifecycleMount < Self >")
        );
        assert!(expanded.contains("ComponentLifecycleHooks for LifecycleFixture"));
        assert!(expanded.contains("const HAS_MOUNTED : bool = true"));
        assert!(expanded.contains("const TRACK_UPDATES : bool = true"));
        assert!(expanded.contains("const HAS_UNMOUNTED : bool = true"));
        assert_eq!(expanded.matches("fn mounted").count(), 1);
        assert_eq!(expanded.matches("fn updated").count(), 1);
        assert_eq!(expanded.matches("fn unmounted").count(), 1);
    }

    /// Keeps hook-free components on the unit mount-state fast path.
    #[test]
    fn hook_free_components_use_unit_mount_state() {
        let expanded = expand(&quote! {
            /// A hook-free fixture.
            component Fixture {
                template(_this, _window, _cx) {
                    gpui_vue::gpui::div()
                }
            }
        })
        .expect("a hook-free component should expand")
        .to_string();

        assert!(expanded.contains("type MountState = ()"));
        assert!(!expanded.contains("ComponentLifecycleHooks for Fixture"));
        assert!(!expanded.contains("ComponentLifecycleMount < Self >"));
    }

    /// Rejects repeated lifecycle sections and accidental binder shadowing.
    #[test]
    fn malformed_lifecycle_sections_are_rejected() {
        let duplicate = expand(&quote! {
            /// A duplicate lifecycle fixture.
            component DuplicateLifecycle {
                mounted(this, window, cx) {}
                mounted(this, window, cx) {}
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("duplicate mounted hooks should fail");
        assert!(duplicate.to_string().contains("duplicate mounted section"));

        let repeated_binder = expand(&quote! {
            /// An invalid lifecycle binder fixture.
            component RepeatedBinder {
                unmounted(this, this) {}
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("repeated unmounted binders should fail");
        assert!(
            repeated_binder
                .to_string()
                .contains("unmounted hook binders must have distinct names")
        );

        let extra_binder = expand(&quote! {
            /// An invalid lifecycle arity fixture.
            component ExtraBinder {
                updated(this, window, cx, extra) {}
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("an extra updated binder should fail");
        assert!(
            extra_binder
                .to_string()
                .contains("updated accepts exactly three bindings")
        );
    }

    /// Lowers typed event declarations directly to GPUI's native event protocol.
    #[test]
    fn emits_generate_event_enum_marker_and_helpers() {
        let expanded = expand(&quote! {
            /// A documented fixture.
            pub component Fixture {
                emits {
                    /// Carries one payload.
                    value_change(value: i32);
                    /// Carries several payloads.
                    submit(label: String, accepted: bool);
                    /// Carries no payload.
                    reset();
                }
                template(_this, _window, _cx) {
                    gpui_vue::gpui::div()
                }
            }
        })
        .expect("typed events should expand")
        .to_string();

        assert!(expanded.contains("pub enum FixtureEvent"));
        assert!(expanded.contains("ValueChange"));
        assert!(expanded.contains("value : i32"));
        assert!(expanded.contains("Submit"));
        assert!(expanded.contains("label : String"));
        assert!(expanded.contains("accepted : bool"));
        assert!(expanded.contains("Reset"));
        assert!(expanded.contains("EventEmitter < FixtureEvent > for Fixture"));
        assert!(expanded.contains("NativeComponentEvents for Fixture"));
        assert!(expanded.contains("type Event = FixtureEvent"));
        assert!(expanded.contains("pub fn __gpui_vue_dispatch_value_change"));
        assert!(expanded.contains("pub fn __gpui_vue_dispatch_submit"));
        assert!(expanded.contains("pub fn __gpui_vue_dispatch_reset"));
        assert!(expanded.contains("Handler : :: core :: ops :: FnMut"));
        assert!(expanded.contains("pub fn emit_value_change"));
        assert!(expanded.contains("FixtureEvent :: ValueChange { value }"));
        assert!(expanded.contains("gpui_vue_internal_cx . emit"));
    }

    /// Rejects undocumented or repeated event declarations with targeted errors.
    #[test]
    fn malformed_emits_are_rejected() {
        let missing_documentation = expand(&quote! {
            /// A documented fixture.
            component MissingEventDocs {
                emits { change(value: usize); }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("an undocumented event should fail");
        assert!(
            missing_documentation
                .to_string()
                .contains("event declaration")
        );

        let duplicate = expand(&quote! {
            /// A documented fixture.
            component DuplicateEvent {
                emits {
                    /// First declaration.
                    change(value: usize);
                    /// Repeated declaration.
                    change(value: usize);
                }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("a repeated event should fail");
        assert!(duplicate.to_string().contains("duplicate event `change`"));

        let duplicate_payload = expand(&quote! {
            /// A documented fixture.
            component DuplicatePayload {
                emits {
                    /// Repeated payload names are ambiguous.
                    change(value: usize, value: usize);
                }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("a repeated event payload should fail");
        assert!(
            duplicate_payload
                .to_string()
                .contains("duplicate payload `value`")
        );
    }

    /// Lowers default, named, and scoped slots to native typed Rust fields.
    #[test]
    fn slots_generate_typed_collection_and_constructor() {
        let expanded = expand(&quote! {
            /// A documented fixture.
            pub component Fixture {
                slots {
                    /// Default unscoped content.
                    default: ();
                    /// An action receiving scoped props.
                    actions: ActionProps;
                }
                template(this, window, cx) {
                    this.slots().default.render_or_else((), window, cx, |(), _, _| {
                        gpui_vue::gpui::div()
                    })
                }
            }
        })
        .expect("typed slots should expand")
        .to_string();

        assert!(expanded.contains("pub struct FixtureSlots"));
        assert!(expanded.contains("pub default : :: gpui_vue :: Slot < () >"));
        assert!(expanded.contains("pub actions : :: gpui_vue :: Slot < ActionProps >"));
        assert!(expanded.contains("pub fn with_default"));
        assert!(expanded.contains("pub fn with_actions"));
        assert!(expanded.contains("pub struct FixtureInput"));
        assert!(expanded.contains("pub fn with_slots"));
        assert!(expanded.contains("pub fn new_with_slots"));
        assert!(expanded.contains("impl :: gpui_vue :: NativeComponentSlots for Fixture"));
        assert!(expanded.contains("type Slots = FixtureSlots"));
        assert!(expanded.contains("fn slots (& self) -> & Self :: Slots"));
        assert!(expanded.contains("fn input_with_slots"));
        assert!(expanded.contains("slots : gpui_vue_internal_slots"));
        assert!(expanded.contains("pub const fn slots"));
    }

    /// Direct markup is lowered in the same macro pass with typed outlet metadata.
    #[test]
    fn direct_markup_receives_component_slot_context() {
        let expanded = expand(&quote! {
            /// A direct-markup fixture.
            component Fixture {
                slots {
                    /// Default unit content.
                    default: ();
                }
                template(this, window, cx) {
                    <div class="flex gap-2">
                        <slot><text>{cx.entity_id().as_u64().to_string()}</text></slot>
                    </div>
                }
            }
        })
        .expect("direct component markup should lower during component expansion")
        .to_string();

        assert!(expanded.contains("NativeComponentSlots > :: slots"));
        assert!(expanded.contains(". default . is_present"));
        assert!(expanded.contains(". child (match"));
        assert_eq!(
            expanded
                .matches("clippy :: no_effect_underscore_binding")
                .count(),
            3
        );
        assert!(!expanded.contains("impl :: gpui_vue :: gpui :: Render for Fixture { # [allow"));
    }

    /// Raw-keyword fields keep raw storage while generated `with_*` names stay legal.
    #[test]
    fn raw_property_and_slot_names_generate_unraw_setters_and_outlets() {
        let expanded = expand(&quote! {
            /// A raw-name fixture.
            component RawFixture {
                props {
                    /// A defaulted raw-keyword property.
                    r#type: usize = 0,
                }
                slots {
                    /// A raw-keyword slot.
                    r#type: ();
                }
                template(_this, _window, _cx) {
                    <slot name="type" />
                }
            }
        })
        .expect("raw declarations must never panic or form `with_r#...` identifiers")
        .to_string();

        assert!(expanded.contains("r#type : usize"));
        assert!(expanded.contains("r#type : :: gpui_vue :: Slot < () >"));
        assert_eq!(expanded.matches("fn with_type").count(), 2);
        assert!(expanded.contains(". r#type . is_present"));
        assert!(!expanded.contains("with_r#type"));
    }

    /// A qualified Rust expression beginning with `<` is not mistaken for markup.
    #[test]
    fn qualified_rust_template_body_remains_compatible() {
        let expanded = expand(&quote! {
            /// A qualified-expression fixture.
            component Fixture {
                template(_this, _window, _cx) {
                    <Self as ElementFactory>::element()
                }
            }
        })
        .expect("qualified Rust syntax should remain an ordinary template block")
        .to_string();

        assert!(expanded.contains("< Self as ElementFactory > :: element ()"));
        assert!(!expanded.contains("allow (unused_variables)"));
    }

    /// Components without a slots section retain their old layout and constructor.
    #[test]
    fn no_slots_emit_no_storage_or_secondary_constructor() {
        let expanded = expand(&quote! {
            /// A documented fixture.
            pub component Fixture {
                template(_this, _window, _cx) {
                    gpui_vue::gpui::div()
                }
            }
        })
        .expect("a component without slots should expand")
        .to_string();

        assert!(expanded.contains("pub struct FixtureInput"));
        assert!(expanded.contains("impl :: gpui_vue :: NativeComponent for Fixture"));
        assert!(!expanded.contains("NativeComponentEvents"));
        assert!(!expanded.contains("NativeComponentSlots"));
        assert!(!expanded.contains("FixtureSlots"));
        assert!(!expanded.contains("with_slots"));
        assert!(!expanded.contains("new_with_slots"));
        assert!(!expanded.contains("gpui_vue_internal_slots"));
    }

    /// Rejects undocumented or repeated slot declarations with targeted errors.
    #[test]
    fn malformed_slots_are_rejected() {
        let missing_documentation = expand(&quote! {
            /// A documented fixture.
            component MissingSlotDocs {
                slots { default: (); }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("an undocumented slot should fail");
        assert!(
            missing_documentation
                .to_string()
                .contains("slot declaration")
        );

        let duplicate = expand(&quote! {
            /// A documented fixture.
            component DuplicateSlot {
                slots {
                    /// First declaration.
                    actions: ();
                    /// Repeated declaration.
                    actions: usize;
                }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("a repeated slot should fail");
        assert!(duplicate.to_string().contains("duplicate slot `actions`"));
    }

    /// Generates marker transitions for required props and ordinary default setters.
    #[test]
    fn props_builder_uses_required_typestate() {
        let expanded = expand(&quote! {
            /// A documented fixture.
            pub component Fixture {
                props {
                    /// First move-only required property.
                    pub label: String,
                    /// Default property with no marker.
                    pub count: usize = 3,
                    /// Second move-only required property.
                    pub values: Vec<usize>,
                }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect("typestate props should expand")
        .to_string();

        assert!(
            expanded.contains("pub struct FixturePropsBuilder < RequiredState0 , RequiredState1 >")
        );
        assert!(
            expanded.contains("label : :: gpui_vue :: RequiredProp < String , RequiredState0 >")
        );
        assert!(expanded.contains("count : usize"));
        assert!(
            expanded.contains(
                "values : :: gpui_vue :: RequiredProp < Vec < usize > , RequiredState1 >"
            )
        );
        assert!(expanded.contains("pub fn builder () -> FixturePropsBuilder < :: gpui_vue :: PropMissing , :: gpui_vue :: PropMissing >"));
        assert!(expanded.contains("pub fn label"));
        assert!(
            expanded.contains("FixturePropsBuilder < :: gpui_vue :: PropSet , RequiredState1 >")
        );
        assert!(expanded.contains("pub fn count"));
        assert!(expanded.contains("pub fn values"));
        assert!(
            expanded.contains("FixturePropsBuilder < RequiredState0 , :: gpui_vue :: PropSet >")
        );
        assert!(expanded.contains(
            "impl FixturePropsBuilder < :: gpui_vue :: PropSet , :: gpui_vue :: PropSet >"
        ));
        assert!(expanded.contains("pub fn build"));
    }

    /// Zero-required builders expose `build` immediately without generic markers.
    #[test]
    fn default_only_props_builder_has_no_generic_parameters() {
        let expanded = expand(&quote! {
            /// A documented fixture.
            component Fixture {
                props {
                    /// Default value.
                    value: usize = 1,
                }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect("default-only props should expand")
        .to_string();

        assert!(expanded.contains("struct FixturePropsBuilder"));
        assert!(!expanded.contains("struct FixturePropsBuilder <"));
        assert!(expanded.contains("fn builder () -> FixturePropsBuilder"));
        assert!(expanded.contains("impl FixturePropsBuilder"));
        assert!(expanded.contains("fn build"));
    }

    /// Rejects a property name that would collide with the terminal method.
    #[test]
    fn build_is_reserved_for_the_props_builder() {
        let error = expand(&quote! {
            /// A documented fixture.
            component Fixture {
                props {
                    /// Invalid terminal-method collision.
                    build: usize,
                }
                template(_this, _window, _cx) { gpui_vue::gpui::div() }
            }
        })
        .expect_err("a build property should fail before lowering");

        assert!(error.to_string().contains("`build` is reserved"));
    }
}
