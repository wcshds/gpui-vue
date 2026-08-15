//! Parser, validation, and GPUI lowering for the `view!` macro.

use std::collections::HashSet;

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Expr, Ident, Lit, LitStr, Pat, Result, Stmt, Token, braced};

use crate::tailwind::CompiledClasses;

/// Parses a template and lowers it to ordinary GPUI builder calls.
pub(crate) fn expand(input: &TokenStream) -> Result<TokenStream> {
    expand_with_context(input, None)
}

/// Validates one token stream as Vue-shaped template markup.
pub(crate) fn validate_template(input: &TokenStream) -> Result<()> {
    syn::parse2::<Template>(input.clone()).map(|_| ())
}

/// Lowers direct component markup with exact render bindings and slot metadata.
pub(crate) fn expand_component_template(
    input: &TokenStream,
    context: &ComponentTemplateContext,
) -> Result<TokenStream> {
    expand_with_context(input, Some(context))
}

/// Shared standalone and component-aware template lowering entry point.
fn expand_with_context(
    input: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    let template = syn::parse2::<Template>(input.clone())?;
    let crate_path = runtime_crate_path();
    if let Some(context) = context {
        validate_unique_slot_outlets(&template.roots, context)?;
    }
    let element = expand_roots(&template.roots, &crate_path, context)?;

    Ok(quote! {{
        #[allow(unused_imports)]
        use #crate_path::gpui::prelude::*;
        #element
    }})
}

/// Exact component-template bindings available only to direct markup.
pub(crate) struct ComponentTemplateContext {
    /// User-selected mutable component binding.
    this: Ident,
    /// User-selected mutable window binding.
    window: Ident,
    /// User-selected mutable entity-context binding.
    context: Ident,
    /// Statically declared slots on the receiving component.
    slots: Vec<ComponentSlotMetadata>,
}

impl ComponentTemplateContext {
    /// Creates a component-aware template-lowering context.
    pub(crate) const fn new(
        this: Ident,
        window: Ident,
        context: Ident,
        slots: Vec<ComponentSlotMetadata>,
    ) -> Self {
        Self {
            this,
            window,
            context,
            slots,
        }
    }

    /// Finds one declaration by its normalized source name.
    fn slot(&self, canonical: &str) -> Option<&ComponentSlotMetadata> {
        self.slots
            .iter()
            .find(|slot| slot.name.unraw() == canonical)
    }
}

/// One generated component slot exposed to direct-markup lowering.
pub(crate) struct ComponentSlotMetadata {
    /// Exact generated field identifier, including raw-identifier spelling.
    name: Ident,
    /// Typed props accepted when the child invokes this slot.
    props: syn::Type,
}

impl ComponentSlotMetadata {
    /// Creates metadata from one parsed component slot declaration.
    pub(crate) const fn new(name: Ident, props: syn::Type) -> Self {
        Self { name, props }
    }

    /// Reports whether the declared props are syntactically unit.
    fn accepts_implicit_unit(&self) -> bool {
        matches!(&self.props, syn::Type::Tuple(tuple) if tuple.elems.is_empty())
    }
}

/// Resolves the runtime crate even when the macro is invoked from that crate.
fn runtime_crate_path() -> TokenStream {
    match crate_name("gpui-vue") {
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
        // `extern crate self as gpui_vue` makes this path work in the library,
        // while Cargo exposes the same name to examples and integration tests.
        Ok(FoundCrate::Itself) | Err(_) => quote!(::gpui_vue),
    }
}

/// A complete macro template, which may contain multiple Vue-style roots.
#[derive(Debug)]
struct Template {
    /// Top-level nodes in source order.
    roots: Vec<Node>,
}

impl Parse for Template {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut roots = Vec::new();
        let mut next_element_ordinal = 0;
        while !input.is_empty() {
            roots.push(parse_node(input, &mut next_element_ordinal)?);
        }
        if roots.is_empty() {
            return Err(input.error("a view! template cannot be empty"));
        }
        Ok(Self { roots })
    }
}

/// A parsed intrinsic or structural element.
#[derive(Clone, Debug)]
struct Element {
    /// Source-level tag name.
    tag: Ident,
    /// Stable source-order position within this `view!` invocation.
    ordinal: usize,
    /// Non-structural attributes in source order.
    attributes: Vec<Attribute>,
    /// Child nodes in source order.
    children: Vec<Node>,
    /// Optional `v-if`, `v-else-if`, or `v-else` directive.
    conditional: Option<ConditionalDirective>,
    /// Optional `v-for` loop.
    directive_for: Option<ForDirective>,
    /// Optional `v-show` visibility expression.
    directive_show: Option<Expr>,
}

/// A template child node.
#[derive(Clone, Debug)]
enum Node {
    /// A tagged element, including the structural `<template>` tag.
    Element(Box<Element>),
    /// A Rust expression inside braces.
    Expression(Expr),
    /// A literal text child.
    Text(LitStr),
    /// An explicit `<>...</>` fragment that is flattened into its parent.
    Fragment(Vec<Node>),
}

/// A normalized sibling conditional directive.
#[derive(Clone, Debug)]
enum ConditionalDirective {
    /// Starts a conditional chain.
    If(Expr),
    /// Adds a tested branch to the preceding chain.
    ElseIf(Expr),
    /// Adds the terminal fallback branch.
    Else,
}

/// The Rust pattern and iterator carried by `v-for`.
#[derive(Clone, Debug)]
struct ForDirective {
    /// Pattern bound for each iterator item.
    pattern: Pat,
    /// Rust expression converted with `IntoIterator`.
    iterator: Expr,
}

/// A source attribute with its diagnostic location.
#[derive(Clone, Debug)]
struct Attribute {
    /// Normalized textual name, including event modifiers.
    name: String,
    /// Span used for targeted compile errors.
    span: Span,
    /// Optional literal or expression value.
    value: Option<AttributeValue>,
}

/// A supported source attribute value.
#[derive(Clone, Debug)]
enum AttributeValue {
    /// Rust expression enclosed in braces.
    Expression(Expr),
    /// Static string literal.
    String(LitStr),
    /// Rust pattern used only by a `#slot={pattern}` provider declaration.
    Pattern(Pat),
}

/// Mutable bindings collected before validating an intrinsic element.
#[derive(Debug, Default)]
struct ElementBindings {
    /// Stable source-order position of the intrinsic within this macro call.
    element_ordinal: usize,
    /// Static Tailwind class literal.
    class: Option<LitStr>,
    /// Compile-time-known conditional class branches.
    dynamic_class: Option<DynamicClassBinding>,
    /// Typed runtime refinement applied after ordinary class styles.
    inline_style: Option<Expr>,
    /// Native child content supplied by the intrinsic-only `v-text` directive.
    text_content: Option<OrderedExpression>,
    /// Required source used only by the native image intrinsic.
    image_source: Option<OrderedTokens>,
    /// Native object-fit policy used only by the image intrinsic.
    image_object_fit: Option<OrderedExpression>,
    /// Lazy native loading view used only by the image intrinsic.
    image_loading: Option<OrderedExpression>,
    /// Lazy native error fallback used only by the image intrinsic.
    image_fallback: Option<OrderedExpression>,
    /// Explicit GPUI element identity.
    id: Option<TokenStream>,
    /// Vue key lowered to a GPUI element identity.
    key: Option<TokenStream>,
    /// Whether the key came from a dynamic `:key` binding.
    has_bound_key: bool,
    /// Explicit focus handle tracked by the native GPUI host.
    track_focus: Option<Expr>,
    /// Static or dynamic keyboard context supplied to GPUI.
    key_context: Option<TokenStream>,
    /// Source-ordered native host event listeners.
    events: Vec<EventBinding>,
    /// Typed value retained by GPUI for one native drag operation.
    drag_payload: Option<OrderedExpression>,
    /// Typed entity constructor used for the native drag preview.
    drag_preview: Option<OrderedExpression>,
    /// Optional type-erased predicate gating native drop dispatch.
    can_drop: Option<OrderedExpression>,
    /// Source-ordered typed styles applied while matching drags hover the host.
    drag_over: Vec<OrderedExpression>,
    /// Whether GPUI should track focus for this element.
    focusable: bool,
    /// Whether this host blocks pointer interaction with hitboxes behind it.
    occlude: bool,
    /// Explicit keyboard tab order.
    tab_index: Option<Expr>,
}

/// Validated bindings for one `PascalCase` native component host.
#[derive(Debug)]
struct ComponentBindings {
    /// Either a complete generated props value or ordered individual setters.
    props: ComponentPropsBinding,
    /// Optional explicit or declaratively constructed typed slot collection.
    slots: Option<ComponentSlotsBinding>,
    /// Optional identity nested below the compile-site slot.
    key: Option<TokenStream>,
    /// Typed component listeners combined into one native subscription.
    events: Vec<ComponentEventBinding>,
}

/// Mutually exclusive ways to supply a generated typed slot collection.
#[derive(Debug)]
enum ComponentSlotsBinding {
    /// A complete slot collection supplied through `:slots={...}`.
    Explicit(Expr),
    /// Lazy providers declared as component children.
    Declarative(Vec<ComponentSlotBinding>),
}

/// One validated default or named lazy slot provider.
#[derive(Debug)]
struct ComponentSlotBinding {
    /// Canonical snake-case slot name used for duplicate detection.
    canonical: String,
    /// Generated fluent setter, such as `with_default` or `with_actions`.
    setter: Ident,
    /// Explicit closure pattern, or `None` for one hygienic ignored binding.
    pattern: Option<Pat>,
    /// Provider roots after removing named templates from default content.
    roots: Vec<Node>,
    /// Source span retained for method lookup and provider type diagnostics.
    span: Span,
}

/// One validated child-side typed slot outlet.
#[derive(Debug)]
struct SlotOutletBinding {
    /// Exact generated slot field selected from component metadata.
    field: Ident,
    /// Typed props moved into the provider when one is present.
    props: Expr,
    /// Child nodes rendered lazily only when no provider produces content.
    fallback: Vec<Node>,
    /// Source span used by generated typed calls and diagnostics.
    span: Span,
}

/// Source attributes accepted by one child-side slot outlet.
struct SlotOutletAttributes {
    /// Optional static name and its diagnostic span.
    source_name: Option<(String, Span)>,
    /// Optional typed props expression and its diagnostic span.
    props: Option<(Expr, Span)>,
}

/// One of the two mutually exclusive component props construction modes.
#[derive(Debug)]
enum ComponentPropsBinding {
    /// A complete generated props value supplied through `:props={...}`.
    Complete(Expr),
    /// Individual attributes lowered through the generated typestate builder.
    Individual(Vec<ComponentPropBinding>),
}

/// One normalized individual component property setter.
#[derive(Debug)]
struct ComponentPropBinding {
    /// Rust method name generated from the source attribute name.
    method: Ident,
    /// Exact value passed to the generated setter.
    value: Expr,
    /// Attribute span retained for setter and type diagnostics.
    span: Span,
}

/// One `PascalCase` component event listener and its generated dispatcher.
#[derive(Debug)]
struct ComponentEventBinding {
    /// Canonical snake-case event name used for duplicate detection.
    canonical: String,
    /// Hidden method generated on the component's associated event enum.
    dispatcher: Ident,
    /// Parent expression evaluated exactly once for this render.
    handler: Expr,
    /// Attribute span retained for handler diagnostics.
    span: Span,
}

/// A deferred dynamic-class expression and its attribute diagnostic span.
#[derive(Debug)]
struct DynamicClassBinding {
    /// Literal or conditional Rust expression to compile after all attributes are known.
    expression: Expr,
    /// Span of the `:class` attribute for unsupported-expression diagnostics.
    span: Span,
}

/// One intrinsic expression retaining its exact attribute position.
#[derive(Debug)]
struct OrderedExpression {
    /// User expression evaluated exactly once.
    expression: Expr,
    /// Zero-based position in the host's source attribute list.
    source_index: usize,
    /// Attribute span retained for generated calls and diagnostics.
    span: Span,
}

/// One intrinsic token expression retaining its exact attribute position.
///
/// Image `src` accepts both a Rust expression and a static string literal, so
/// it cannot use [`OrderedExpression`] without fabricating a parsed Rust AST.
#[derive(Debug)]
struct OrderedTokens {
    /// User value evaluated exactly once.
    tokens: TokenStream,
    /// Zero-based position in the host's source attribute list.
    source_index: usize,
    /// Attribute span retained for generated calls and diagnostics.
    span: Span,
}

/// One validated native GPUI event handler.
#[derive(Debug)]
struct EventBinding {
    /// Native event and any required mouse button.
    kind: EventKind,
    /// User-provided GPUI listener expression.
    handler: Expr,
    /// Click-only modifiers applied in source order.
    modifiers: Vec<EventModifier>,
    /// Zero-based position in the host's source attribute list.
    source_index: usize,
    /// Attribute span retained for cross-binding diagnostics.
    span: Span,
}

/// One source-positioned native interaction or content binding on an intrinsic.
enum OrderedHostInteraction<'a> {
    /// An ordinary or typed native event listener.
    Event(&'a EventBinding),
    /// The payload half of the one drag-source pair.
    DragPayload(&'a OrderedExpression),
    /// The preview-constructor half of the one drag-source pair.
    DragPreview(&'a OrderedExpression),
    /// The host's one type-erased drop predicate.
    CanDrop(&'a OrderedExpression),
    /// One exact-payload-type drag-over style lane.
    DragOver(&'a OrderedExpression),
    /// One intrinsic text child evaluated at its source attribute position.
    Text(&'a OrderedExpression),
}

impl OrderedHostInteraction<'_> {
    /// Returns the exact source attribute position used for stable lowering.
    const fn source_index(&self) -> usize {
        match self {
            Self::Event(event) => event.source_index,
            Self::DragPayload(binding)
            | Self::DragPreview(binding)
            | Self::CanDrop(binding)
            | Self::DragOver(binding)
            | Self::Text(binding) => binding.source_index,
        }
    }
}

/// A native GPUI event supported by an intrinsic host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EventKind {
    /// Pointer click with optional Vue-compatible guards and side effects.
    Click,
    /// Keyboard input delivered to the host's key context.
    KeyDown,
    /// Keyboard release delivered to the host's key context.
    KeyUp,
    /// Keyboard modifier-state change delivered along the focused dispatch path.
    ModifiersChanged,
    /// Pointer press for one explicit mouse button.
    MouseDown(MouseButtonBinding),
    /// Pointer press outside the host for one explicit mouse button.
    MouseDownOut(MouseButtonBinding),
    /// Pointer movement over the host.
    MouseMove,
    /// Movement during an active drag whose payload has one exact Rust type.
    DragMove,
    /// Pointer release for one explicit mouse button.
    MouseUp(MouseButtonBinding),
    /// Pointer release after leaving the host for one explicit mouse button.
    MouseUpOut(MouseButtonBinding),
    /// Native trackpad pinch/magnification input.
    Pinch,
    /// Native scroll-wheel input.
    ScrollWheel,
    /// Native hitbox hover entry and exit.
    Hover,
    /// A drop whose payload has one exact Rust type.
    Drop,
    /// Exact focus acquisition for an explicitly tracked focus handle.
    Focus,
    /// Exact focus loss for an explicitly tracked focus handle.
    Blur,
}

/// A source mouse-button modifier lowered to GPUI's native enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseButtonBinding {
    /// Primary/left pointer button.
    Left,
    /// Secondary/right pointer button.
    Right,
    /// Middle pointer button.
    Middle,
}

/// Event modifiers with a direct GPUI equivalent for click events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EventModifier {
    /// Stops GPUI event propagation.
    Stop,
    /// Suppresses GPUI's default click behavior.
    Prevent,
    /// Requires the control modifier.
    Control,
    /// Requires the alt modifier.
    Alt,
    /// Requires the shift modifier.
    Shift,
    /// Requires the platform command/super modifier.
    Meta,
    /// Rejects any system modifier not explicitly requested.
    Exact,
}

/// A statically enumerable dynamic class expression.
#[derive(Debug)]
enum DynamicClasses {
    /// One literal Tailwind class list.
    Literal(CompiledClasses),
    /// A Rust boolean condition selecting two precompiled class branches.
    Conditional {
        /// Boolean condition emitted into the GPUI builder chain.
        condition: Expr,
        /// Classes applied when the condition is true.
        then_branch: Box<Self>,
        /// Optional classes applied when the condition is false.
        else_branch: Option<Box<Self>>,
    },
}

impl Element {
    /// Parses an element after its opening `<` has already been consumed.
    fn parse_opened(input: ParseStream<'_>, next_element_ordinal: &mut usize) -> Result<Self> {
        let tag = Ident::parse_any(input)?;
        let ordinal = *next_element_ordinal;
        *next_element_ordinal += 1;
        let mut attributes = Vec::new();
        let mut conditional = None;
        let mut directive_for = None;
        let mut directive_show = None;

        while !input.peek(Token![>]) && !input.peek(Token![/]) {
            let (name, span) = parse_attribute_name(input)?;
            if name == "v-for" {
                parse_for_directive(input, span, &mut directive_for)?;
                continue;
            }

            let value = if name.starts_with('#') {
                parse_optional_slot_pattern(input)?
            } else {
                parse_optional_attribute_value(input)?
            };
            match name.as_str() {
                "v-if" => set_conditional(
                    &mut conditional,
                    ConditionalDirective::If(expect_expression(value, span, "v-if")?),
                    span,
                )?,
                "v-else-if" => set_conditional(
                    &mut conditional,
                    ConditionalDirective::ElseIf(expect_expression(value, span, "v-else-if")?),
                    span,
                )?,
                "v-else" => {
                    if value.is_some() {
                        return Err(syn::Error::new(span, "v-else does not take a value"));
                    }
                    set_conditional(&mut conditional, ConditionalDirective::Else, span)?;
                }
                "v-show" => {
                    if directive_show.is_some() {
                        return Err(syn::Error::new(span, "duplicate v-show directive"));
                    }
                    directive_show = Some(expect_expression(value, span, "v-show")?);
                }
                _ => attributes.push(Attribute { name, span, value }),
            }
        }

        let self_closing = parse_tag_end(input)?;
        let children = if self_closing {
            Vec::new()
        } else {
            parse_element_children(input, &tag, next_element_ordinal)?
        };

        Ok(Self {
            tag,
            ordinal,
            attributes,
            children,
            conditional,
            directive_for,
            directive_show,
        })
    }
}

/// Parses either an element or an explicit fragment after seeing `<`.
fn parse_markup(input: ParseStream<'_>, next_element_ordinal: &mut usize) -> Result<Node> {
    input.parse::<Token![<]>()?;
    if input.peek(Token![>]) {
        input.parse::<Token![>]>()?;
        return parse_fragment_children(input, next_element_ordinal).map(Node::Fragment);
    }
    if input.peek(Token![/]) {
        return Err(input.error("unexpected closing tag"));
    }
    Element::parse_opened(input, next_element_ordinal)
        .map(Box::new)
        .map(Node::Element)
}

/// Parses one template node.
fn parse_node(input: ParseStream<'_>, next_element_ordinal: &mut usize) -> Result<Node> {
    if input.peek(Token![<]) {
        return parse_markup(input, next_element_ordinal);
    }
    if input.peek(syn::token::Brace) {
        let content;
        braced!(content in input);
        let expression = content.parse::<Expr>()?;
        if !content.is_empty() {
            return Err(content.error("expected one Rust expression inside `{ ... }`"));
        }
        return Ok(Node::Expression(expression));
    }
    if input.peek(LitStr) {
        return input.parse().map(Node::Text);
    }
    Err(input
        .error("expected a child element, a string literal, or a Rust expression in `{ ... }`"))
}

/// Parses `/>` or `>` and reports whether the element is self-closing.
fn parse_tag_end(input: ParseStream<'_>) -> Result<bool> {
    if input.peek(Token![/]) {
        input.parse::<Token![/]>()?;
        input.parse::<Token![>]>()?;
        Ok(true)
    } else {
        input.parse::<Token![>]>()?;
        Ok(false)
    }
}

/// Parses children and validates the matching named closing tag.
fn parse_element_children(
    input: ParseStream<'_>,
    tag: &Ident,
    next_element_ordinal: &mut usize,
) -> Result<Vec<Node>> {
    let mut children = Vec::new();
    while !is_closing_markup(input) {
        if input.is_empty() {
            return Err(syn::Error::new(
                tag.span(),
                format!("missing closing tag </{tag}>"),
            ));
        }
        children.push(parse_node(input, next_element_ordinal)?);
    }

    input.parse::<Token![<]>()?;
    input.parse::<Token![/]>()?;
    if input.peek(Token![>]) {
        return Err(input.error(format!("expected closing tag </{tag}>")));
    }
    let closing = Ident::parse_any(input)?;
    input.parse::<Token![>]>()?;
    if closing != *tag {
        return Err(syn::Error::new(
            closing.span(),
            format!("closing tag </{closing}> does not match <{tag}>"),
        ));
    }
    Ok(children)
}

/// Parses children until an explicit `</>` fragment terminator.
fn parse_fragment_children(
    input: ParseStream<'_>,
    next_element_ordinal: &mut usize,
) -> Result<Vec<Node>> {
    let mut children = Vec::new();
    while !is_closing_markup(input) {
        if input.is_empty() {
            return Err(input.error("missing fragment closing tag </>"));
        }
        children.push(parse_node(input, next_element_ordinal)?);
    }

    input.parse::<Token![<]>()?;
    input.parse::<Token![/]>()?;
    if !input.peek(Token![>]) {
        return Err(input.error("a fragment must close with </>"));
    }
    input.parse::<Token![>]>()?;
    Ok(children)
}

/// Checks whether the next tokens begin any closing tag.
fn is_closing_markup(input: ParseStream<'_>) -> bool {
    if !input.peek(Token![<]) {
        return false;
    }
    let fork = input.fork();
    fork.parse::<Token![<]>().is_ok() && fork.peek(Token![/])
}

/// Parses `v-for={PAT in EXPR}` into its Rust pattern and iterator.
fn parse_for_directive(
    input: ParseStream<'_>,
    span: Span,
    destination: &mut Option<ForDirective>,
) -> Result<()> {
    if destination.is_some() {
        return Err(syn::Error::new(span, "duplicate v-for directive"));
    }
    input.parse::<Token![=]>()?;
    let content;
    braced!(content in input);
    let pattern = Pat::parse_single(&content)?;
    content.parse::<Token![in]>()?;
    let iterator = content.parse::<Expr>()?;
    if !content.is_empty() {
        return Err(content.error("unexpected tokens after the v-for iterator"));
    }
    *destination = Some(ForDirective { pattern, iterator });
    Ok(())
}

/// Stores one conditional directive and rejects duplicates on the same node.
fn set_conditional(
    destination: &mut Option<ConditionalDirective>,
    directive: ConditionalDirective,
    span: Span,
) -> Result<()> {
    if destination.is_some() {
        return Err(syn::Error::new(
            span,
            "an element can have only one of v-if, v-else-if, or v-else",
        ));
    }
    *destination = Some(directive);
    Ok(())
}

/// Parses an attribute name, including repeated dashes and event modifiers.
fn parse_attribute_name(input: ParseStream<'_>) -> Result<(String, Span)> {
    let (mut name, span) = if input.peek(Token![@]) {
        let at = input.parse::<Token![@]>()?;
        let event = Ident::parse_any(input)?;
        (format!("@{}", event.unraw()), at.span)
    } else if input.peek(Token![#]) {
        let pound = input.parse::<Token![#]>()?;
        let slot = Ident::parse_any(input)?;
        (format!("#{}", slot.unraw()), pound.span)
    } else if input.peek(Token![:]) {
        let colon = input.parse::<Token![:]>()?;
        let binding = Ident::parse_any(input)?;
        (format!(":{}", binding.unraw()), colon.span)
    } else {
        let first = Ident::parse_any(input)?;
        (first.unraw().to_string(), first.span())
    };

    loop {
        if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            let part = Ident::parse_any(input)?;
            name.push('-');
            name.push_str(&part.unraw().to_string());
        } else if input.peek(Token![:]) && name == "on" {
            input.parse::<Token![:]>()?;
            let part = Ident::parse_any(input)?;
            name.push(':');
            name.push_str(&part.unraw().to_string());
        } else if input.peek(Token![.]) {
            input.parse::<Token![.]>()?;
            let part = Ident::parse_any(input)?;
            name.push('.');
            name.push_str(&part.unraw().to_string());
        } else {
            break;
        }
    }
    Ok((name, span))
}

/// Parses the optional Rust pattern in `#name={pattern}`.
fn parse_optional_slot_pattern(input: ParseStream<'_>) -> Result<Option<AttributeValue>> {
    if !input.peek(Token![=]) {
        return Ok(None);
    }
    input.parse::<Token![=]>()?;
    if !input.peek(syn::token::Brace) {
        return Err(input.error("slot props must be a Rust pattern in `{ ... }`"));
    }
    let content;
    braced!(content in input);
    let pattern = Pat::parse_single(&content)?;
    if !content.is_empty() {
        return Err(content.error("expected one Rust pattern inside `{ ... }`"));
    }
    Ok(Some(AttributeValue::Pattern(pattern)))
}

/// Parses an optional `=value` following an attribute name.
fn parse_optional_attribute_value(input: ParseStream<'_>) -> Result<Option<AttributeValue>> {
    if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
        parse_attribute_value(input).map(Some)
    } else {
        Ok(None)
    }
}

/// Parses a literal or braced Rust attribute value.
fn parse_attribute_value(input: ParseStream<'_>) -> Result<AttributeValue> {
    if input.peek(LitStr) {
        return input.parse().map(AttributeValue::String);
    }
    if input.peek(syn::token::Brace) {
        let content;
        braced!(content in input);
        let expression = content.parse::<Expr>()?;
        if !content.is_empty() {
            return Err(content.error("expected one Rust expression inside `{ ... }`"));
        }
        return Ok(AttributeValue::Expression(expression));
    }
    Err(input.error("attribute values must be a string or `{ Rust expression }`"))
}

/// Requires a braced Rust expression for a structural directive.
fn expect_expression(value: Option<AttributeValue>, span: Span, name: &str) -> Result<Expr> {
    match value {
        Some(AttributeValue::Expression(expression)) => Ok(expression),
        _ => Err(syn::Error::new(
            span,
            format!("{name} requires a Rust expression: {name}={{...}}"),
        )),
    }
}

impl ElementBindings {
    /// Collects and validates the supported attributes for an intrinsic tag.
    fn parse(element: &Element, is_button: bool, is_image: bool) -> Result<Self> {
        let mut bindings = Self {
            focusable: is_button,
            element_ordinal: element.ordinal,
            ..Self::default()
        };
        for (source_index, attribute) in element.attributes.iter().enumerate() {
            bindings.apply_attribute(attribute, is_image, source_index)?;
        }
        if let Some(text) = &bindings.text_content
            && !element.children.is_empty()
        {
            return Err(syn::Error::new(
                text.span,
                "v-text replaces an intrinsic's child content and cannot be used with children",
            ));
        }
        match (&bindings.drag_payload, &bindings.drag_preview) {
            (Some(payload), None) => {
                return Err(syn::Error::new(
                    payload.span,
                    ":drag-payload requires a matching :drag-preview binding on the same host",
                ));
            }
            (None, Some(preview)) => {
                return Err(syn::Error::new(
                    preview.span,
                    ":drag-preview requires a matching :drag-payload binding on the same host",
                ));
            }
            (Some(_), Some(_)) | (None, None) => {}
        }
        if let Some(event) = bindings
            .events
            .iter()
            .find(|event| event.kind.is_focus_lifecycle())
            && bindings.track_focus.is_none()
        {
            return Err(syn::Error::new(
                event.span,
                format!(
                    "@{} requires :track-focus={{&focus_handle}} so GPUI can observe one exact native focus handle",
                    event.kind.diagnostic_name(),
                ),
            ));
        }
        if bindings.id.is_some() && bindings.key.is_some() {
            return Err(syn::Error::new(
                element.tag.span(),
                "use either id or :key; both map to GPUI's ElementId",
            ));
        }
        if element.directive_for.is_some() && !bindings.has_bound_key {
            return Err(syn::Error::new(
                element.tag.span(),
                "every v-for root requires a dynamic `:key={...}` to namespace GPUI element state",
            ));
        }
        Ok(bindings)
    }

    /// Applies one parsed source attribute while preserving duplicate diagnostics.
    fn apply_attribute(
        &mut self,
        attribute: &Attribute,
        is_image: bool,
        source_index: usize,
    ) -> Result<()> {
        match attribute.name.as_str() {
            "class" => self.set_class(attribute),
            "src" | ":src" if is_image => self.set_image_source(attribute, source_index),
            ":object-fit" if is_image => self.set_image_object_fit(attribute, source_index),
            ":loading" if is_image => self.set_image_loading(attribute, source_index),
            ":fallback" if is_image => self.set_image_fallback(attribute, source_index),
            "src" | ":src" => Err(syn::Error::new(
                attribute.span,
                "`src` / `:src` is only supported on the <img> intrinsic",
            )),
            ":object-fit" | ":loading" | ":fallback" => Err(syn::Error::new(
                attribute.span,
                format!(
                    "{} is only supported on the <img> intrinsic",
                    attribute.name
                ),
            )),
            "id" | ":id" => self.set_id(attribute),
            "key" => self.set_static_key(attribute),
            ":key" => self.set_bound_key(attribute),
            "focusable" => self.set_focusable(attribute),
            "occlude" => self.set_occlude(attribute),
            "tab-index" | "tab_index" => self.set_tab_index(attribute),
            ":track-focus" => self.set_track_focus(attribute),
            "key-context" | ":key-context" => self.set_key_context(attribute),
            ":class" => self.set_dynamic_class(attribute),
            ":style" => self.set_inline_style(attribute),
            "v-text" => self.set_text_content(attribute, is_image, source_index),
            ":drag-payload" => self.set_drag_payload(attribute, source_index),
            ":drag-preview" => self.set_drag_preview(attribute, source_index),
            ":can-drop" => self.set_can_drop(attribute, source_index),
            ":drag-over" => self.push_drag_over(attribute, source_index),
            name if name.starts_with('@') || name.starts_with("on:") => {
                self.set_event(attribute, source_index)
            }
            unknown => Err(syn::Error::new(
                attribute.span,
                format!("unsupported attribute `{unknown}`"),
            )),
        }
    }

    /// Stores one native child expression for the intrinsic-only `v-text` lane.
    fn set_text_content(
        &mut self,
        attribute: &Attribute,
        is_image: bool,
        source_index: usize,
    ) -> Result<()> {
        if is_image {
            return Err(syn::Error::new(
                attribute.span,
                "v-text requires a parent intrinsic and cannot be used on <img>",
            ));
        }
        if self.text_content.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate v-text directive",
            ));
        }
        self.text_content = Some(OrderedExpression {
            expression: expect_attribute_expression(attribute, "v-text")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores the one typed payload used to initiate a native drag.
    fn set_drag_payload(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        if self.drag_payload.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate :drag-payload binding; a host can initiate only one native drag",
            ));
        }
        self.drag_payload = Some(OrderedExpression {
            expression: expect_attribute_expression(attribute, ":drag-payload")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores the one entity constructor paired with `:drag-payload`.
    fn set_drag_preview(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        if self.drag_preview.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate :drag-preview binding; a host can render only one native drag preview",
            ));
        }
        self.drag_preview = Some(OrderedExpression {
            expression: expect_attribute_expression(attribute, ":drag-preview")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores the one type-erased predicate used by all typed drop lanes.
    fn set_can_drop(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        if self.can_drop.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate :can-drop binding; a host has one native drop predicate",
            ));
        }
        self.can_drop = Some(OrderedExpression {
            expression: expect_attribute_expression(attribute, ":can-drop")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Adds one exact-payload-type native drag-over style lane.
    fn push_drag_over(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        self.drag_over.push(OrderedExpression {
            expression: expect_attribute_expression(attribute, ":drag-over")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores a compile-time class literal.
    fn set_class(&mut self, attribute: &Attribute) -> Result<()> {
        if self.class.is_some() {
            return Err(syn::Error::new(attribute.span, "duplicate class attribute"));
        }
        match &attribute.value {
            Some(AttributeValue::String(value)) => {
                self.class = Some(value.clone());
                Ok(())
            }
            _ => Err(syn::Error::new(
                attribute.span,
                "class must be a static literal so it can compile away",
            )),
        }
    }

    /// Stores a literal or Rust `if` tree used by Vue-style `:class`.
    fn set_dynamic_class(&mut self, attribute: &Attribute) -> Result<()> {
        if self.dynamic_class.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate dynamic class binding",
            ));
        }
        let expression = expect_attribute_expression(attribute, ":class")?;
        self.dynamic_class = Some(DynamicClassBinding {
            expression,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores one typed `StyleRefinement -> StyleRefinement` callback.
    fn set_inline_style(&mut self, attribute: &Attribute) -> Result<()> {
        if self.inline_style.is_some() {
            return Err(syn::Error::new(attribute.span, "duplicate :style binding"));
        }
        self.inline_style = Some(expect_attribute_expression(attribute, ":style")?);
        Ok(())
    }

    /// Stores the static or dynamically bound source of an `<img>` host.
    fn set_image_source(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        if self.image_source.is_some() {
            return Err(syn::Error::new(attribute.span, "duplicate image source"));
        }
        let tokens = if attribute.name == ":src" {
            let expression = expect_bound_expression(attribute, "src")?;
            quote!(#expression)
        } else {
            value_tokens(attribute)?
        };
        self.image_source = Some(OrderedTokens {
            tokens,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores the one typed GPUI object-fit policy on an `<img>` host.
    fn set_image_object_fit(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        if self.image_object_fit.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate :object-fit binding",
            ));
        }
        self.image_object_fit = Some(OrderedExpression {
            expression: expect_attribute_expression(attribute, ":object-fit")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores the one lazy GPUI loading view on an `<img>` host.
    fn set_image_loading(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        if self.image_loading.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate :loading binding",
            ));
        }
        self.image_loading = Some(OrderedExpression {
            expression: expect_attribute_expression(attribute, ":loading")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores the one lazy GPUI error view on an `<img>` host.
    fn set_image_fallback(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        if self.image_fallback.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate :fallback binding",
            ));
        }
        self.image_fallback = Some(OrderedExpression {
            expression: expect_attribute_expression(attribute, ":fallback")?,
            source_index,
            span: attribute.span,
        });
        Ok(())
    }

    /// Stores an explicit element id.
    fn set_id(&mut self, attribute: &Attribute) -> Result<()> {
        if self.id.is_some() {
            return Err(syn::Error::new(attribute.span, "duplicate id attribute"));
        }
        self.id = Some(if attribute.name == ":id" {
            let expression = expect_bound_expression(attribute, "id")?;
            quote!(#expression)
        } else {
            value_tokens(attribute)?
        });
        Ok(())
    }

    /// Stores a static key, which is valid outside loops.
    fn set_static_key(&mut self, attribute: &Attribute) -> Result<()> {
        if self.key.is_some() {
            return Err(syn::Error::new(attribute.span, "duplicate key attribute"));
        }
        self.key = Some(value_tokens(attribute)?);
        Ok(())
    }

    /// Stores a dynamic key and rejects a literal masquerading as loop identity.
    fn set_bound_key(&mut self, attribute: &Attribute) -> Result<()> {
        if self.key.is_some() {
            return Err(syn::Error::new(attribute.span, "duplicate key attribute"));
        }
        let expression = expect_bound_expression(attribute, "key")?;
        if matches!(&expression, Expr::Lit(_)) {
            return Err(syn::Error::new(
                attribute.span,
                ":key must derive a unique identity from the v-for item, not a literal",
            ));
        }
        self.key = Some(quote!(#expression));
        self.has_bound_key = true;
        Ok(())
    }

    /// Enables focus tracking for a boolean `focusable` attribute.
    fn set_focusable(&mut self, attribute: &Attribute) -> Result<()> {
        if attribute.value.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "focusable is a boolean attribute and takes no value",
            ));
        }
        self.focusable = true;
        Ok(())
    }

    /// Enables native pointer occlusion on this intrinsic host.
    fn set_occlude(&mut self, attribute: &Attribute) -> Result<()> {
        if self.occlude {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate occlude attribute",
            ));
        }
        if attribute.value.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "occlude is a boolean attribute and takes no value",
            ));
        }
        self.occlude = true;
        Ok(())
    }

    /// Stores an explicit keyboard tab index.
    fn set_tab_index(&mut self, attribute: &Attribute) -> Result<()> {
        if self.tab_index.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate tab-index attribute",
            ));
        }
        self.tab_index = Some(expect_attribute_expression(attribute, "tab-index")?);
        Ok(())
    }

    /// Stores the exact native focus-handle reference expression.
    fn set_track_focus(&mut self, attribute: &Attribute) -> Result<()> {
        if self.track_focus.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate :track-focus binding",
            ));
        }
        self.track_focus = Some(expect_attribute_expression(attribute, ":track-focus")?);
        Ok(())
    }

    /// Stores a static or dynamic GPUI keyboard context.
    fn set_key_context(&mut self, attribute: &Attribute) -> Result<()> {
        if self.key_context.is_some() {
            return Err(syn::Error::new(
                attribute.span,
                "duplicate key-context binding",
            ));
        }
        self.key_context = Some(if attribute.name == ":key-context" {
            let expression = expect_attribute_expression(attribute, ":key-context")?;
            quote!(#expression)
        } else {
            match &attribute.value {
                Some(AttributeValue::String(value)) => quote!(#value),
                _ => {
                    return Err(syn::Error::new(
                        attribute.span,
                        "key-context must be a static string literal; use :key-context={...} for a Rust expression",
                    ));
                }
            }
        });
        Ok(())
    }

    /// Parses one native host event and rejects duplicate event kinds.
    fn set_event(&mut self, attribute: &Attribute, source_index: usize) -> Result<()> {
        let event = EventBinding::parse(attribute, source_index)?;
        if self
            .events
            .iter()
            .any(|existing| existing.kind == event.kind)
            && !event.kind.allows_multiple()
        {
            return Err(syn::Error::new(
                attribute.span,
                format!("duplicate {} handler", event.kind.diagnostic_name()),
            ));
        }
        if event.kind.requires_focus() {
            self.focusable = true;
        }
        self.events.push(event);
        Ok(())
    }

    /// Reports whether this host needs source-ordered interaction/content lowering.
    fn has_ordered_host_bindings(&self) -> bool {
        self.text_content.is_some()
            || self.drag_payload.is_some()
            || self.drag_preview.is_some()
            || self.can_drop.is_some()
            || !self.drag_over.is_empty()
            || self
                .events
                .iter()
                .any(|event| matches!(event.kind, EventKind::DragMove | EventKind::Drop))
    }
}

impl ComponentBindings {
    /// Collects the host-input surface for a component tag.
    fn parse(element: &Element) -> Result<Self> {
        if element.directive_show.is_some() {
            return Err(syn::Error::new(
                element.tag.span(),
                "v-show is not supported on PascalCase components because the native host adds no layout wrapper",
            ));
        }

        let mut complete_props = None;
        let mut individual_props = Vec::new();
        let mut seen_individual_props = Vec::new();
        let mut explicit_slots = None;
        let mut key = None;
        let mut has_bound_key = false;
        let mut events = Vec::new();
        let mut seen_events = Vec::new();
        for attribute in &element.attributes {
            Self::reject_intrinsic_only_directive(attribute)?;
            match attribute.name.as_str() {
                ":props" => {
                    if complete_props.is_some() {
                        return Err(syn::Error::new(attribute.span, "duplicate :props binding"));
                    }
                    if !individual_props.is_empty() {
                        return Err(Self::mixed_props_error(attribute.span));
                    }
                    complete_props = Some(expect_attribute_expression(attribute, ":props")?);
                }
                ":slots" => {
                    if explicit_slots.is_some() {
                        return Err(syn::Error::new(attribute.span, "duplicate :slots binding"));
                    }
                    explicit_slots = Some(expect_attribute_expression(attribute, ":slots")?);
                }
                "key" => {
                    if key.is_some() {
                        return Err(syn::Error::new(attribute.span, "duplicate key attribute"));
                    }
                    key = Some(value_tokens(attribute)?);
                }
                ":key" => {
                    if key.is_some() {
                        return Err(syn::Error::new(attribute.span, "duplicate key attribute"));
                    }
                    let expression = expect_bound_expression(attribute, "key")?;
                    if matches!(&expression, Expr::Lit(_)) {
                        return Err(syn::Error::new(
                            attribute.span,
                            ":key must derive a unique identity from the v-for item, not a literal",
                        ));
                    }
                    key = Some(quote!(#expression));
                    has_bound_key = true;
                }
                name if name.starts_with('@') || name.starts_with("on:") => {
                    Self::push_event(attribute, &mut events, &mut seen_events)?;
                }
                _ => {
                    if complete_props.is_some() {
                        return Err(Self::mixed_props_error(attribute.span));
                    }
                    let binding = ComponentPropBinding::parse(attribute)?;
                    let canonical = binding.method.unraw().to_string();
                    if seen_individual_props.contains(&canonical) {
                        return Err(syn::Error::new(
                            attribute.span,
                            format!("duplicate component prop `{canonical}`"),
                        ));
                    }
                    seen_individual_props.push(canonical);
                    individual_props.push(binding);
                }
            }
        }

        if element.directive_for.is_some() && !has_bound_key {
            return Err(syn::Error::new(
                element.tag.span(),
                "every v-for root requires a dynamic `:key={...}` to namespace GPUI element state",
            ));
        }
        let props = complete_props.map_or_else(
            || ComponentPropsBinding::Individual(individual_props),
            ComponentPropsBinding::Complete,
        );
        if explicit_slots.is_some() && !element.children.is_empty() {
            return Err(syn::Error::new(
                element.tag.span(),
                "`:slots={...}` cannot be mixed with component children or named slot templates",
            ));
        }
        let declarative_slots = Self::parse_declarative_slots(element)?;
        let slots = explicit_slots
            .map(ComponentSlotsBinding::Explicit)
            .or(declarative_slots);

        Ok(Self {
            props,
            slots,
            key,
            events,
        })
    }

    /// Rejects bindings whose semantics require a real GPUI parent intrinsic.
    fn reject_intrinsic_only_directive(attribute: &Attribute) -> Result<()> {
        if attribute.name == "v-text" {
            return Err(syn::Error::new(
                attribute.span,
                "v-text is available only on parent intrinsics, not PascalCase components",
            ));
        }
        Ok(())
    }

    /// Classifies component children into one implicit default and named providers.
    fn parse_declarative_slots(element: &Element) -> Result<Option<ComponentSlotsBinding>> {
        if element.children.is_empty() {
            return Ok(None);
        }

        let mut providers = Vec::new();
        let mut default_roots = Vec::new();
        let mut seen = Vec::new();
        for node in &element.children {
            let Node::Element(child) = node else {
                default_roots.push(node.clone());
                continue;
            };
            let Some(provider) = Self::parse_named_slot_template(child)? else {
                default_roots.push(node.clone());
                continue;
            };
            if seen.contains(&provider.canonical) {
                return Err(syn::Error::new(
                    provider.span,
                    format!("duplicate component slot `{}`", provider.canonical),
                ));
            }
            seen.push(provider.canonical.clone());
            providers.push(provider);
        }

        if !default_roots.is_empty() {
            if seen.iter().any(|name| name == "default") {
                return Err(syn::Error::new(
                    element.tag.span(),
                    "the default slot cannot use both ordinary component children and `<template #default>`",
                ));
            }
            providers.insert(
                0,
                ComponentSlotBinding {
                    canonical: "default".to_owned(),
                    setter: Ident::new("with_default", element.tag.span()),
                    pattern: None,
                    roots: default_roots,
                    span: element.tag.span(),
                },
            );
        }

        Ok(Some(ComponentSlotsBinding::Declarative(providers)))
    }

    /// Parses a direct `<template #name={pattern}>` child when present.
    fn parse_named_slot_template(element: &Element) -> Result<Option<ComponentSlotBinding>> {
        if element.tag != "template" {
            return Ok(None);
        }
        let slot_attributes = element
            .attributes
            .iter()
            .filter(|attribute| attribute.name.starts_with('#'))
            .collect::<Vec<_>>();
        if slot_attributes.is_empty() {
            return Ok(None);
        }
        let attribute = slot_attributes[0];
        if slot_attributes.len() != 1 || element.attributes.len() != 1 {
            return Err(syn::Error::new(
                attribute.span,
                "a named slot template accepts exactly one `#name` binding and no other attributes",
            ));
        }
        if element.conditional.is_some()
            || element.directive_for.is_some()
            || element.directive_show.is_some()
        {
            return Err(syn::Error::new(
                element.tag.span(),
                "structural directives are not yet supported on named slot templates",
            ));
        }
        if element.children.is_empty() {
            return Err(syn::Error::new(
                element.tag.span(),
                "a named slot provider must contain at least one child node",
            ));
        }

        let source_name = attribute
            .name
            .strip_prefix('#')
            .expect("slot attributes were filtered by their prefix");
        let canonical = source_name.replace('-', "_");
        let setter = format_ident!("with_{canonical}", span = attribute.span);
        let pattern = match &attribute.value {
            None => None,
            Some(AttributeValue::Pattern(pattern)) => Some(pattern.clone()),
            Some(AttributeValue::Expression(_) | AttributeValue::String(_)) => {
                return Err(syn::Error::new(
                    attribute.span,
                    "slot props must be a Rust pattern in `{ ... }`",
                ));
            }
        };

        Ok(Some(ComponentSlotBinding {
            canonical,
            setter,
            pattern,
            roots: element.children.clone(),
            span: attribute.span,
        }))
    }

    /// Reports an attempt to combine a complete props value with setter syntax.
    fn mixed_props_error(span: Span) -> syn::Error {
        syn::Error::new(
            span,
            "`:props={...}` cannot be mixed with individual component props",
        )
    }

    /// Adds one canonical event binding while preserving targeted duplicates.
    fn push_event(
        attribute: &Attribute,
        events: &mut Vec<ComponentEventBinding>,
        seen_events: &mut Vec<String>,
    ) -> Result<()> {
        let binding = ComponentEventBinding::parse(attribute)?;
        if seen_events.contains(&binding.canonical) {
            return Err(syn::Error::new(
                attribute.span,
                format!("duplicate component event `{}`", binding.canonical),
            ));
        }
        seen_events.push(binding.canonical.clone());
        events.push(binding);
        Ok(())
    }
}

impl SlotOutletBinding {
    /// Validates one `<slot>` against its receiving component's declarations.
    fn parse(element: &Element, context: Option<&ComponentTemplateContext>) -> Result<Self> {
        let Some(context) = context else {
            return Err(syn::Error::new(
                element.tag.span(),
                "<slot> is available only in direct component markup; use `template(this, window, cx) { <slot /> }` or invoke a typed slot explicitly",
            ));
        };
        Self::validate_structure(element)?;
        let attributes = Self::parse_attributes(element)?;
        let (source_name, name_span) = attributes
            .source_name
            .unwrap_or_else(|| ("default".to_owned(), element.tag.span()));
        let canonical = source_name.replace('-', "_");
        let Some(metadata) = context.slot(&canonical) else {
            let message = if context.slots.is_empty() {
                "this component does not declare any slots".to_owned()
            } else {
                format!("this component does not declare a `{source_name}` slot")
            };
            return Err(syn::Error::new(name_span, message));
        };
        let (props, props_span) = match attributes.props {
            Some((props, span)) => (props, span),
            None if metadata.accepts_implicit_unit() => {
                (syn::parse_quote_spanned!(name_span=> ()), name_span)
            }
            None => {
                return Err(syn::Error::new(
                    name_span,
                    format!(
                        "slot `{source_name}` has non-unit props and requires `:props={{...}}`"
                    ),
                ));
            }
        };

        Ok(Self {
            field: metadata.name.clone(),
            props,
            fallback: element.children.clone(),
            span: props_span,
        })
    }

    /// Rejects structural directives that require a rendered or keyed host.
    fn validate_structure(element: &Element) -> Result<()> {
        if element.conditional.is_some() {
            return Err(syn::Error::new(
                element.tag.span(),
                "structural conditions are not supported directly on <slot>; wrap the outlet in `<template v-if={...}>`",
            ));
        }
        if element.directive_for.is_some() {
            return Err(syn::Error::new(
                element.tag.span(),
                "v-for is not supported on <slot> because a wrapper-free repeated slot has no GPUI fragment identity",
            ));
        }
        if element.directive_show.is_some() {
            return Err(syn::Error::new(
                element.tag.span(),
                "v-show is not supported on <slot> because an outlet has no rendered host element",
            ));
        }
        Ok(())
    }

    /// Parses the static name and optional typed props expression.
    fn parse_attributes(element: &Element) -> Result<SlotOutletAttributes> {
        let mut source_name = None;
        let mut props = None;
        for attribute in &element.attributes {
            match attribute.name.as_str() {
                "v-text" => {
                    return Err(syn::Error::new(
                        attribute.span,
                        "v-text requires a parent intrinsic and cannot be used on <slot>",
                    ));
                }
                "name" => {
                    if source_name.is_some() {
                        return Err(syn::Error::new(
                            attribute.span,
                            "duplicate slot outlet name",
                        ));
                    }
                    let Some(AttributeValue::String(name)) = &attribute.value else {
                        return Err(syn::Error::new(
                            attribute.span,
                            "slot outlet `name` must be a static string literal",
                        ));
                    };
                    source_name = Some((name.value(), attribute.span));
                }
                ":props" => {
                    if props.is_some() {
                        return Err(syn::Error::new(
                            attribute.span,
                            "duplicate slot outlet :props binding",
                        ));
                    }
                    props = Some((
                        expect_attribute_expression(attribute, ":props")?,
                        attribute.span,
                    ));
                }
                ":name" => {
                    return Err(syn::Error::new(
                        attribute.span,
                        "dynamic slot outlet names are not supported because slots have heterogeneous Rust props types",
                    ));
                }
                "props" => {
                    return Err(syn::Error::new(
                        attribute.span,
                        "slot outlet props require a bound Rust expression: `:props={...}`",
                    ));
                }
                unsupported => {
                    return Err(syn::Error::new(
                        attribute.span,
                        format!(
                            "unsupported <slot> attribute `{unsupported}`; use only a static `name` and `:props={{...}}`"
                        ),
                    ));
                }
            }
        }
        Ok(SlotOutletAttributes { source_name, props })
    }
}

/// Conservatively rejects repeated outlets that could collide in GPUI state identity.
fn validate_unique_slot_outlets(nodes: &[Node], context: &ComponentTemplateContext) -> Result<()> {
    fn visit(
        nodes: &[Node],
        context: &ComponentTemplateContext,
        seen: &mut HashSet<String>,
    ) -> Result<()> {
        for node in nodes {
            match node {
                Node::Element(element) => {
                    if element.tag == "slot" {
                        let binding = SlotOutletBinding::parse(element, Some(context))?;
                        let name = binding.field.unraw().to_string();
                        if !seen.insert(name.clone()) {
                            return Err(syn::Error::new(
                                element.tag.span(),
                                format!(
                                    "slot `{name}` has more than one outlet in this component template; the current zero-wrapper outlet lane cannot assign distinct GPUI identity to repeated provider content"
                                ),
                            ));
                        }
                    }
                    visit(&element.children, context, seen)?;
                }
                Node::Fragment(children) => visit(children, context, seen)?,
                Node::Expression(_) | Node::Text(_) => {}
            }
        }
        Ok(())
    }

    visit(nodes, context, &mut HashSet::new())
}

impl ComponentPropBinding {
    /// Normalizes one Vue-shaped prop attribute into an exact Rust setter call.
    fn parse(attribute: &Attribute) -> Result<Self> {
        let (source_name, bound) = attribute
            .name
            .strip_prefix(':')
            .map_or((attribute.name.as_str(), false), |name| (name, true));
        let method = component_prop_ident(source_name, attribute.span)?;
        let value = if bound {
            match &attribute.value {
                Some(AttributeValue::Expression(expression)) => expression.clone(),
                None => {
                    let shorthand = method.clone();
                    syn::parse_quote_spanned!(attribute.span=> #shorthand)
                }
                Some(AttributeValue::String(_) | AttributeValue::Pattern(_)) => {
                    return Err(syn::Error::new(
                        attribute.span,
                        format!(
                            ":{source_name} must be a Rust expression in `{{ ... }}` or the same-name shorthand"
                        ),
                    ));
                }
            }
        } else {
            match &attribute.value {
                Some(AttributeValue::Expression(expression)) => expression.clone(),
                Some(AttributeValue::String(value)) => {
                    syn::parse_quote_spanned!(attribute.span=> #value)
                }
                None => syn::parse_quote_spanned!(attribute.span=> true),
                Some(AttributeValue::Pattern(_)) => {
                    return Err(syn::Error::new(
                        attribute.span,
                        "`#name={pattern}` is valid only on a direct `<template>` component child",
                    ));
                }
            }
        };

        Ok(Self {
            method,
            value,
            span: attribute.span,
        })
    }
}

impl ComponentEventBinding {
    /// Parses one modifier-free Vue event spelling into a typed enum dispatcher.
    fn parse(attribute: &Attribute) -> Result<Self> {
        let event_with_modifiers = attribute
            .name
            .strip_prefix('@')
            .or_else(|| attribute.name.strip_prefix("on:"))
            .ok_or_else(|| syn::Error::new(attribute.span, "invalid component event binding"))?;
        let mut parts = event_with_modifiers.split('.');
        let source_name = parts.next().unwrap_or_default();
        if let Some(modifier) = parts.next() {
            return Err(syn::Error::new(
                attribute.span,
                format!(
                    "component event modifiers are not supported in this stage; remove `.{modifier}`"
                ),
            ));
        }

        let canonical = source_name.replace('-', "_");
        let dispatcher_name = format!("__gpui_vue_dispatch_{canonical}");
        let mut dispatcher = syn::parse_str::<Ident>(&dispatcher_name).map_err(|_| {
            syn::Error::new(
                attribute.span,
                format!(
                    "component event `{source_name}` does not normalize to a legal Rust identifier"
                ),
            )
        })?;
        dispatcher.set_span(attribute.span);
        let handler = expect_attribute_expression(attribute, &attribute.name)?;

        Ok(Self {
            canonical,
            dispatcher,
            handler,
            span: attribute.span,
        })
    }
}

/// Converts kebab-case source syntax to a legal snake-case Rust method name.
fn component_prop_ident(source_name: &str, span: Span) -> Result<Ident> {
    let normalized = source_name.replace('-', "_");
    keyword_aware_ident(&normalized, span).ok_or_else(|| {
        syn::Error::new(
            span,
            format!("component prop `{source_name}` does not normalize to a legal Rust identifier"),
        )
    })
}

/// Parses an ordinary identifier or its safe raw-keyword spelling without panicking.
fn keyword_aware_ident(normalized: &str, span: Span) -> Option<Ident> {
    // Rust reserves these path/self keywords even in `r#...` form. Checking them
    // before constructing a raw identifier also avoids `Ident::new_raw` panics.
    if matches!(normalized, "self" | "Self" | "super" | "crate") {
        return None;
    }

    let raw = format!("r#{normalized}");
    let mut identifier = syn::parse_str::<Ident>(normalized)
        .or_else(|_| syn::parse_str::<Ident>(&raw))
        .ok()?;
    identifier.set_span(span);
    Some(identifier)
}

impl DynamicClasses {
    /// Parses a literal or nested Rust `if` expression with an optional static base.
    fn parse(expression: &Expr, span: Span, base: Option<&LitStr>) -> Result<Self> {
        match expression {
            Expr::Lit(literal) => match &literal.lit {
                Lit::Str(classes) => Self::parse_literal(classes, base),
                _ => Err(Self::unsupported_error(span)),
            },
            Expr::If(expression_if) => {
                let then_expression = single_block_expression(&expression_if.then_branch)?;
                let then_branch = Box::new(Self::parse(then_expression, span, base)?);
                let else_branch = if let Some((_, expression)) = &expression_if.else_branch {
                    Some(Box::new(Self::parse(expression, span, base)?))
                } else {
                    base.map(Self::parse_static_literal)
                        .transpose()?
                        .map(Box::new)
                };
                Ok(Self::Conditional {
                    condition: (*expression_if.cond).clone(),
                    then_branch,
                    else_branch,
                })
            }
            Expr::Block(block) => Self::parse(single_block_expression(&block.block)?, span, base),
            _ => Err(Self::unsupported_error(span)),
        }
    }

    /// Compiles one dynamic literal after prepending the element's static classes.
    fn parse_literal(literal: &LitStr, base: Option<&LitStr>) -> Result<Self> {
        let Some(base) = base else {
            return CompiledClasses::parse(literal).map(Self::Literal);
        };

        let base_value = base.value();
        let dynamic_value = literal.value();
        let merged_value = match (base_value.is_empty(), dynamic_value.is_empty()) {
            (true, _) => dynamic_value,
            (_, true) => base_value,
            (false, false) => format!("{base_value} {dynamic_value}"),
        };
        let merged = LitStr::new(&merged_value, literal.span());
        CompiledClasses::parse(&merged).map(Self::Literal)
    }

    /// Compiles the static-only fallback for a condition without an `else` branch.
    fn parse_static_literal(literal: &LitStr) -> Result<Self> {
        CompiledClasses::parse(literal).map(Self::Literal)
    }

    /// Builds the diagnostic for runtime class strings and unsupported shapes.
    fn unsupported_error(span: Span) -> syn::Error {
        syn::Error::new(
            span,
            ":class must be a string literal or `if condition { \"literal classes\" } else { \"literal classes\" }`; runtime class strings are intentionally unsupported",
        )
    }

    /// Reports whether any branch needs a stateful GPUI id.
    fn needs_stateful_id(&self) -> bool {
        match self {
            Self::Literal(classes) => classes.needs_stateful_id(),
            Self::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                then_branch.needs_stateful_id()
                    || else_branch.as_deref().is_some_and(Self::needs_stateful_id)
            }
        }
    }

    /// Reports whether any branch applies focus styling.
    fn needs_focusable(&self) -> bool {
        match self {
            Self::Literal(classes) => classes.needs_focusable(),
            Self::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                then_branch.needs_focusable()
                    || else_branch.as_deref().is_some_and(Self::needs_focusable)
            }
        }
    }

    /// Applies ordinary and interaction styles in one conditional-tree traversal.
    fn apply(&self, target: &TokenStream, crate_path: &TokenStream) -> TokenStream {
        match self {
            Self::Literal(classes) => {
                let output = classes.apply_regular(target.clone(), crate_path);
                classes.apply_variants(output, crate_path)
            }
            Self::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                let then_tokens = then_branch.apply(&quote!(__gpui_vue_element), crate_path);
                if let Some(else_branch) = else_branch {
                    let else_tokens = else_branch.apply(&quote!(__gpui_vue_element), crate_path);
                    quote!(#target.when_else(
                        #condition,
                        |__gpui_vue_element| #then_tokens,
                        |__gpui_vue_element| #else_tokens,
                    ))
                } else {
                    quote!(#target.when(
                        #condition,
                        |__gpui_vue_element| #then_tokens,
                    ))
                }
            }
        }
    }
}

/// Returns the sole trailing expression from a dynamic-class block.
fn single_block_expression(block: &syn::Block) -> Result<&Expr> {
    match block.stmts.as_slice() {
        [Stmt::Expr(expression, None)] => Ok(expression),
        _ => Err(syn::Error::new(
            block.brace_token.span.join(),
            "a :class branch must contain exactly one literal or nested if expression",
        )),
    }
}

impl EventBinding {
    /// Parses one intrinsic event and validates its event-specific modifiers.
    fn parse(attribute: &Attribute, source_index: usize) -> Result<Self> {
        let event_with_modifiers = attribute
            .name
            .strip_prefix('@')
            .or_else(|| attribute.name.strip_prefix("on:"))
            .ok_or_else(|| syn::Error::new(attribute.span, "invalid event binding"))?;
        let mut parts = event_with_modifiers.split('.');
        let event = parts.next().unwrap_or_default();
        let modifier_names = parts.collect::<Vec<_>>();
        let (kind, modifiers) = EventKind::parse(event, &modifier_names, attribute.span)?;
        let handler_label = format!("{event} handler");
        Ok(Self {
            kind,
            handler: expect_attribute_expression(attribute, &handler_label)?,
            modifiers,
            source_index,
            span: attribute.span,
        })
    }

    /// Rejects modifiers on native events whose GPUI listener has no modifier lane.
    fn reject_modifiers(event: &str, modifiers: &[&str], span: Span) -> Result<()> {
        if let Some(modifier) = modifiers.first() {
            return Err(syn::Error::new(
                span,
                format!("@{event} does not support modifier `.{modifier}`"),
            ));
        }
        Ok(())
    }

    /// Emits either the original listener or a zero-allocation modifier wrapper.
    fn listener_tokens(&self, crate_path: &TokenStream) -> TokenStream {
        let handler = &self.handler;
        if self.modifiers.is_empty() {
            return quote!(#handler);
        }

        let statements = self
            .modifiers
            .iter()
            .map(|modifier| modifier.statement(&self.modifiers));
        quote!({
            let __gpui_vue_handler = #crate_path::ui::type_click_handler(#handler);
            move |
                __gpui_vue_event: &#crate_path::gpui::ClickEvent,
                __gpui_vue_window: &mut #crate_path::gpui::Window,
                __gpui_vue_cx: &mut #crate_path::gpui::App,
            | {
                #(#statements)*
                (__gpui_vue_handler)(
                    __gpui_vue_event,
                    __gpui_vue_window,
                    __gpui_vue_cx,
                );
            }
        })
    }

    /// Appends this listener through its exact native GPUI builder method.
    fn apply(&self, target: &TokenStream, crate_path: &TokenStream) -> TokenStream {
        let listener = self.listener_tokens(crate_path);
        match self.kind {
            EventKind::Click => quote!(#target.on_click(#listener)),
            EventKind::KeyDown => quote!(#target.on_key_down(#listener)),
            EventKind::KeyUp => quote!(#target.on_key_up(#listener)),
            EventKind::ModifiersChanged => quote!(#target.on_modifiers_changed(#listener)),
            EventKind::MouseDown(button) => {
                let button = button.tokens(crate_path);
                quote!(#target.on_mouse_down(#button, #listener))
            }
            EventKind::MouseDownOut(button) => {
                let button = button.tokens(crate_path);
                let handler = &self.handler;
                quote!(#target.on_mouse_down_out({
                    let __gpui_vue_handler =
                        #crate_path::ui::type_mouse_down_handler(#handler);
                    move |
                        __gpui_vue_event: &#crate_path::gpui::MouseDownEvent,
                        __gpui_vue_window: &mut #crate_path::gpui::Window,
                        __gpui_vue_cx: &mut #crate_path::gpui::App,
                    | {
                        if __gpui_vue_event.button == #button {
                            (__gpui_vue_handler)(
                                __gpui_vue_event,
                                __gpui_vue_window,
                                __gpui_vue_cx,
                            );
                        }
                    }
                }))
            }
            EventKind::MouseMove => quote!(#target.on_mouse_move(#listener)),
            EventKind::DragMove => quote!(#target.on_drag_move(#listener)),
            EventKind::MouseUp(button) => {
                let button = button.tokens(crate_path);
                quote!(#target.on_mouse_up(#button, #listener))
            }
            EventKind::MouseUpOut(button) => {
                let button = button.tokens(crate_path);
                quote!(#target.on_mouse_up_out(#button, #listener))
            }
            EventKind::Pinch => quote!(#target.on_pinch(#listener)),
            EventKind::ScrollWheel => quote!(#target.on_scroll_wheel(#listener)),
            EventKind::Hover => quote!(#target.on_hover(#listener)),
            EventKind::Drop => quote!(#target.on_drop(#listener)),
            EventKind::Focus => {
                let handler = &self.handler;
                quote!({
                    let __gpui_vue_element = #target;
                    __gpui_vue_on_focus = ::core::option::Option::Some(
                        #crate_path::ui::boxed_focus_handler(#handler),
                    );
                    __gpui_vue_element
                })
            }
            EventKind::Blur => {
                let handler = &self.handler;
                quote!({
                    let __gpui_vue_element = #target;
                    __gpui_vue_on_blur = ::core::option::Option::Some(
                        #crate_path::ui::boxed_focus_handler(#handler),
                    );
                    __gpui_vue_element
                })
            }
        }
    }
}

impl EventKind {
    /// Parses one canonical native event plus its event-specific modifiers.
    fn parse(
        event: &str,
        modifier_names: &[&str],
        span: Span,
    ) -> Result<(Self, Vec<EventModifier>)> {
        let no_modifiers = |kind| {
            EventBinding::reject_modifiers(event, modifier_names, span)?;
            Ok((kind, Vec::new()))
        };
        let button = || MouseButtonBinding::parse(event, modifier_names, span);
        match event {
            "click" => {
                let mut modifiers = Vec::new();
                for name in modifier_names {
                    let modifier = EventModifier::parse(name, span)?;
                    if modifiers.contains(&modifier) {
                        return Err(syn::Error::new(
                            span,
                            format!("duplicate event modifier `.{name}`"),
                        ));
                    }
                    modifiers.push(modifier);
                }
                Ok((Self::Click, modifiers))
            }
            "key-down" => no_modifiers(Self::KeyDown),
            "key-up" => no_modifiers(Self::KeyUp),
            "modifiers-changed" => no_modifiers(Self::ModifiersChanged),
            "mouse-down" => Ok((Self::MouseDown(button()?), Vec::new())),
            "mouse-down-out" => Ok((Self::MouseDownOut(button()?), Vec::new())),
            "mouse-move" => no_modifiers(Self::MouseMove),
            "drag-move" => no_modifiers(Self::DragMove),
            "mouse-up" => Ok((Self::MouseUp(button()?), Vec::new())),
            "mouse-up-out" => Ok((Self::MouseUpOut(button()?), Vec::new())),
            "pinch" => no_modifiers(Self::Pinch),
            "scroll-wheel" => no_modifiers(Self::ScrollWheel),
            "hover" => no_modifiers(Self::Hover),
            "drop" => no_modifiers(Self::Drop),
            "focus" => no_modifiers(Self::Focus),
            "blur" => no_modifiers(Self::Blur),
            unsupported => Err(syn::Error::new(
                span,
                format!(
                    "unsupported event `{unsupported}`; supported intrinsic events: click, key-down, key-up, modifiers-changed, mouse-down, mouse-down-out, mouse-move, drag-move, mouse-up, mouse-up-out, pinch, scroll-wheel, hover, drop, focus, blur"
                ),
            )),
        }
    }

    /// Returns the fully qualified event name used by duplicate diagnostics.
    const fn diagnostic_name(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::KeyDown => "key-down",
            Self::KeyUp => "key-up",
            Self::ModifiersChanged => "modifiers-changed",
            Self::MouseDown(MouseButtonBinding::Left) => "mouse-down.left",
            Self::MouseDown(MouseButtonBinding::Right) => "mouse-down.right",
            Self::MouseDown(MouseButtonBinding::Middle) => "mouse-down.middle",
            Self::MouseDownOut(MouseButtonBinding::Left) => "mouse-down-out.left",
            Self::MouseDownOut(MouseButtonBinding::Right) => "mouse-down-out.right",
            Self::MouseDownOut(MouseButtonBinding::Middle) => "mouse-down-out.middle",
            Self::MouseMove => "mouse-move",
            Self::DragMove => "drag-move",
            Self::MouseUp(MouseButtonBinding::Left) => "mouse-up.left",
            Self::MouseUp(MouseButtonBinding::Right) => "mouse-up.right",
            Self::MouseUp(MouseButtonBinding::Middle) => "mouse-up.middle",
            Self::MouseUpOut(MouseButtonBinding::Left) => "mouse-up-out.left",
            Self::MouseUpOut(MouseButtonBinding::Right) => "mouse-up-out.right",
            Self::MouseUpOut(MouseButtonBinding::Middle) => "mouse-up-out.middle",
            Self::Pinch => "pinch",
            Self::ScrollWheel => "scroll-wheel",
            Self::Hover => "hover",
            Self::Drop => "drop",
            Self::Focus => "focus",
            Self::Blur => "blur",
        }
    }

    /// Reports whether the host must enter GPUI's focus dispatch path.
    const fn requires_focus(self) -> bool {
        matches!(self, Self::KeyDown | Self::KeyUp | Self::ModifiersChanged)
    }

    /// Reports whether GPUI retains independent exact-payload-type lanes.
    const fn allows_multiple(self) -> bool {
        matches!(self, Self::DragMove | Self::Drop)
    }

    /// Reports whether this event uses gpui-vue's exact-focus subscription bridge.
    const fn is_focus_lifecycle(self) -> bool {
        matches!(self, Self::Focus | Self::Blur)
    }
}

impl MouseButtonBinding {
    /// Parses the one mandatory mouse-button modifier for button-filtered events.
    fn parse(event: &str, modifiers: &[&str], span: Span) -> Result<Self> {
        if modifiers.len() != 1 {
            return Err(syn::Error::new(
                span,
                format!(
                    "@{event} requires exactly one mouse button modifier: .left, .right, or .middle"
                ),
            ));
        }
        match modifiers[0] {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "middle" => Ok(Self::Middle),
            unsupported => Err(syn::Error::new(
                span,
                format!(
                    "unsupported @{event} modifier `.{unsupported}`; expected .left, .right, or .middle"
                ),
            )),
        }
    }

    /// Emits one fully qualified native GPUI mouse-button value.
    fn tokens(self, crate_path: &TokenStream) -> TokenStream {
        match self {
            Self::Left => quote!(#crate_path::gpui::MouseButton::Left),
            Self::Right => quote!(#crate_path::gpui::MouseButton::Right),
            Self::Middle => quote!(#crate_path::gpui::MouseButton::Middle),
        }
    }
}

impl EventModifier {
    /// Parses one GPUI-compatible click modifier.
    fn parse(name: &str, span: Span) -> Result<Self> {
        match name {
            "stop" => Ok(Self::Stop),
            "prevent" => Ok(Self::Prevent),
            "ctrl" => Ok(Self::Control),
            "alt" => Ok(Self::Alt),
            "shift" => Ok(Self::Shift),
            "meta" => Ok(Self::Meta),
            "exact" => Ok(Self::Exact),
            "passive" => Err(syn::Error::new(
                span,
                ".passive configures DOM listeners and has no GPUI equivalent",
            )),
            unsupported => Err(syn::Error::new(
                span,
                format!(
                    "unsupported click modifier `.{unsupported}`; supported: stop, prevent, ctrl, alt, shift, meta, exact"
                ),
            )),
        }
    }

    /// Emits the ordered guard or side effect for this modifier.
    fn statement(self, required: &[Self]) -> TokenStream {
        match self {
            Self::Stop => quote!(__gpui_vue_cx.stop_propagation();),
            Self::Prevent => quote!(__gpui_vue_window.prevent_default();),
            Self::Control => quote!(if !__gpui_vue_event.modifiers().control {
                return;
            }),
            Self::Alt => quote!(if !__gpui_vue_event.modifiers().alt {
                return;
            }),
            Self::Shift => quote!(if !__gpui_vue_event.modifiers().shift {
                return;
            }),
            Self::Meta => quote!(if !__gpui_vue_event.modifiers().platform {
                return;
            }),
            Self::Exact => {
                let control = required.contains(&Self::Control);
                let alt = required.contains(&Self::Alt);
                let shift = required.contains(&Self::Shift);
                let meta = required.contains(&Self::Meta);
                quote!({
                    let __gpui_vue_modifiers = __gpui_vue_event.modifiers();
                    if (!#control && __gpui_vue_modifiers.control)
                        || (!#alt && __gpui_vue_modifiers.alt)
                        || (!#shift && __gpui_vue_modifiers.shift)
                        || (!#meta && __gpui_vue_modifiers.platform)
                        || __gpui_vue_modifiers.function
                    {
                        return;
                    }
                })
            }
        }
    }
}

/// Lowers the optional provider result shared by root and nested slot outlets.
fn slot_outlet_option_tokens(
    binding: &SlotOutletBinding,
    crate_path: &TokenStream,
    context: &ComponentTemplateContext,
) -> TokenStream {
    let this = &context.this;
    let window = &context.window;
    let component_context = &context.context;
    let field = &binding.field;
    let props = &binding.props;
    let outlet = format_ident!("__gpui_vue_slot_outlet", span = Span::mixed_site());
    let slot_props = format_ident!("__gpui_vue_slot_outlet_props", span = Span::mixed_site());

    quote_spanned! {binding.span=> {
        let #outlet = if
            <Self as #crate_path::NativeComponentSlots>::slots(&*#this)
                .#field
                .is_present()
        {
            let #slot_props = #props;
            <Self as #crate_path::NativeComponentSlots>::slots(&*#this)
                .#field
                .render(
                    #slot_props,
                    &mut *#window,
                    &mut *#component_context,
                )
        } else {
            ::core::option::Option::None
        };
        #outlet
    }}
}

/// Lowers one sole Render-root outlet, using GPUI's empty element when absent.
fn expand_root_slot_outlet(
    element: &Element,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    let binding = SlotOutletBinding::parse(element, context)?;
    let context = context.expect("slot outlet parsing requires component context");
    let optional = slot_outlet_option_tokens(&binding, crate_path, context);
    let rendered_ident = format_ident!("__gpui_vue_slot_content", span = Span::mixed_site());
    let fallback = if binding.fallback.is_empty() {
        quote!(#crate_path::SlotContent::new(#crate_path::gpui::Empty))
    } else {
        let fallback = expand_roots(&binding.fallback, crate_path, Some(context))?;
        quote!(#crate_path::SlotContent::new(#fallback))
    };

    Ok(quote_spanned! {binding.span=> {
        match #optional {
            ::core::option::Option::Some(#rendered_ident) => #rendered_ident,
            ::core::option::Option::None => #fallback,
        }
    }})
}

/// Appends one outlet without adding a child when an unprovided slot has no fallback.
fn append_slot_outlet(
    parent: &TokenStream,
    element: &Element,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    let binding = SlotOutletBinding::parse(element, context)?;
    let context = context.expect("slot outlet parsing requires component context");
    let optional = slot_outlet_option_tokens(&binding, crate_path, context);
    if binding.fallback.is_empty() {
        return Ok(quote!(#parent.children(#optional)));
    }

    let fallback = expand_roots(&binding.fallback, crate_path, Some(context))?;
    let rendered_ident = format_ident!("__gpui_vue_slot_content", span = Span::mixed_site());
    Ok(quote_spanned! {binding.span=>
        #parent.child(match #optional {
            ::core::option::Option::Some(#rendered_ident) => #rendered_ident,
            ::core::option::Option::None => #crate_path::SlotContent::new(#fallback),
        })
    })
}

/// Expands one or more roots, adding a documented host boundary when needed.
fn expand_roots(
    roots: &[Node],
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    if let [Node::Element(element)] = roots
        && element.conditional.is_none()
        && element.directive_for.is_none()
        && element.tag != "template"
    {
        return expand_element(element, crate_path, context);
    }

    // GPUI 0.2.2 requires one IntoElement at the Render boundary and has no
    // display:contents element. Nested fragments are flattened; only this
    // outer boundary receives a synthetic container.
    append_nodes(quote!(#crate_path::gpui::div()), roots, crate_path, context)
}

/// Dispatches an intrinsic or `PascalCase` tag to its native GPUI lowering.
fn expand_element(
    element: &Element,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    if is_component_tag(&element.tag) {
        return expand_component_element(element, crate_path, context);
    }

    if element.tag == "slot" {
        return expand_root_slot_outlet(element, crate_path, context);
    }
    let (is_button, is_image) = intrinsic_kind(element)?;
    let mut bindings = ElementBindings::parse(element, is_button, is_image)?;
    validate_image_source(element, &bindings, is_image)?;
    let classes = bindings
        .class
        .as_ref()
        .map(CompiledClasses::parse)
        .transpose()?;
    let dynamic_classes = bindings
        .dynamic_class
        .as_ref()
        .map(|binding| {
            DynamicClasses::parse(&binding.expression, binding.span, bindings.class.as_ref())
        })
        .transpose()?;
    if classes
        .as_ref()
        .is_some_and(CompiledClasses::needs_focusable)
        || dynamic_classes
            .as_ref()
            .is_some_and(DynamicClasses::needs_focusable)
    {
        bindings.focusable = true;
    }

    validate_element_identity(
        element,
        &bindings,
        classes.as_ref(),
        dynamic_classes.as_ref(),
    )?;
    let unconditional_classes = if dynamic_classes.is_none() {
        classes.as_ref()
    } else {
        None
    };
    let has_focus_lifecycle = bindings
        .events
        .iter()
        .any(|event| event.kind.is_focus_lifecycle());
    let focus_handle_local = has_focus_lifecycle.then(|| quote!(__gpui_vue_focus_handle));
    let (mut output, image_value_bindings) = if is_image {
        image_intrinsic_output(&bindings, crate_path)
    } else {
        (quote!(#crate_path::gpui::div()), Vec::new())
    };
    if is_button {
        output = quote!(#output.cursor_pointer());
    }
    if let Some(classes) = unconditional_classes {
        output = classes.apply_regular(output, crate_path);
    }
    output = apply_interactivity(
        output,
        &bindings,
        unconditional_classes,
        dynamic_classes.as_ref(),
        is_button,
        focus_handle_local.as_ref(),
        crate_path,
    );
    if let Some(show) = &element.directive_show {
        output = quote!(#output.when(!(#show), |__gpui_vue_element| {
            __gpui_vue_element.hidden()
        }));
    }
    if !is_image {
        output = append_nodes(output, &element.children, crate_path, context)?;
    }
    if has_focus_lifecycle {
        let track_focus = bindings
            .track_focus
            .as_ref()
            .expect("focus lifecycle validation requires :track-focus");
        let focus_binding = focus_handler_binding(&bindings, EventKind::Focus, crate_path);
        let blur_binding = focus_handler_binding(&bindings, EventKind::Blur, crate_path);
        output = apply_focus_lifecycle(&output, crate_path);
        output = quote!({
            let __gpui_vue_focus_handle = #track_focus;
            #focus_binding
            #blur_binding
            #output
        });
    }
    if is_image {
        output = quote!({
            #(#image_value_bindings)*
            #output
        });
    }
    Ok(output)
}

/// Requires one source on the native image intrinsic.
fn validate_image_source(
    element: &Element,
    bindings: &ElementBindings,
    is_image: bool,
) -> Result<()> {
    if is_image && bindings.image_source.is_none() {
        return Err(syn::Error::new(
            element.tag.span(),
            "the <img> intrinsic requires `src` or `:src`",
        ));
    }
    Ok(())
}

/// Builds one typed GPUI image and source-ordered locals for its attributes.
fn image_intrinsic_output(
    bindings: &ElementBindings,
    crate_path: &TokenStream,
) -> (TokenStream, Vec<TokenStream>) {
    let ordinal = bindings.element_ordinal;
    let source_local = format_ident!(
        "__gpui_vue_image_source_{ordinal}",
        span = Span::mixed_site()
    );
    let fit_local = format_ident!("__gpui_vue_image_fit_{ordinal}", span = Span::mixed_site());
    let loading_local = format_ident!(
        "__gpui_vue_image_loading_{ordinal}",
        span = Span::mixed_site()
    );
    let fallback_local = format_ident!(
        "__gpui_vue_image_fallback_{ordinal}",
        span = Span::mixed_site()
    );
    let source = bindings
        .image_source
        .as_ref()
        .expect("image source validation runs before lowering");
    let source_tokens = &source.tokens;
    let mut statements = vec![(
        source.source_index,
        quote_spanned! {source.span=> let #source_local = #source_tokens; },
    )];
    for (binding, local) in [
        (bindings.image_object_fit.as_ref(), &fit_local),
        (bindings.image_loading.as_ref(), &loading_local),
        (bindings.image_fallback.as_ref(), &fallback_local),
    ] {
        if let Some(binding) = binding {
            let expression = &binding.expression;
            statements.push((
                binding.source_index,
                quote_spanned! {binding.span=> let #local = #expression; },
            ));
        }
    }
    statements.sort_by_key(|(source_index, _)| *source_index);

    let mut output = quote!(#crate_path::ui::image(#source_local));
    if let Some(binding) = &bindings.image_object_fit {
        output = quote_spanned! {binding.span=>
            #crate_path::ui::image_object_fit(#output, #fit_local)
        };
    }
    if let Some(binding) = &bindings.image_loading {
        output = quote_spanned! {binding.span=>
            #crate_path::ui::image_loading(#output, #loading_local)
        };
    }
    if let Some(binding) = &bindings.image_fallback {
        output = quote_spanned! {binding.span=>
            #crate_path::ui::image_fallback(#output, #fallback_local)
        };
    }
    (
        output,
        statements
            .into_iter()
            .map(|(_, statement)| statement)
            .collect(),
    )
}

/// Validates one intrinsic tag and reports its host-specific flags.
fn intrinsic_kind(element: &Element) -> Result<(bool, bool)> {
    let tag = element.tag.to_string();
    if !matches!(
        tag.as_str(),
        "div" | "view" | "text" | "span" | "button" | "img"
    ) {
        return Err(syn::Error::new(
            element.tag.span(),
            format!(
                "unsupported intrinsic <{tag}>; use div/view/text/span/button/img, <template>, or insert a custom GPUI component with `{{ component }}`"
            ),
        ));
    }
    let is_image = tag == "img";
    if is_image && !element.children.is_empty() {
        return Err(syn::Error::new(
            element.tag.span(),
            "the <img> intrinsic cannot have children",
        ));
    }
    Ok((tag == "button", is_image))
}

/// Reports whether a simple Rust identifier uses the `PascalCase` component lane.
fn is_component_tag(tag: &Ident) -> bool {
    tag.unraw()
        .to_string()
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
}

/// Lowers one generated component directly to its persistent native host.
fn expand_component_element(
    element: &Element,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    let bindings = ComponentBindings::parse(element)?;
    let tag = &element.tag;
    let props = match &bindings.props {
        ComponentPropsBinding::Complete(props) => quote_spanned!(props.span()=> #props),
        ComponentPropsBinding::Individual(properties) => {
            let mut builder = quote_spanned! {element.tag.span()=>
                <#tag as #crate_path::NativeComponent>::Props::builder()
            };
            for property in properties {
                let method = &property.method;
                let value = &property.value;
                builder = quote_spanned!(property.span=> #builder.#method(#value));
            }
            quote_spanned!(element.tag.span()=> #builder.build())
        }
    };
    let input = component_input_tokens(tag, &props, bindings.slots.as_ref(), crate_path, context)?;

    let key = if let Some(key) = &bindings.key {
        quote!(::core::option::Option::Some(
            ::core::convert::Into::<#crate_path::gpui::ElementId>::into(#key)
        ))
    } else {
        quote!(::core::option::Option::None)
    };
    let ordinal = element.ordinal;
    let stable_slot = quote_spanned! {element.tag.span()=>
        #crate_path::gpui::ElementId::named_usize(
            ::core::concat!(
                ::core::module_path!(),
                ":",
                ::core::file!(),
                ":",
                ::core::line!(),
                ":",
                ::core::column!(),
            ),
            #ordinal,
        )
    };

    if bindings.events.is_empty() {
        return Ok(quote_spanned! {element.tag.span()=>
            #crate_path::component_element::<#tag, _, 0>(
                #stable_slot,
                #key,
                #input,
                |_, _, _| [],
            )
        });
    }

    Ok(expand_component_with_events(
        tag,
        &bindings,
        &stable_slot,
        &key,
        &input,
        crate_path,
    ))
}

/// Emits one live parent-scoped provider backed only by a weak entity handle.
fn contextual_slot_provider_tokens(
    provider: &ComponentSlotBinding,
    index: usize,
    provider_element: &TokenStream,
    crate_path: &TokenStream,
    context: &ComponentTemplateContext,
) -> TokenStream {
    let owner = format_ident!("__gpui_vue_slot_owner_{index}", span = Span::mixed_site());
    let update_this = format_ident!(
        "gpui_vue_internal_slot_this_{index}",
        span = Span::mixed_site()
    );
    let update_context = format_ident!(
        "gpui_vue_internal_slot_context_{index}",
        span = Span::mixed_site()
    );
    let rendered = format_ident!(
        "__gpui_vue_slot_rendered_{index}",
        span = Span::mixed_site()
    );
    let slot_props = format_ident!("__gpui_vue_slot_props_{index}", span = Span::mixed_site());
    let slot_window = format_ident!("__gpui_vue_slot_window_{index}", span = Span::mixed_site());
    let slot_app = format_ident!("__gpui_vue_slot_app_{index}", span = Span::mixed_site());
    let this = &context.this;
    let window = &context.window;
    let component_context = &context.context;
    let pattern = provider
        .pattern
        .as_ref()
        .map_or_else(|| quote!(_), |pattern| quote!(#pattern));
    let binding_lint = implicit_provider_binding_lint();

    quote_spanned! {provider.span=>
        #crate_path::Slot::new({
            let #owner = #component_context.weak_entity();
            move |
                #slot_props,
                #slot_window: &mut #crate_path::gpui::Window,
                #slot_app: &mut #crate_path::gpui::App,
            | {
                match #owner.update(#slot_app, |#update_this, #update_context| {
                    #binding_lint
                    let #this = #update_this;
                    #binding_lint
                    let #component_context = #update_context;
                    #binding_lint
                    let #window = #slot_window;
                    let #pattern = #slot_props;
                    #crate_path::SlotContent::new(#provider_element)
                }) {
                    ::core::result::Result::Ok(#rendered) => #rendered,
                    ::core::result::Result::Err(_) =>
                        #crate_path::SlotContent::new(#crate_path::gpui::Empty),
                }
            }
        })
    }
}

/// Emits one standalone provider that retains only owned `'static` captures.
fn owned_slot_provider_tokens(
    provider: &ComponentSlotBinding,
    index: usize,
    provider_element: &TokenStream,
    crate_path: &TokenStream,
) -> TokenStream {
    let slot_props = format_ident!("__gpui_vue_slot_props_{index}", span = Span::mixed_site());
    let slot_window = format_ident!("__gpui_vue_slot_window_{index}", span = Span::mixed_site());
    let slot_app = format_ident!("__gpui_vue_slot_app_{index}", span = Span::mixed_site());
    let pattern = provider
        .pattern
        .as_ref()
        .map_or_else(|| quote!(#slot_props), |pattern| quote!(#pattern));

    quote_spanned! {provider.span=>
        #crate_path::Slot::new(
            move |
                #pattern,
                #slot_window: &mut #crate_path::gpui::Window,
                #slot_app: &mut #crate_path::gpui::App,
            | { #provider_element },
        )
    }
}

/// Restricts generated provider lint exemptions to implicit context aliases.
fn implicit_provider_binding_lint() -> TokenStream {
    quote! {
        #[allow(
            unused_variables,
            clippy::no_effect_underscore_binding,
            clippy::used_underscore_binding,
        )]
    }
}

/// Builds the frame input for a plain, explicitly slotted, or declaratively slotted host.
fn component_input_tokens(
    tag: &Ident,
    props: &TokenStream,
    slots_binding: Option<&ComponentSlotsBinding>,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    match slots_binding {
        None => Ok(quote!(<#tag as #crate_path::NativeComponent>::Input::new(#props))),
        Some(ComponentSlotsBinding::Explicit(slots)) => Ok(quote! {
            <#tag as #crate_path::NativeComponentSlots>::input_with_slots(#props, #slots)
        }),
        Some(ComponentSlotsBinding::Declarative(providers)) => {
            let mut slots = quote! {
                <<#tag as #crate_path::NativeComponentSlots>::Slots as
                    ::core::default::Default>::default()
            };
            for (index, provider) in providers.iter().enumerate() {
                let setter = &provider.setter;
                let provider_element = expand_roots(&provider.roots, crate_path, context)?;
                let renderer = context.map_or_else(
                    || owned_slot_provider_tokens(provider, index, &provider_element, crate_path),
                    |context| {
                        contextual_slot_provider_tokens(
                            provider,
                            index,
                            &provider_element,
                            crate_path,
                            context,
                        )
                    },
                );
                slots = quote_spanned!(provider.span=> #slots.#setter(#renderer));
            }
            Ok(quote! {
                <#tag as #crate_path::NativeComponentSlots>::input_with_slots(#props, #slots)
            })
        }
    }
}

/// Lowers all typed listeners into the component's single monomorphic event host.
fn expand_component_with_events(
    tag: &Ident,
    bindings: &ComponentBindings,
    stable_slot: &TokenStream,
    key: &TokenStream,
    input: &TokenStream,
    crate_path: &TokenStream,
) -> TokenStream {
    let handler_names = (0..bindings.events.len())
        .map(|index| format_ident!("__gpui_vue_component_handler_{index}"))
        .collect::<Vec<_>>();
    let handler_bindings =
        bindings
            .events
            .iter()
            .zip(&handler_names)
            .map(|(event, handler_name)| {
                let expression = &event.handler;
                quote_spanned!(event.span=> let mut #handler_name = #expression;)
            });
    let dispatches = bindings
        .events
        .iter()
        .zip(&handler_names)
        .map(|(event, handler_name)| {
            let dispatcher = &event.dispatcher;
            quote_spanned! {event.span=>
                __gpui_vue_component_event.#dispatcher(
                    &mut #handler_name,
                    __gpui_vue_component_window,
                    __gpui_vue_component_cx,
                );
            }
        });

    quote_spanned! {tag.span()=> {
        #(#handler_bindings)*
        #crate_path::component_element_with_events::<
            #tag,
            <#tag as #crate_path::NativeComponentEvents>::Event,
            _,
        >(
            #stable_slot,
            #key,
            #input,
            move |
                __gpui_vue_component_event: &<#tag as #crate_path::NativeComponentEvents>::Event,
                __gpui_vue_component_window: &mut #crate_path::gpui::Window,
                __gpui_vue_component_cx: &mut #crate_path::gpui::App,
            | {
                #(#dispatches)*
            },
        )
    }}
}

/// Ensures every stateful GPUI builder has a stable element identity.
fn validate_element_identity(
    element: &Element,
    bindings: &ElementBindings,
    classes: Option<&CompiledClasses>,
    dynamic_classes: Option<&DynamicClasses>,
) -> Result<()> {
    let class_needs_id = classes.is_some_and(CompiledClasses::needs_stateful_id);
    let dynamic_class_needs_id = dynamic_classes.is_some_and(DynamicClasses::needs_stateful_id);
    let needs_id = !bindings.events.is_empty()
        || bindings.drag_payload.is_some()
        || bindings.drag_preview.is_some()
        || bindings.can_drop.is_some()
        || !bindings.drag_over.is_empty()
        || bindings.track_focus.is_some()
        || bindings.key_context.is_some()
        || bindings.focusable
        || bindings.occlude
        || bindings.tab_index.is_some()
        || class_needs_id
        || dynamic_class_needs_id;
    if needs_id && bindings.id.is_none() && bindings.key.is_none() {
        return Err(syn::Error::new(
            element.tag.span(),
            "interactive elements require a stable `id`; inside v-for use `:key={...}`",
        ));
    }
    Ok(())
}

/// Adds identity, focus, variants, and events in GPUI's required builder order.
fn apply_interactivity(
    mut output: TokenStream,
    bindings: &ElementBindings,
    classes: Option<&CompiledClasses>,
    dynamic_classes: Option<&DynamicClasses>,
    is_button: bool,
    focus_handle_override: Option<&TokenStream>,
    crate_path: &TokenStream,
) -> TokenStream {
    if let Some(element_id) = bindings.key.as_ref().or(bindings.id.as_ref()) {
        output = quote!(#output.id(#element_id));
    }
    if bindings.occlude {
        output = quote!(#output.occlude());
    }
    if let Some(track_focus) = focus_handle_override {
        output = quote!(#output.track_focus(#track_focus));
    } else if let Some(track_focus) = &bindings.track_focus {
        output = quote!(#output.track_focus(#track_focus));
    }
    if let Some(tab_index) = &bindings.tab_index {
        output = quote!(#output.tab_index(#tab_index));
    } else if is_button {
        output = quote!(#output.tab_index(0));
    } else if bindings.focusable {
        output = quote!(#output.focusable());
    }
    if let Some(key_context) = &bindings.key_context {
        output = quote!(#output.key_context(#key_context));
    }
    if let Some(classes) = classes {
        output = classes.apply_variants(output, crate_path);
    }
    if let Some(dynamic_classes) = dynamic_classes {
        output = dynamic_classes.apply(&output, crate_path);
    }
    if let Some(inline_style) = &bindings.inline_style {
        output = quote_spanned! {inline_style.span()=>
            #crate_path::ui::apply_style_refinement(#output, #inline_style)
        };
    }
    if bindings.has_ordered_host_bindings() {
        output = apply_host_bindings_in_source_order(&output, bindings, crate_path);
    } else {
        for event in &bindings.events {
            output = event.apply(&output, crate_path);
        }
    }
    output
}

/// Lowers interactions and `v-text` without regrouping source expressions.
///
/// GPUI exposes a single drag source and predicate but repeated typed move,
/// drop, and drag-over lanes. Keeping each source expression at its original
/// attribute position preserves Rust move semantics and observable evaluation
/// order even when the payload and preview attributes are separated.
fn apply_host_bindings_in_source_order(
    output: &TokenStream,
    bindings: &ElementBindings,
    crate_path: &TokenStream,
) -> TokenStream {
    let element_ordinal = bindings.element_ordinal;
    let mut interactions = Vec::with_capacity(
        bindings.events.len()
            + bindings.drag_over.len()
            + usize::from(bindings.drag_payload.is_some())
            + usize::from(bindings.drag_preview.is_some())
            + usize::from(bindings.can_drop.is_some())
            + usize::from(bindings.text_content.is_some()),
    );
    interactions.extend(bindings.events.iter().map(OrderedHostInteraction::Event));
    if let Some(payload) = &bindings.drag_payload {
        interactions.push(OrderedHostInteraction::DragPayload(payload));
    }
    if let Some(preview) = &bindings.drag_preview {
        interactions.push(OrderedHostInteraction::DragPreview(preview));
    }
    if let Some(can_drop) = &bindings.can_drop {
        interactions.push(OrderedHostInteraction::CanDrop(can_drop));
    }
    interactions.extend(
        bindings
            .drag_over
            .iter()
            .map(OrderedHostInteraction::DragOver),
    );
    if let Some(text) = &bindings.text_content {
        interactions.push(OrderedHostInteraction::Text(text));
    }
    interactions.sort_by_key(OrderedHostInteraction::source_index);

    let element = format_ident!(
        "__gpui_vue_drag_host_{element_ordinal}",
        span = Span::mixed_site()
    );
    let payload_local = format_ident!(
        "__gpui_vue_drag_payload_{element_ordinal}",
        span = Span::mixed_site()
    );
    let preview_local = format_ident!(
        "__gpui_vue_drag_preview_{element_ordinal}",
        span = Span::mixed_site()
    );
    let text_local = format_ident!(
        "__gpui_vue_v_text_{element_ordinal}",
        span = Span::mixed_site()
    );
    let drag_source_completion_index = drag_source_completion_index(bindings);
    let statements = interactions
        .iter()
        .map(|interaction| {
            host_interaction_tokens(
                interaction,
                &element,
                &payload_local,
                &preview_local,
                &text_local,
                drag_source_completion_index,
                crate_path,
            )
        })
        .collect::<Vec<_>>();

    quote!({
        let mut #element = #output;
        #(#statements)*
        #element
    })
}

/// Finds the source position where both halves of one drag source are available.
fn drag_source_completion_index(bindings: &ElementBindings) -> Option<usize> {
    bindings
        .drag_payload
        .as_ref()
        .zip(bindings.drag_preview.as_ref())
        .map(|(payload, preview)| payload.source_index.max(preview.source_index))
}

/// Emits one source-positioned intrinsic interaction or content binding.
fn host_interaction_tokens(
    interaction: &OrderedHostInteraction<'_>,
    element: &Ident,
    payload_local: &Ident,
    preview_local: &Ident,
    text_local: &Ident,
    drag_source_completion_index: Option<usize>,
    crate_path: &TokenStream,
) -> TokenStream {
    match interaction {
        OrderedHostInteraction::Event(event) => {
            let applied = event.apply(&quote!(#element), crate_path);
            quote_spanned! {event.span=> #element = #applied; }
        }
        OrderedHostInteraction::DragPayload(binding) => {
            let expression = &binding.expression;
            let attach = attach_completed_drag_source(
                binding,
                element,
                payload_local,
                preview_local,
                drag_source_completion_index,
            );
            quote_spanned! {binding.span=>
                let #payload_local = #expression;
                #attach
            }
        }
        OrderedHostInteraction::DragPreview(binding) => {
            let expression = &binding.expression;
            let attach = attach_completed_drag_source(
                binding,
                element,
                payload_local,
                preview_local,
                drag_source_completion_index,
            );
            quote_spanned! {binding.span=>
                let #preview_local = #crate_path::ui::type_drag_preview(#expression);
                #attach
            }
        }
        OrderedHostInteraction::CanDrop(binding) => {
            let expression = &binding.expression;
            quote_spanned! {binding.span=> #element = #element.can_drop(#expression); }
        }
        OrderedHostInteraction::DragOver(binding) => {
            let expression = &binding.expression;
            quote_spanned! {binding.span=> #element = #element.drag_over(#expression); }
        }
        OrderedHostInteraction::Text(binding) => {
            let expression = &binding.expression;
            quote_spanned! {binding.span=>
                let #text_local = #expression;
                #element = #element.child(#text_local);
            }
        }
    }
}

/// Attaches the one native drag source after its later expression is evaluated.
fn attach_completed_drag_source(
    binding: &OrderedExpression,
    element: &Ident,
    payload_local: &Ident,
    preview_local: &Ident,
    completion_index: Option<usize>,
) -> TokenStream {
    if completion_index == Some(binding.source_index) {
        quote_spanned! {binding.span=>
            #element = #element.on_drag(#payload_local, #preview_local);
        }
    } else {
        TokenStream::new()
    }
}

/// Declares one typed focus-handler slot, mutable only when source assigns it.
fn focus_handler_binding(
    bindings: &ElementBindings,
    kind: EventKind,
    crate_path: &TokenStream,
) -> TokenStream {
    let mutable = bindings.events.iter().any(|event| event.kind == kind);
    let name = match kind {
        EventKind::Focus => quote!(__gpui_vue_on_focus),
        EventKind::Blur => quote!(__gpui_vue_on_blur),
        _ => unreachable!("focus handler slots exist only for focus and blur"),
    };
    if mutable {
        quote!(let mut #name: ::core::option::Option<
            #crate_path::ui::FocusEventHandler
        > = ::core::option::Option::None;)
    } else {
        quote!(let #name: ::core::option::Option<
            #crate_path::ui::FocusEventHandler
        > = ::core::option::Option::None;)
    }
}

/// Wraps a completed native host with exact focus and blur subscriptions.
fn apply_focus_lifecycle(output: &TokenStream, crate_path: &TokenStream) -> TokenStream {
    quote!(#crate_path::ui::focus_events(
        #output,
        __gpui_vue_focus_handle,
        __gpui_vue_on_focus,
        __gpui_vue_on_blur,
    ))
}

/// Quotes a required attribute value.
fn value_tokens(attribute: &Attribute) -> Result<TokenStream> {
    match &attribute.value {
        Some(AttributeValue::Expression(expression)) => Ok(quote!(#expression)),
        Some(AttributeValue::String(value)) => Ok(quote!(#value)),
        Some(AttributeValue::Pattern(_)) => Err(syn::Error::new(
            attribute.span,
            "slot patterns cannot be used as ordinary attribute values",
        )),
        None => Err(syn::Error::new(
            attribute.span,
            format!("{} requires a value", attribute.name),
        )),
    }
}

/// Requires a braced expression for an element binding.
fn expect_attribute_expression(attribute: &Attribute, label: &str) -> Result<Expr> {
    match &attribute.value {
        Some(AttributeValue::Expression(expression)) => Ok(expression.clone()),
        _ => Err(syn::Error::new(
            attribute.span,
            format!("{label} must be a Rust expression in `{{ ... }}`"),
        )),
    }
}

/// Reads a bound expression or expands Vue 3.4's same-name shorthand.
fn expect_bound_expression(attribute: &Attribute, binding_name: &str) -> Result<Expr> {
    match &attribute.value {
        Some(AttributeValue::Expression(expression)) => Ok(expression.clone()),
        None => {
            let identifier = Ident::new(binding_name, attribute.span);
            Ok(syn::parse_quote!(#identifier))
        }
        Some(AttributeValue::String(_) | AttributeValue::Pattern(_)) => Err(syn::Error::new(
            attribute.span,
            format!(
                ":{binding_name} must be a Rust expression in `{{ ... }}` or the same-name shorthand"
            ),
        )),
    }
}

/// Appends sibling nodes while normalizing adjacent conditional chains.
fn append_nodes(
    mut parent: TokenStream,
    nodes: &[Node],
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    let mut index = 0;
    while index < nodes.len() {
        match &nodes[index] {
            Node::Element(element)
                if matches!(element.conditional, Some(ConditionalDirective::If(_))) =>
            {
                let end = conditional_chain_end(nodes, index);
                parent =
                    append_conditional_chain(&parent, &nodes[index..end], crate_path, context)?;
                index = end;
            }
            Node::Element(element)
                if matches!(
                    element.conditional,
                    Some(ConditionalDirective::ElseIf(_) | ConditionalDirective::Else)
                ) =>
            {
                return Err(syn::Error::new(
                    element.tag.span(),
                    "v-else-if and v-else must immediately follow a v-if or v-else-if sibling",
                ));
            }
            node => {
                parent = append_unconditional_node(&parent, node, crate_path, context)?;
                index += 1;
            }
        }
    }
    Ok(parent)
}

/// Finds the exclusive end of one adjacent `v-if` sibling chain.
fn conditional_chain_end(nodes: &[Node], start: usize) -> usize {
    let mut end = start + 1;
    while let Some(Node::Element(element)) = nodes.get(end) {
        match element.conditional {
            Some(ConditionalDirective::ElseIf(_)) => end += 1,
            Some(ConditionalDirective::Else) => return end + 1,
            _ => break,
        }
    }
    end
}

/// Emits nested `when_else` calls for one normalized conditional chain.
fn append_conditional_chain(
    parent: &TokenStream,
    branches: &[Node],
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    let Some(Node::Element(element)) = branches.first() else {
        return Err(syn::Error::new(
            Span::call_site(),
            "empty conditional chain",
        ));
    };
    match element.conditional.as_ref() {
        Some(ConditionalDirective::If(condition) | ConditionalDirective::ElseIf(condition)) => {
            let then_branch = append_unconditional_element(
                &quote!(__gpui_vue_parent),
                element,
                crate_path,
                context,
            )?;
            if branches.len() == 1 {
                Ok(quote!(#parent.when(#condition, |__gpui_vue_parent| #then_branch)))
            } else {
                let else_branch = append_conditional_chain(
                    &quote!(__gpui_vue_parent),
                    &branches[1..],
                    crate_path,
                    context,
                )?;
                Ok(quote!(#parent.when_else(
                    #condition,
                    |__gpui_vue_parent| #then_branch,
                    |__gpui_vue_parent| #else_branch,
                )))
            }
        }
        Some(ConditionalDirective::Else) => {
            if branches.len() != 1 {
                return Err(syn::Error::new(
                    element.tag.span(),
                    "v-else must be the final branch",
                ));
            }
            append_unconditional_element(parent, element, crate_path, context)
        }
        None => Err(syn::Error::new(
            element.tag.span(),
            "internal conditional-chain normalization error",
        )),
    }
}

/// Appends a node after structural condition handling has completed.
fn append_unconditional_node(
    parent: &TokenStream,
    node: &Node,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    match node {
        Node::Expression(expression) => Ok(quote!(#parent.child(#expression))),
        Node::Text(text) => Ok(quote!(#parent.child(#text))),
        Node::Fragment(children) => append_nodes(parent.clone(), children, crate_path, context),
        Node::Element(element) => {
            append_unconditional_element(parent, element, crate_path, context)
        }
    }
}

/// Appends an element, applying its loop after its conditional has been resolved.
fn append_unconditional_element(
    parent: &TokenStream,
    element: &Element,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    if element.tag == "template" {
        return append_structural_template(parent, element, crate_path, context);
    }
    if element.tag == "slot" {
        return append_slot_outlet(parent, element, crate_path, context);
    }
    let child = expand_element(element, crate_path, context)?;
    if let Some(for_directive) = &element.directive_for {
        let pattern = &for_directive.pattern;
        let iterator = &for_directive.iterator;
        Ok(quote!(
            #parent.children((#iterator).into_iter().map(|#pattern| #child))
        ))
    } else {
        Ok(quote!(#parent.child(#child)))
    }
}

/// Flattens a structural `<template>` into its parent when GPUI identity permits.
fn append_structural_template(
    parent: &TokenStream,
    element: &Element,
    crate_path: &TokenStream,
    context: Option<&ComponentTemplateContext>,
) -> Result<TokenStream> {
    if let Some(attribute) = element
        .attributes
        .iter()
        .find(|attribute| attribute.name == "v-text")
    {
        return Err(syn::Error::new(
            attribute.span,
            "v-text requires a parent intrinsic and cannot be used on <template>",
        ));
    }
    if !element.attributes.is_empty() {
        return Err(syn::Error::new(
            element.tag.span(),
            "<template> only accepts structural directives",
        ));
    }
    if element.directive_show.is_some() {
        return Err(syn::Error::new(
            element.tag.span(),
            "v-show cannot be used on <template> because it has no rendered host element",
        ));
    }
    if element.directive_for.is_some() {
        return Err(syn::Error::new(
            element.tag.span(),
            "<template v-for> requires fragment identity that GPUI 0.2.2 cannot represent without changing layout; put v-for and :key on a real child element",
        ));
    }
    append_nodes(parent.clone(), &element.children, crate_path, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates direct-template metadata for one unit default and typed action slot.
    fn slotted_component_context() -> ComponentTemplateContext {
        ComponentTemplateContext::new(
            syn::parse_quote!(this),
            syn::parse_quote!(window),
            syn::parse_quote!(cx),
            vec![
                ComponentSlotMetadata::new(syn::parse_quote!(default), syn::parse_quote!(())),
                ComponentSlotMetadata::new(
                    syn::parse_quote!(actions),
                    syn::parse_quote!(ActionProps),
                ),
            ],
        )
    }

    #[test]
    fn parses_multiple_roots_fragments_and_directives() {
        let template: Template = syn::parse_quote! {
            <>
                <text v-if={visible}>{label}</text>
                <text v-else-if={pending}>"Pending"</text>
                <text v-else>"Hidden"</text>
            </>
            <div v-show={visible} />
        };
        assert_eq!(template.roots.len(), 2);
    }

    #[test]
    fn every_loop_requires_a_bound_key() {
        let error = expand(&quote!(
            <div><button v-for={n in 0..2} key="same">{n}</button></div>
        ))
        .unwrap_err();
        assert!(error.to_string().contains("dynamic `:key"));

        let error = expand(&quote!(
            <div><span v-for={n in 0..2} :key={1}>{n}</span></div>
        ))
        .unwrap_err();
        assert!(error.to_string().contains("not a literal"));
    }

    #[test]
    fn keyed_loop_namespaces_stateful_descendants() {
        expand(&quote! {
            <div>
                <div v-for={n in 0_usize..2} :key={("row", n)}>
                    <button id="same-child-id" @click={|_, _, _| {}}>{n}</button>
                </div>
            </div>
        })
        .unwrap();

        expand(&quote! {
            <div><span v-for={key in keys} :key>{key}</span></div>
        })
        .unwrap();
    }

    #[test]
    fn focus_button_and_show_semantics_are_generated() {
        let focus = expand(&quote!(
            <div id="card" v-show={visible} class="focus:bg-blue-500">"card"</div>
        ))
        .unwrap()
        .to_string();
        assert!(focus.contains("focusable"));
        assert!(focus.contains("hidden"));

        let button = expand(&quote!(<button id="save">"Save"</button>))
            .unwrap()
            .to_string();
        assert!(button.contains("tab_index (0"));
    }

    #[test]
    fn button_cursor_utility_overrides_the_intrinsic_default() {
        let expanded = expand(&quote!(
            <button id="busy" class="cursor-not-allowed">"Busy"</button>
        ))
        .unwrap()
        .to_string();
        let default_cursor = expanded.find("cursor_pointer").unwrap();
        let utility_cursor = expanded.find("cursor_not_allowed").unwrap();

        assert!(default_cursor < utility_cursor);
    }

    #[test]
    fn v_text_lowers_once_as_a_source_ordered_native_child() {
        let expanded = expand(&quote! {
            <div
                id="status"
                @hover={hover_factory()}
                v-text={text_factory()}
                @click={click_factory()}
            />
        })
        .unwrap()
        .to_string();

        assert_eq!(expanded.matches("text_factory ()").count(), 1);
        assert!(expanded.contains("child (__gpui_vue_v_text_0)"));
        let hover = expanded.find("hover_factory ()").unwrap();
        let text = expanded.find("text_factory ()").unwrap();
        let click = expanded.find("click_factory ()").unwrap();
        assert!(hover < text && text < click, "{expanded}");
    }

    #[test]
    fn v_text_composes_with_existing_structural_and_identity_rules() {
        let conditional = expand(&quote! {
            <div v-if={ready} v-text={label} />
        })
        .expect("v-text should use the ordinary intrinsic conditional lane")
        .to_string();
        assert!(conditional.contains("when (ready"));
        assert!(conditional.contains("child (__gpui_vue_v_text_0)"));

        let looped = expand(&quote! {
            <div v-for={item in items} :key={item.id} v-text={item.label} />
        })
        .expect("v-text should preserve keyed v-for identity")
        .to_string();
        assert!(looped.contains("into_iter () . map"));
        assert!(looped.contains("item . label"));

        let unkeyed = expand(&quote! {
            <div v-for={item in items} v-text={item.label} />
        })
        .unwrap_err();
        assert!(unkeyed.to_string().contains("dynamic `:key"));
    }

    #[test]
    fn v_text_rejects_children_non_expressions_and_non_parent_hosts() {
        let children = expand(&quote! {
            <div v-text={label}>"existing child"</div>
        })
        .unwrap_err();
        assert!(
            children
                .to_string()
                .contains("cannot be used with children")
        );

        let literal = expand(&quote! { <span v-text="literal" /> }).unwrap_err();
        assert!(literal.to_string().contains("must be a Rust expression"));

        let duplicate = expand(&quote! {
            <text v-text={first} v-text={second} />
        })
        .unwrap_err();
        assert!(duplicate.to_string().contains("duplicate v-text"));

        for (source, target) in [
            (quote!(<img src="icon.png" v-text={label} />), "<img>"),
            (quote!(<template v-text={label} />), "<template>"),
            (quote!(<Child v-text={label} />), "PascalCase components"),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains(target));
        }
    }

    #[test]
    fn event_modifiers_are_ordered_and_dom_only_modifier_fails() {
        let expanded = expand(&quote!(
            <button id="save" @click.stop.prevent.ctrl.exact={handler}>"Save"</button>
        ))
        .unwrap()
        .to_string();
        let stop = expanded.find("stop_propagation").unwrap();
        let prevent = expanded.find("prevent_default").unwrap();
        let control = expanded.find("modifiers () . control").unwrap();
        assert!(stop < prevent && prevent < control);
        assert!(expanded.contains("ui :: type_click_handler"));

        let error = expand(&quote!(
            <button id="save" @click.passive={handler}>"Save"</button>
        ))
        .unwrap_err();
        assert!(error.to_string().contains("no GPUI equivalent"));
    }

    #[test]
    fn focus_context_and_host_events_lower_in_native_builder_order() {
        let expanded = expand(&quote! {
            <div
                id="workspace"
                :track-focus={&focus}
                key-context="KageEditor"
                @key-down={key_down}
                @mouse-down.left={mouse_down_left}
                @mouse-down.right={mouse_down_right}
                @mouse-move={mouse_move}
                @mouse-up.middle={mouse_up}
                @mouse-up-out.left={mouse_up_out}
                @pinch={pinch}
                @scroll-wheel={scroll}
            />
        })
        .unwrap()
        .to_string();

        let identity = expanded.find("id (\"workspace\")").unwrap();
        let track_focus = expanded.find("track_focus (& focus)").unwrap();
        let key_context = expanded.find("key_context (\"KageEditor\")").unwrap();
        let key_down = expanded.find("on_key_down (key_down)").unwrap();
        assert!(identity < track_focus && track_focus < key_context && key_context < key_down);
        assert!(expanded.contains("on_mouse_down (:: gpui_vue :: gpui :: MouseButton :: Left"));
        assert!(expanded.contains("on_mouse_down (:: gpui_vue :: gpui :: MouseButton :: Right"));
        assert!(expanded.contains("on_mouse_move (mouse_move)"));
        assert!(expanded.contains("on_mouse_up (:: gpui_vue :: gpui :: MouseButton :: Middle"));
        assert!(expanded.contains("on_mouse_up_out (:: gpui_vue :: gpui :: MouseButton :: Left"));
        assert!(expanded.contains("on_pinch (pinch)"));
        assert!(expanded.contains("on_scroll_wheel (scroll)"));
        let pinch = expanded.find("on_pinch (pinch)").unwrap();
        let scroll = expanded.find("on_scroll_wheel (scroll)").unwrap();
        assert!(key_down < pinch && pinch < scroll);

        let dynamic_context = expand(&quote! {
            <div id="dynamic-context" :key-context={context} />
        })
        .unwrap()
        .to_string();
        assert!(dynamic_context.contains("key_context (context)"));
    }

    #[test]
    fn extended_native_events_preserve_typed_routing_and_source_order() {
        let expanded = expand(&quote! {
            <div
                id="extended-events"
                :track-focus={&focus_handle}
                @key-up={key_up}
                on:modifiers-changed={modifiers_changed}
                @mouse-down-out.left={left_out}
                on:mouse-down-out.right={right_out}
                @mouse-down-out.middle={middle_out}
                @hover={hover}
                @focus={focused}
                on:blur={blurred}
            />
        })
        .unwrap()
        .to_string();

        assert_eq!(expanded.matches("& focus_handle").count(), 1);
        assert!(expanded.contains("on_key_up (key_up)"));
        assert!(expanded.contains("on_modifiers_changed (modifiers_changed)"));
        assert_eq!(expanded.matches("on_mouse_down_out").count(), 3);
        assert!(expanded.contains("ui :: type_mouse_down_handler (left_out)"));
        assert!(expanded.contains("MouseButton :: Left"));
        assert!(expanded.contains("MouseButton :: Right"));
        assert!(expanded.contains("MouseButton :: Middle"));
        assert!(expanded.contains("on_hover (hover)"));
        assert!(expanded.contains("ui :: boxed_focus_handler (focused)"));
        assert!(expanded.contains("ui :: boxed_focus_handler (blurred)"));
        assert!(expanded.contains("ui :: focus_events"));

        let key_up = expanded.find("on_key_up (key_up)").unwrap();
        let modifiers = expanded
            .find("on_modifiers_changed (modifiers_changed)")
            .unwrap();
        let left_out = expanded.find("type_mouse_down_handler (left_out)").unwrap();
        let right_out = expanded
            .find("type_mouse_down_handler (right_out)")
            .unwrap();
        let middle_out = expanded
            .find("type_mouse_down_handler (middle_out)")
            .unwrap();
        let hover = expanded.find("on_hover (hover)").unwrap();
        let focused = expanded.find("boxed_focus_handler (focused)").unwrap();
        let blurred = expanded.find("boxed_focus_handler (blurred)").unwrap();
        assert!(
            key_up < modifiers
                && modifiers < left_out
                && left_out < right_out
                && right_out < middle_out
                && middle_out < hover
                && hover < focused
                && focused < blurred
        );
    }

    #[test]
    fn drag_drop_bindings_lower_to_native_typed_lanes_in_source_order() {
        let expanded = expand(&quote! {
            <div
                id="drag-surface"
                :drag-preview={preview_factory()}
                @drag-move={row_move_factory()}
                :drag-payload={payload_factory()}
                on:drag-move={tab_move_factory()}
                :can-drop={predicate_factory()}
                :drag-over={row_style_factory()}
                @drop={row_drop_factory()}
                :drag-over={file_style_factory()}
                on:drop={file_drop_factory()}
            />
        })
        .unwrap()
        .to_string();

        assert!(expanded.contains("ui :: type_drag_preview (preview_factory ())"));
        assert!(expanded.contains("on_drag"));
        assert_eq!(expanded.matches("on_drag_move").count(), 2);
        assert_eq!(expanded.matches("drag_over").count(), 2);
        assert_eq!(expanded.matches("on_drop").count(), 2);
        for expression in [
            "preview_factory ()",
            "row_move_factory ()",
            "payload_factory ()",
            "tab_move_factory ()",
            "predicate_factory ()",
            "row_style_factory ()",
            "row_drop_factory ()",
            "file_style_factory ()",
            "file_drop_factory ()",
        ] {
            assert_eq!(expanded.matches(expression).count(), 1, "{expanded}");
        }

        let identity = expanded.find("id (\"drag-surface\")").unwrap();
        let preview = expanded.find("preview_factory ()").unwrap();
        let row_move = expanded.find("row_move_factory ()").unwrap();
        let payload = expanded.find("payload_factory ()").unwrap();
        let drag_source = expanded.find(". on_drag (").unwrap();
        let tab_move = expanded.find("tab_move_factory ()").unwrap();
        let predicate = expanded.find("predicate_factory ()").unwrap();
        let row_style = expanded.find("row_style_factory ()").unwrap();
        let row_drop = expanded.find("row_drop_factory ()").unwrap();
        let file_style = expanded.find("file_style_factory ()").unwrap();
        let file_drop = expanded.find("file_drop_factory ()").unwrap();
        assert!(
            identity < preview
                && preview < row_move
                && row_move < payload
                && payload < drag_source
                && drag_source < tab_move
                && tab_move < predicate
                && predicate < row_style
                && row_style < row_drop
                && row_drop < file_style
                && file_style < file_drop,
            "{expanded}",
        );
    }

    #[test]
    fn drag_source_pair_and_single_native_lanes_are_diagnosed() {
        let missing_preview = expand(&quote! {
            <div id="source" :drag-payload={payload} />
        })
        .unwrap_err();
        assert!(
            missing_preview
                .to_string()
                .contains(":drag-payload requires a matching :drag-preview")
        );

        let missing_payload = expand(&quote! {
            <div id="source" :drag-preview={preview} />
        })
        .unwrap_err();
        assert!(
            missing_payload
                .to_string()
                .contains(":drag-preview requires a matching :drag-payload")
        );

        for (source, diagnostic) in [
            (
                quote!(<div id="source" :drag-payload={first} :drag-payload={second} :drag-preview={preview} />),
                "duplicate :drag-payload",
            ),
            (
                quote!(<div id="source" :drag-payload={payload} :drag-preview={first} :drag-preview={second} />),
                "duplicate :drag-preview",
            ),
            (
                quote!(<div id="target" :can-drop={first} :can-drop={second} />),
                "duplicate :can-drop",
            ),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains(diagnostic));
        }
    }

    #[test]
    fn every_drag_drop_surface_requires_stable_identity_and_no_modifiers() {
        for source in [
            quote!(<div :drag-payload={payload} :drag-preview={preview} />),
            quote!(<div @drag-move={moving} />),
            quote!(<div :can-drop={predicate} />),
            quote!(<div :drag-over={style} />),
            quote!(<div @drop={dropped} />),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains("stable `id`"));
        }

        for (source, diagnostic) in [
            (
                quote!(<div id="move" @drag-move.stop={moving} />),
                "@drag-move does not support modifier `.stop`",
            ),
            (
                quote!(<div id="drop" on:drop.prevent={dropped} />),
                "@drop does not support modifier `.prevent`",
            ),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains(diagnostic));
        }
    }

    #[test]
    fn exact_focus_events_require_an_explicit_tracked_handle() {
        for source in [
            quote!(<div id="focus" @focus={focused} />),
            quote!(<div id="blur" @blur={blurred} />),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains("requires :track-focus"));
            assert!(error.to_string().contains("exact native focus handle"));
        }
    }

    #[test]
    fn host_binding_duplicates_and_stable_identity_are_diagnosed() {
        let duplicate_focus = expand(&quote! {
            <div id="focus" :track-focus={first} :track-focus={second} />
        })
        .unwrap_err();
        assert!(
            duplicate_focus
                .to_string()
                .contains("duplicate :track-focus")
        );

        let duplicate_context = expand(&quote! {
            <div id="context" key-context="Static" :key-context={dynamic} />
        })
        .unwrap_err();
        assert!(
            duplicate_context
                .to_string()
                .contains("duplicate key-context")
        );

        let duplicate_event = expand(&quote! {
            <div id="events" @mouse-down.left={first} on:mouse-down.left={second} />
        })
        .unwrap_err();
        assert!(
            duplicate_event
                .to_string()
                .contains("duplicate mouse-down.left handler")
        );

        let duplicate_pinch = expand(&quote! {
            <div id="pinch" @pinch={first} on:pinch={second} />
        })
        .unwrap_err();
        assert!(
            duplicate_pinch
                .to_string()
                .contains("duplicate pinch handler")
        );

        for (source, diagnostic) in [
            (
                quote!(<div id="key-up" @key-up={first} on:key-up={second} />),
                "duplicate key-up handler",
            ),
            (
                quote!(<div id="modifiers" @modifiers-changed={first} on:modifiers-changed={second} />),
                "duplicate modifiers-changed handler",
            ),
            (
                quote!(<div id="down-out" @mouse-down-out.left={first} on:mouse-down-out.left={second} />),
                "duplicate mouse-down-out.left handler",
            ),
            (
                quote!(<div id="hover" @hover={first} on:hover={second} />),
                "duplicate hover handler",
            ),
            (
                quote!(<div id="focus-event" :track-focus={&focus} @focus={first} on:focus={second} />),
                "duplicate focus handler",
            ),
            (
                quote!(<div id="blur-event" :track-focus={&focus} @blur={first} on:blur={second} />),
                "duplicate blur handler",
            ),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains(diagnostic));
        }

        for source in [
            quote!(<div :track-focus={&focus} />),
            quote!(<div key-context="Editor" />),
            quote!(<div @mouse-move={moving} />),
            quote!(<div @pinch={pinching} />),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains("stable `id`"));
        }
    }

    #[test]
    fn occluding_hosts_require_identity_and_lower_after_it() {
        let expanded = expand(&quote! {
            <div id="modal-backdrop" occlude class="absolute inset-0" />
        })
        .unwrap()
        .to_string();
        let identity = expanded.find("id (\"modal-backdrop\")").unwrap();
        let occlusion = expanded.find("occlude ()").unwrap();
        assert!(identity < occlusion);

        let missing_identity = expand(&quote! { <div occlude /> }).unwrap_err();
        assert!(missing_identity.to_string().contains("stable `id`"));

        let valued = expand(&quote! { <div id="modal" occlude={true} /> }).unwrap_err();
        assert!(valued.to_string().contains("takes no value"));

        let duplicate = expand(&quote! { <div id="modal" occlude occlude /> }).unwrap_err();
        assert!(duplicate.to_string().contains("duplicate occlude"));
    }

    #[test]
    fn image_intrinsic_requires_one_source_and_remains_styleable() {
        let expanded = expand(&quote! {
            <img
                id="preview"
                :src={thumbnail_url()}
                class="size-full rounded"
                :style={runtime_style()}
            />
        })
        .unwrap()
        .to_string();
        assert_eq!(expanded.matches("thumbnail_url ()").count(), 1);
        assert!(expanded.contains("let __gpui_vue_image_source_0 = thumbnail_url ()"));
        assert!(expanded.contains("ui :: image (__gpui_vue_image_source_0)"));
        assert!(expanded.contains("id (\"preview\")"));
        assert!(expanded.contains("apply_style_refinement"));

        let static_source = expand(&quote! {
            <img src="icons/editor.png" class="w-8 h-8" />
        })
        .unwrap()
        .to_string();
        assert!(static_source.contains("let __gpui_vue_image_source_0 = \"icons/editor.png\""));
        assert!(static_source.contains("ui :: image (__gpui_vue_image_source_0)"));

        let shorthand = expand(&quote! { <img :src /> }).unwrap().to_string();
        assert!(shorthand.contains("let __gpui_vue_image_source_0 = src"));

        let missing = expand(&quote! { <img class="w-8" /> }).unwrap_err();
        assert!(missing.to_string().contains("requires `src` or `:src`"));

        let children = expand(&quote! { <img src="icon.png">"invalid"</img> }).unwrap_err();
        assert!(children.to_string().contains("cannot have children"));

        let duplicate = expand(&quote! { <img src="one.png" :src={second} /> }).unwrap_err();
        assert!(duplicate.to_string().contains("duplicate image source"));
    }

    #[test]
    fn image_specific_bindings_are_typed_once_and_evaluated_in_source_order() {
        let expanded = expand(&quote! {
            <img
                :loading={loading_factory()}
                :src={source_factory()}
                :fallback={fallback_factory()}
                :object-fit={fit_factory()}
            />
        })
        .unwrap()
        .to_string();

        for expression in [
            "loading_factory ()",
            "source_factory ()",
            "fallback_factory ()",
            "fit_factory ()",
        ] {
            assert_eq!(expanded.matches(expression).count(), 1, "{expanded}");
        }
        let loading = expanded.find("loading_factory ()").unwrap();
        let source = expanded.find("source_factory ()").unwrap();
        let fallback = expanded.find("fallback_factory ()").unwrap();
        let fit = expanded.find("fit_factory ()").unwrap();
        assert!(
            loading < source && source < fallback && fallback < fit,
            "{expanded}"
        );
        assert!(expanded.contains("ui :: image_object_fit"));
        assert!(expanded.contains("ui :: image_loading"));
        assert!(expanded.contains("ui :: image_fallback"));
        assert!(
            !expanded.contains(". id ("),
            "image states do not require identity"
        );
    }

    #[test]
    fn image_specific_bindings_reject_duplicates_non_images_and_dom_strings() {
        for (source, diagnostic) in [
            (
                quote!(<img src="one.png" :object-fit={first} :object-fit={second} />),
                "duplicate :object-fit binding",
            ),
            (
                quote!(<img src="one.png" :loading={first} :loading={second} />),
                "duplicate :loading binding",
            ),
            (
                quote!(<img src="one.png" :fallback={first} :fallback={second} />),
                "duplicate :fallback binding",
            ),
            (
                quote!(<div :object-fit={fit} />),
                ":object-fit is only supported on the <img> intrinsic",
            ),
            (
                quote!(<div :loading={loading} />),
                ":loading is only supported on the <img> intrinsic",
            ),
            (
                quote!(<div :fallback={fallback} />),
                ":fallback is only supported on the <img> intrinsic",
            ),
            (
                quote!(<img src="one.png" object-fit="cover" />),
                "unsupported attribute `object-fit`",
            ),
            (
                quote!(<img src="one.png" :object-fit="cover" />),
                ":object-fit must be a Rust expression",
            ),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains(diagnostic), "{error}");
        }
    }

    #[test]
    fn mouse_buttons_are_mandatory_and_event_modifiers_are_scoped() {
        for source in [
            quote!(<div id="down" @mouse-down={handler} />),
            quote!(<div id="down-out" @mouse-down-out={handler} />),
            quote!(<div id="up" @mouse-up={handler} />),
            quote!(<div id="out" @mouse-up-out={handler} />),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("requires exactly one mouse button")
            );
        }

        let multiple = expand(&quote! {
            <div id="multiple" @mouse-down.left.right={handler} />
        })
        .unwrap_err();
        assert!(multiple.to_string().contains("exactly one mouse button"));

        let down_out_multiple = expand(&quote! {
            <div id="multiple-out" @mouse-down-out.left.right={handler} />
        })
        .unwrap_err();
        assert!(
            down_out_multiple
                .to_string()
                .contains("exactly one mouse button")
        );

        let unsupported_button = expand(&quote! {
            <div id="unsupported" @mouse-up.primary={handler} />
        })
        .unwrap_err();
        assert!(
            unsupported_button
                .to_string()
                .contains("expected .left, .right, or .middle")
        );

        let unsupported_modifier = expand(&quote! {
            <div id="move" @mouse-move.stop={handler} />
        })
        .unwrap_err();
        assert!(
            unsupported_modifier
                .to_string()
                .contains("@mouse-move does not support modifier `.stop`")
        );

        let pinch_modifier = expand(&quote! {
            <div id="pinch" @pinch.stop={handler} />
        })
        .unwrap_err();
        assert!(
            pinch_modifier
                .to_string()
                .contains("@pinch does not support modifier `.stop`")
        );

        for source in [
            quote!(<div id="key-up" @key-up.stop={handler} />),
            quote!(<div id="modifiers" @modifiers-changed.passive={handler} />),
            quote!(<div id="hover" @hover.capture={handler} />),
            quote!(<div id="focus" :track-focus={&focus} @focus.once={handler} />),
            quote!(<div id="blur" :track-focus={&focus} @blur.prevent={handler} />),
        ] {
            let error = expand(&source).unwrap_err();
            assert!(error.to_string().contains("does not support modifier"));
        }
    }

    #[test]
    fn dynamic_class_branches_compile_and_propagate_interactivity() {
        let expanded = expand(&quote!(
            <div
                id="status"
                class="p-2"
                :class={if selected {
                    "bg-blue-500 focus:bg-blue-700"
                } else if failed {
                    "bg-red-500"
                } else {
                    "bg-slate-500"
                }}
            >
                "Status"
            </div>
        ))
        .unwrap()
        .to_string();
        assert!(expanded.contains("when_else"));
        assert!(expanded.contains("focusable"));

        let error = expand(&quote!(
            <div :class={runtime_class}>"Status"</div>
        ))
        .unwrap_err();
        assert!(error.to_string().contains("runtime class strings"));
    }

    #[test]
    fn typed_inline_style_lowers_once_after_classes_and_before_events() {
        let expanded = expand(&quote! {
            <div
                id="runtime-style"
                class="w-4"
                :class={if selected { "text-blue-500" } else { "text-red-500" }}
                :style={make_refiner()}
                @click={handler}
            />
        })
        .unwrap()
        .to_string();

        assert_eq!(expanded.matches("make_refiner ()").count(), 1);
        let inline_style = expanded.find("apply_style_refinement").unwrap();
        let classes = expanded.find("when_else").unwrap();
        let event = expanded.find("on_click").unwrap();
        // The class expression is the helper's first argument, so Rust evaluates
        // it before invoking the outer style-refinement call.
        assert!(inline_style < classes && classes < event);
    }

    #[test]
    fn typed_inline_style_requires_one_rust_expression() {
        let duplicate = expand(&quote! {
            <div :style={first} :style={second} />
        })
        .unwrap_err();
        assert!(duplicate.to_string().contains("duplicate :style binding"));

        let literal = expand(&quote! {
            <div :style="color: red" />
        })
        .unwrap_err();
        assert!(
            literal
                .to_string()
                .contains(":style must be a Rust expression")
        );
    }

    #[test]
    fn static_and_dynamic_classes_share_one_conditional_lowering() {
        let expanded = expand(&quote!(
            <div
                id="merged-styles"
                class="pl-1 hover:pl-1 active:pl-1 focus:pr-1"
                :class={if class_condition() {
                    "p-4 hover:pl-4 active:pl-4 focus:pr-4"
                } else {
                    "p-6 hover:pl-6 active:pl-6 focus:pr-6"
                }}
            >
                "Merged"
            </div>
        ))
        .unwrap()
        .to_string();

        assert_eq!(expanded.matches("class_condition ()").count(), 1);
        assert_eq!(expanded.matches(". when_else (").count(), 1);
        assert_eq!(expanded.matches(". hover (").count(), 2);
        assert_eq!(expanded.matches(". active (").count(), 2);
        assert_eq!(expanded.matches(". focus (").count(), 2);
    }

    #[test]
    fn nested_dynamic_class_conditions_are_each_emitted_once() {
        let expanded = expand(&quote!(
            <div
                :class={if outer_condition() {
                    if inner_condition() { "p-2" } else { "p-3" }
                } else {
                    "p-4"
                }}
            />
        ))
        .unwrap()
        .to_string();

        assert_eq!(expanded.matches("outer_condition ()").count(), 1);
        assert_eq!(expanded.matches("inner_condition ()").count(), 1);
    }

    #[test]
    fn static_classes_form_the_fallback_for_a_missing_dynamic_else() {
        let expanded = expand(&quote!(
            <div
                id="static-fallback"
                :class={if optional_class() { "hover:bg-blue-500" }}
                class="hover:bg-slate-500"
            />
        ))
        .unwrap()
        .to_string();

        assert_eq!(expanded.matches("optional_class ()").count(), 1);
        assert_eq!(expanded.matches(". when_else (").count(), 1);
        assert_eq!(expanded.matches(". hover (").count(), 2);
    }

    #[test]
    fn orphan_else_and_template_show_are_errors() {
        let orphan = expand(&quote!(<div><span v-else>"x"</span></div>)).unwrap_err();
        assert!(orphan.to_string().contains("immediately follow"));

        let show = expand(&quote!(
            <div><template v-show={visible}>"x"</template></div>
        ))
        .unwrap_err();
        assert!(show.to_string().contains("cannot be used"));
    }

    #[test]
    fn bound_attribute_after_v_else_starts_a_new_intrinsic_attribute() {
        let expanded = expand(&quote! {
            <div>
                <span v-if={ready} :class={"text-green-500"} />
                <span v-else :class={"text-red-500"} />
            </div>
        })
        .expect("the colon after v-else must start :class")
        .to_string();

        assert!(expanded.contains("when_else"));
        assert!(expanded.contains("text_color"));
    }

    #[test]
    fn pascal_component_lowers_to_a_direct_stable_native_host() {
        let expanded = expand(&quote! {
            <Child :props={ChildProps::new("one")} />
        })
        .expect("a self-closing PascalCase component should lower")
        .to_string();

        assert!(expanded.contains("component_element :: < Child , _ , 0"));
        assert!(expanded.contains("< Child as :: gpui_vue :: NativeComponent > :: Input :: new"));
        assert!(expanded.contains("ElementId :: named_usize"));
        assert!(expanded.contains("module_path !"));
        assert!(expanded.contains("file !"));
        assert!(expanded.contains("line !"));
        assert!(expanded.contains("column !"));
        assert!(expanded.contains("Option :: None"));
        assert!(!expanded.contains("AnyElement"));
    }

    #[test]
    fn pascal_siblings_slots_and_keys_keep_distinct_compile_slots() {
        let expanded = expand(&quote! {
            <div>
                <Child
                    key="first"
                    :props={ChildProps::new("one")}
                    :slots={ChildSlots::new()}
                />
                <Child :props={ChildProps::new("two")} />
            </div>
        })
        .expect("component siblings should lower")
        .to_string();

        assert_eq!(expanded.matches("ElementId :: named_usize").count(), 2);
        assert_eq!(expanded.matches("1usize").count(), 1);
        assert_eq!(expanded.matches("2usize").count(), 1);
        assert!(expanded.contains("NativeComponentSlots > :: input_with_slots"));
        assert!(expanded.contains("ChildSlots :: new"));
        assert!(expanded.contains("Option :: Some"));
        assert!(expanded.contains("Into :: < :: gpui_vue :: gpui :: ElementId > :: into"));
    }

    #[test]
    fn pascal_structural_directives_reuse_keyed_native_lowering() {
        let loop_expansion = expand(&quote! {
            <Child
                v-for={item in items}
                :key={item.id}
                :props={ChildProps::new(item.label)}
            />
        })
        .expect("a keyed component loop should lower")
        .to_string();
        assert!(loop_expansion.contains("into_iter () . map"));
        assert!(loop_expansion.contains("item . id"));
        assert!(loop_expansion.contains("Option :: Some"));

        let conditional = expand(&quote! {
            <div>
                <Child v-if={ready} :props={ready_props} />
                <Child v-else :props={fallback_props} />
            </div>
        })
        .expect("component conditionals should reuse the sibling chain")
        .to_string();
        assert!(conditional.contains("when_else"));
        assert_eq!(conditional.matches("component_element").count(), 2);
    }

    #[test]
    fn pascal_individual_props_lower_through_the_generated_builder() {
        let expanded = expand(&quote! {
            <Child
                :label
                display-name="literal"
                payload={move_only}
                enabled
            />
        })
        .expect("individual props should lower")
        .to_string();
        assert!(
            expanded.contains("< Child as :: gpui_vue :: NativeComponent > :: Props :: builder")
        );
        assert!(expanded.contains("label (label)"));
        assert!(expanded.contains("display_name (\"literal\")"));
        assert!(expanded.contains("payload (move_only)"));
        assert!(expanded.contains("enabled (true)"));
        assert!(expanded.contains("build ()"));

        let empty = expand(&quote!(<Child />))
            .expect("zero-prop components should use the immediately complete builder")
            .to_string();
        assert!(empty.contains(
            "< Child as :: gpui_vue :: NativeComponent > :: Props :: builder () . build ()"
        ));
    }

    #[test]
    fn raw_keyword_component_props_lower_to_raw_methods_and_shorthand() {
        let explicit = expand(&quote!(<Child type={value} />))
            .expect("a keyword prop should lower through its raw Rust method")
            .to_string();
        assert!(explicit.contains("r#type (value)"));

        let shorthand = expand(&quote!(<Child :type />))
            .expect("keyword same-name shorthand should reference the raw local identifier")
            .to_string();
        assert!(shorthand.contains("r#type (r#type)"));

        let duplicate = "<Child type={first} r#type={second} />"
            .parse()
            .expect("raw and canonical prop spellings should tokenize");
        let duplicate = expand(&duplicate)
            .expect_err("raw and canonical spellings must share duplicate detection");
        assert!(
            duplicate
                .to_string()
                .contains("duplicate component prop `type`")
        );
    }

    #[test]
    fn path_keywords_are_targeted_instead_of_becoming_raw_ident_panics() {
        for keyword in ["self", "Self", "super", "crate"] {
            let source = format!("<Child {keyword}={{value}} />")
                .parse()
                .expect("the special-keyword fixture should tokenize");
            let error = expand(&source).expect_err("path keywords cannot name builder methods");
            assert!(error.to_string().contains("component prop"));
            assert!(error.to_string().contains("legal Rust identifier"));
        }
    }

    #[test]
    fn pascal_props_modes_are_exclusive_and_canonical_duplicates_fail() {
        let mixed = expand(&quote!(<Child :props={props} label={label} />)).unwrap_err();
        assert!(mixed.to_string().contains("cannot be mixed"));

        let reverse_mixed = expand(&quote!(<Child label={label} :props={props} />)).unwrap_err();
        assert!(reverse_mixed.to_string().contains("cannot be mixed"));

        let duplicate =
            expand(&quote!(<Child display-name="one" display_name="two" />)).unwrap_err();
        assert!(duplicate.to_string().contains("duplicate component prop"));
        assert!(duplicate.to_string().contains("display_name"));

        let invalid = expand(&quote!(<Child self="reserved path keyword" />)).unwrap_err();
        assert!(invalid.to_string().contains("legal Rust identifier"));
    }

    #[test]
    fn pascal_components_validate_duplicate_bindings_and_host_visibility() {
        let duplicate = expand(&quote!(<Child :props={first} :props={second} />)).unwrap_err();
        assert!(duplicate.to_string().contains("duplicate :props"));

        let empty_body = expand(&quote!(<Child :props={props}></Child>))
            .expect("an empty paired component tag is equivalent to a self-closing tag")
            .to_string();
        assert!(empty_body.contains("component_element"));

        let show = expand(&quote!(<Child v-show={visible} :props={props} />)).unwrap_err();
        assert!(show.to_string().contains("no layout wrapper"));

        let static_props = expand(&quote!(<Child :props="not an expression" />)).unwrap_err();
        assert!(
            static_props
                .to_string()
                .contains("must be a Rust expression")
        );

        let shorthand_slots = expand(&quote!(<Child :props={props} :slots />)).unwrap_err();
        assert!(
            shorthand_slots
                .to_string()
                .contains("must be a Rust expression")
        );
    }

    #[test]
    fn pascal_children_lower_to_default_named_and_scoped_slot_providers() {
        let source = r#"
            <Child :props={props}>
                <text>{owned.clone()}</text>
                <div>"second root"</div>
                <template #actions={ActionProps { count }}>
                    <text>{format!("{count}")}</text>
                </template>
                <template #footer>
                    <text>"footer"</text>
                </template>
            </Child>
        "#
        .parse()
        .expect("the fixture should tokenize");
        let expanded = expand(&source)
            .expect("declarative providers should lower")
            .to_string();

        assert!(expanded.contains("NativeComponentSlots > :: input_with_slots"));
        assert!(expanded.contains("NativeComponentSlots > :: Slots"));
        assert!(expanded.contains("with_default"));
        assert!(expanded.contains("with_actions"));
        assert!(expanded.contains("with_footer"));
        assert!(expanded.contains("move | ActionProps { count }"));
        assert!(expanded.contains("move | __gpui_vue_slot_props"));
        assert_eq!(expanded.matches("Slot :: new").count(), 3);
        assert!(!expanded.contains("Rc"));
    }

    #[test]
    fn declarative_slot_conflicts_and_unsupported_templates_are_targeted() {
        let mixed = expand(
            &r#"<Child :props={props} :slots={slots}><text>"mixed"</text></Child>"#
                .parse()
                .expect("the fixture should tokenize"),
        )
        .unwrap_err();
        assert!(mixed.to_string().contains("cannot be mixed"));

        let duplicate = expand(
            &r#"
                <Child :props={props}>
                    <template #actions><text>"one"</text></template>
                    <template #actions><text>"two"</text></template>
                </Child>
            "#
            .parse()
            .expect("the fixture should tokenize"),
        )
        .unwrap_err();
        assert!(duplicate.to_string().contains("duplicate component slot"));

        let duplicate_default = expand(
            &r#"
                <Child :props={props}>
                    <text>"implicit"</text>
                    <template #default><text>"explicit"</text></template>
                </Child>
            "#
            .parse()
            .expect("the fixture should tokenize"),
        )
        .unwrap_err();
        assert!(duplicate_default.to_string().contains("default slot"));

        let structural = expand(
            &r#"
                <Child :props={props}>
                    <template #actions v-if={ready}><text>"action"</text></template>
                </Child>
            "#
            .parse()
            .expect("the fixture should tokenize"),
        )
        .unwrap_err();
        assert!(structural.to_string().contains("structural directives"));

        let empty = expand(
            &r"<Child :props={props}><template #actions /></Child>"
                .parse()
                .expect("the fixture should tokenize"),
        )
        .unwrap_err();
        assert!(empty.to_string().contains("at least one child"));
    }

    #[test]
    fn standalone_slot_outlets_remain_targeted() {
        let outlet = expand(&quote!(<slot />)).unwrap_err();
        assert!(outlet.to_string().contains("direct component markup"));
        assert!(outlet.to_string().contains("typed slot explicitly"));
    }

    #[test]
    fn nested_absent_outlet_preserves_optional_child_cardinality() {
        let expanded = expand_component_template(
            &quote!(<div class="flex gap-2"><slot /></div>),
            &slotted_component_context(),
        )
        .expect("a nested unit outlet should lower")
        .to_string();

        assert!(expanded.contains(". children ("));
        assert!(expanded.contains("Option :: None"));
        assert!(!expanded.contains("gpui :: Empty"));
    }

    #[test]
    fn sole_root_outlet_uses_empty_only_at_the_render_boundary() {
        let expanded = expand_component_template(&quote!(<slot />), &slotted_component_context())
            .expect("a sole root outlet should lower")
            .to_string();

        assert!(expanded.contains("SlotContent :: new (:: gpui_vue :: gpui :: Empty)"));
        assert!(expanded.contains("NativeComponentSlots > :: slots"));
    }

    #[test]
    fn outlet_uses_the_exact_declared_raw_field_identifier() {
        let context = ComponentTemplateContext::new(
            syn::parse_quote!(this),
            syn::parse_quote!(window),
            syn::parse_quote!(cx),
            vec![ComponentSlotMetadata::new(
                syn::parse_quote!(r#type),
                syn::parse_quote!(()),
            )],
        );
        let expanded = expand_component_template(&quote!(<slot name="type" />), &context)
            .expect("raw Rust slot identifiers should retain their exact field token")
            .to_string();

        assert!(expanded.contains(". r#type . is_present"));
    }

    #[test]
    fn raw_keyword_named_provider_uses_an_unraw_fluent_setter() {
        let input = r#"<Child><template #type><text>"raw provider"</text></template></Child>"#
            .parse()
            .expect("the raw-keyword provider fixture should tokenize");
        let expanded = expand(&input)
            .expect("`#type` is a valid named provider for a raw Rust slot field")
            .to_string();

        assert!(expanded.contains(". with_type"));
        assert!(!expanded.contains("with_r#type"));
    }

    #[test]
    fn named_outlet_props_and_nested_fallback_are_lazy_and_typed() {
        let expanded = expand_component_template(
            &quote! {
                <div>
                    <slot name="actions" :props={ActionProps { count }}>
                        <slot><text>"nested fallback"</text></slot>
                    </slot>
                </div>
            },
            &slotted_component_context(),
        )
        .expect("named scoped outlets and nested fallbacks should lower")
        .to_string();

        assert_eq!(expanded.matches("ActionProps { count }").count(), 1);
        assert!(expanded.contains(". actions . is_present"));
        assert!(expanded.contains("SlotContent :: new"));
        assert!(expanded.contains(". default . is_present"));
    }

    #[test]
    fn repeated_outlets_are_rejected_before_gpui_identity_can_collide() {
        let error = expand_component_template(
            &quote! {
                <div>
                    <slot />
                    <slot />
                </div>
            },
            &slotted_component_context(),
        )
        .expect_err("one provider cannot be invoked at two sibling outlet identities");

        assert!(error.to_string().contains("more than one outlet"));
        assert!(error.to_string().contains("distinct GPUI identity"));
    }

    #[test]
    fn outlet_structural_directives_have_host_specific_errors() {
        let context = slotted_component_context();
        let conditional = expand_component_template(&quote!(<slot v-if={ready} />), &context)
            .expect_err("an outlet condition should require a structural template");
        assert!(conditional.to_string().contains("<template v-if"));

        let repeated = expand_component_template(
            &quote!(<slot v-for={item in items} :key={item.id} />),
            &context,
        )
        .expect_err("a wrapper-free repeated outlet has no identity namespace");
        assert!(repeated.to_string().contains("no GPUI fragment identity"));

        let shown = expand_component_template(&quote!(<slot v-show={visible} />), &context)
            .expect_err("an outlet has no v-show host");
        assert!(shown.to_string().contains("no rendered host element"));
    }

    #[test]
    fn contextual_declarative_providers_reenter_the_live_parent_entity() {
        let source = r#"
            <Child>
                <text>{this.props().label.clone()}</text>
                <template #actions={ActionProps { count }}>
                    <Child><text>{format!("{count}:{}", this.live)}</text></Child>
                </template>
            </Child>
        "#
        .parse()
        .expect("the contextual provider fixture should tokenize");
        let expanded = expand_component_template(
            &source,
            &ComponentTemplateContext::new(
                syn::parse_quote!(this),
                syn::parse_quote!(window),
                syn::parse_quote!(cx),
                Vec::new(),
            ),
        )
        .expect("component-aware providers should lower through WeakEntity")
        .to_string();

        assert_eq!(expanded.matches("weak_entity ()").count(), 3);
        assert_eq!(expanded.matches(". update (").count(), 3);
        assert!(expanded.contains("SlotContent :: new (:: gpui_vue :: gpui :: Empty)"));
        assert!(expanded.contains("let ActionProps { count }"));
    }

    #[test]
    fn pascal_component_events_share_one_typed_monomorphic_host() {
        let expanded = expand(&quote! {
            <Child
                :props={props}
                @value-change={make_value_handler()}
                on:close={close_handler}
            />
        })
        .expect("typed component events should lower")
        .to_string();

        assert_eq!(expanded.matches("component_element_with_events").count(), 1);
        assert!(!expanded.contains("component_element :: < Child"));
        assert!(expanded.contains("< Child as :: gpui_vue :: NativeComponentEvents > :: Event"));
        assert_eq!(expanded.matches("make_value_handler ()").count(), 1);
        assert_eq!(expanded.matches("close_handler").count(), 1);
        assert!(expanded.contains("__gpui_vue_dispatch_value_change"));
        assert!(expanded.contains("__gpui_vue_dispatch_close"));
        assert!(!expanded.contains("Box"));
        assert!(!expanded.contains("Vec"));
    }

    #[test]
    fn pascal_component_event_modifiers_and_duplicates_are_rejected() {
        let modifier = expand(&quote! {
            <Child :props={props} @save.stop={handler} />
        })
        .unwrap_err();
        assert!(modifier.to_string().contains("modifiers are not supported"));
        assert!(modifier.to_string().contains(".stop"));

        let once = expand(&quote! {
            <Child :props={props} on:save.once={handler} />
        })
        .unwrap_err();
        assert!(once.to_string().contains("modifiers are not supported"));
        assert!(once.to_string().contains(".once"));

        let duplicate = expand(&quote! {
            <Child :props={props} @value-change={first} on:value_change={second} />
        })
        .unwrap_err();
        assert!(
            duplicate
                .to_string()
                .contains("duplicate component event `value_change`")
        );
    }
}
