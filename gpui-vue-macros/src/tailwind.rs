//! Compile a statically known Tailwind-like class list into GPUI style calls.
//!
//! The compiler deliberately uses a small typed cascade instead of replaying
//! class strings verbatim. Supported shorthand families use Tailwind's broad,
//! axis, then side canonical order, independent of class-attribute order.
//! Equal-rank utilities still use the later candidate as a documented partial
//! compatibility boundary. `!important` remains entirely compile-time, and no
//! class parser is emitted into the application.
//!
//! GPUI does not expose CSS `currentColor` for borders. A width-only `border`
//! candidate therefore emits only width and may remain invisible; callers must
//! add an explicit `border-<color>` utility for portable rendering.

use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Error, LitStr, Result};

/// Tailwind CSS 4.3.2 default colors pre-lowered to packed sRGB.
#[path = "tailwind_palette.rs"]
mod tailwind_palette;

/// The private GPUI group name used by Tailwind's unqualified `group` marker.
///
/// GPUI pushes same-name group hitboxes while painting descendants and reads
/// only the last stack entry. Nested unqualified groups therefore resolve to
/// the nearest group, whereas Tailwind's selector can match any unqualified
/// group ancestor. Named groups are left unsupported until that distinction can
/// be represented explicitly.
const DEFAULT_GROUP_NAME: &str = "__gpui_vue_tailwind_group";

/// The GPUI state groups produced from one class literal.
#[derive(Debug, Default)]
pub(crate) struct CompiledClasses {
    /// Whether this element establishes Tailwind's default interaction group.
    group: bool,
    /// Declarations that apply in the element's ordinary state.
    regular: Cascade,
    /// Declarations that apply while this element is inside a focused ancestor.
    ///
    /// GPUI's native `within_focused` predicate also includes the tracked
    /// element itself. Tailwind's implicit `in-focus` selector normally names
    /// an ancestor, so self-focus is a bounded host divergence.
    in_focus: Cascade,
    /// Declarations that apply while the pointer hovers the element.
    hover: Cascade,
    /// Declarations that apply while the pointer hovers the nearest default group.
    group_hover: Cascade,
    /// Declarations that apply while the element is active.
    active: Cascade,
    /// Declarations that apply while the nearest default group is active.
    group_active: Cascade,
    /// Declarations that apply while the element has focus.
    focus: Cascade,
}

impl CompiledClasses {
    /// Parses and resolves a static class literal at macro-expansion time.
    pub(crate) fn parse(literal: &LitStr) -> Result<Self> {
        let mut classes = Self::default();
        for source in literal.value().split_whitespace() {
            if source == "group" {
                classes.group = true;
                continue;
            }
            let candidate = Candidate::parse(source, literal.span())?;
            let cascade = match candidate.variant {
                Variant::Regular => &mut classes.regular,
                Variant::InFocus => &mut classes.in_focus,
                Variant::Hover => &mut classes.hover,
                Variant::GroupHover => &mut classes.group_hover,
                Variant::Active => &mut classes.active,
                Variant::GroupActive => &mut classes.group_active,
                Variant::Focus => &mut classes.focus,
            };
            cascade.insert(candidate.utility, candidate.important);
        }
        classes.validate_simultaneous_states(literal.span())?;
        Ok(classes)
    }

    /// Rejects state combinations whose winner cannot survive GPUI's fixed
    /// refinement order.
    ///
    /// Tailwind 4.3.2 compares `!important`, selector specificity, and then its
    /// generated variant order. GPUI stores each callback as a plain
    /// `StyleRefinement`; cross-callback importance is unavailable, and
    /// `compute_style_internal` applies the callbacks in one fixed order. A
    /// pair is accepted only when both engines therefore select the same
    /// declaration for every shared property slot.
    fn validate_simultaneous_states(&self, span: Span) -> Result<()> {
        let group_active_can_apply = self
            .group_active
            .has_effective_style_declarations(&self.regular);
        if self.group && group_active_can_apply {
            return Err(Error::new(
                span,
                "`group` and `group-active:` cannot target the same element: GPUI 0.2.2 captures the element's own group hitbox before its active mouse handler, while Tailwind `group-active:` selects descendants of the active group rather than the group element itself",
            ));
        }

        let states = [
            (Variant::InFocus, &self.in_focus),
            (Variant::Hover, &self.hover),
            (Variant::GroupHover, &self.group_hover),
            (Variant::Active, &self.active),
            (Variant::GroupActive, &self.group_active),
            (Variant::Focus, &self.focus),
        ];

        for (left_index, (left_variant, left_cascade)) in states.iter().enumerate() {
            for (right_variant, right_cascade) in &states[left_index + 1..] {
                for left in &left_cascade.declarations {
                    // A regular important declaration is deliberately omitted
                    // from a non-important state callback, so it cannot
                    // participate in a cross-state runtime conflict.
                    if self.regular.blocks_state(left) {
                        continue;
                    }
                    let Some(right) = right_cascade.declaration(left.property) else {
                        continue;
                    };
                    if self.regular.blocks_state(right) {
                        continue;
                    }

                    let tailwind_winner = Variant::tailwind_winner(
                        *left_variant,
                        left.important,
                        *right_variant,
                        right.important,
                    );
                    let gpui_winner = Variant::gpui_winner(*left_variant, *right_variant);
                    if tailwind_winner != gpui_winner {
                        let importance_boundary = if left.important == right.important {
                            ""
                        } else {
                            "; GPUI state refinements cannot carry cross-state `!important`"
                        };
                        return Err(Error::new(
                            span,
                            format!(
                                "simultaneous `{left}:` and `{right}:` assignments to property `{property:?}` cannot preserve Tailwind 4.3.2 cascade semantics: Tailwind lets `{tailwind}:` win, but GPUI 0.2.2 refines `{gpui}:` last{importance_boundary}; use distinct properties or remove one of these state assignments",
                                left = left_variant.name(),
                                right = right_variant.name(),
                                property = left.property,
                                tailwind = tailwind_winner.name(),
                                gpui = gpui_winner.name(),
                            ),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Reports whether selectors or retained layout operations need an element id.
    pub(crate) fn needs_stateful_id(&self) -> bool {
        self.regular.requires_stateful_id()
            || self.active.has_effective_style_declarations(&self.regular)
            || self
                .group_active
                .has_effective_style_declarations(&self.regular)
            || self.focus.has_effective_style_declarations(&self.regular)
            || self
                .in_focus
                .has_effective_style_declarations(&self.regular)
    }

    /// Reports whether focus styling requires making the element focusable.
    pub(crate) fn needs_focusable(&self) -> bool {
        self.focus.has_effective_style_declarations(&self.regular)
            || self
                .in_focus
                .has_effective_style_declarations(&self.regular)
    }

    /// Appends the resolved ordinary-state style calls to `target`.
    pub(crate) fn apply_regular(
        &self,
        target: TokenStream,
        crate_path: &TokenStream,
    ) -> TokenStream {
        let mut output = TokenStream::new();
        output.extend(target);
        output = self.regular.apply(&output, crate_path, None);
        if self.group {
            output = quote!(#output.group(#DEFAULT_GROUP_NAME));
        }
        output
    }

    /// Appends post-identity retained state and interaction callbacks to `target`.
    pub(crate) fn apply_variants(
        &self,
        target: TokenStream,
        crate_path: &TokenStream,
    ) -> TokenStream {
        let mut output = TokenStream::new();
        output.extend(target);

        // Stateful overflow builders are unavailable until GPUI's `.id(...)`
        // has changed the element's type. Static classes reach this phase only
        // after identity lowering; dynamic class branches are also evaluated
        // from the already-identified element.
        output = self.regular.apply_stateful(&output, crate_path);

        let has_in_focus = self
            .in_focus
            .has_effective_style_declarations(&self.regular);
        let has_hover = self.hover.has_effective_style_declarations(&self.regular);
        let has_group_hover = self
            .group_hover
            .has_effective_style_declarations(&self.regular);
        let has_active = self.active.has_effective_style_declarations(&self.regular);
        let has_group_active = self
            .group_active
            .has_effective_style_declarations(&self.regular);
        let has_focus = self.focus.has_effective_style_declarations(&self.regular);

        if has_in_focus {
            // This deliberately uses GPUI's native focus-tree seam. It avoids
            // window-context checks in generated render code, while inheriting
            // GPUI's ancestor-or-self behavior documented on `in_focus` above.
            let style =
                self.in_focus
                    .apply(&quote!(__gpui_vue_style), crate_path, Some(&self.regular));
            output = quote!(#output.in_focus(|__gpui_vue_style| #style));
        }
        if has_hover {
            let style =
                self.hover
                    .apply(&quote!(__gpui_vue_style), crate_path, Some(&self.regular));
            output = quote!(#output.hover(|__gpui_vue_style| #style));
        }
        if has_group_hover {
            let style =
                self.group_hover
                    .apply(&quote!(__gpui_vue_style), crate_path, Some(&self.regular));
            output = quote!(#output.group_hover(
                #DEFAULT_GROUP_NAME,
                |__gpui_vue_style| #style,
            ));
        } else if (has_active || has_group_active) && !has_hover {
            // GPUI 0.2.2 does not include `active_style` or
            // `group_active_style` in its hitbox predicate. An empty hover
            // refinement creates the target hitbox needed by the native
            // mouse-down handler without changing any visual property.
            output = quote!(#output.hover(|__gpui_vue_style| __gpui_vue_style));
        }
        if has_active {
            let style =
                self.active
                    .apply(&quote!(__gpui_vue_style), crate_path, Some(&self.regular));
            output = quote!(#output.active(|__gpui_vue_style| #style));
        }
        if has_group_active {
            let style =
                self.group_active
                    .apply(&quote!(__gpui_vue_style), crate_path, Some(&self.regular));
            output = quote!(#output.group_active(
                #DEFAULT_GROUP_NAME,
                |__gpui_vue_style| #style,
            ));
        }
        if has_focus {
            let style =
                self.focus
                    .apply(&quote!(__gpui_vue_style), crate_path, Some(&self.regular));
            output = quote!(#output.focus(|__gpui_vue_style| #style));
        }
        output
    }
}

/// A parsed class candidate before it is merged into a state cascade.
#[derive(Debug)]
struct Candidate {
    /// The optional GPUI interaction state selected by the candidate.
    variant: Variant,
    /// The candidate's typed property declarations.
    utility: Utility,
    /// Whether the trailing `!` raises this candidate above normal declarations.
    important: bool,
}

impl Candidate {
    /// Parses one whitespace-delimited Tailwind candidate.
    fn parse(source: &str, span: Span) -> Result<Self> {
        let (variant, utility_source) = split_variant(source, span)?;
        let (utility_source, important) = match utility_source.strip_suffix('!') {
            Some(utility) if !utility.is_empty() && !utility.ends_with('!') => (utility, true),
            Some(_) => {
                return Err(Error::new(
                    span,
                    format!("invalid important candidate `{source}`; use one trailing `!`"),
                ));
            }
            None => (utility_source, false),
        };

        let utility = Utility::parse(utility_source, span)?;
        if variant != Variant::Regular && utility.requires_stateful_id() {
            return Err(Error::new(
                span,
                format!(
                    "`{source}` changes GPUI retained overflow state; overflow scroll utilities are supported only without an interaction variant because state-style callbacks cannot change retained layout state"
                ),
            ));
        }

        Ok(Self {
            variant,
            utility,
            important,
        })
    }
}

/// A supported interaction variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Variant {
    /// No interaction variant.
    Regular,
    /// The `in-focus:` variant.
    InFocus,
    /// The `hover:` variant.
    Hover,
    /// The `group-hover:` variant for the default group.
    GroupHover,
    /// The `active:` variant.
    Active,
    /// The `group-active:` variant for the default group.
    GroupActive,
    /// The `focus:` variant.
    Focus,
}

impl Variant {
    /// Returns the spelling used in a class prefix and in diagnostics.
    fn name(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::InFocus => "in-focus",
            Self::Hover => "hover",
            Self::GroupHover => "group-hover",
            Self::Active => "active",
            Self::GroupActive => "group-active",
            Self::Focus => "focus",
        }
    }

    /// Effective low-to-high Tailwind 4.3.2 precedence for this supported set.
    ///
    /// `in-focus:` is emitted after the other variants, but its ancestor is
    /// wrapped in `:where(...)`; it consequently has lower specificity than
    /// the target pseudo-class and group variants. The remaining variants have
    /// equal specificity here and follow the generated stylesheet order.
    fn tailwind_precedence(self) -> u8 {
        match self {
            Self::Regular => 0,
            Self::InFocus => 1,
            Self::GroupHover => 2,
            Self::GroupActive => 3,
            Self::Hover => 4,
            Self::Focus => 5,
            Self::Active => 6,
        }
    }

    /// Low-to-high order in GPUI 0.2.2's `compute_style_internal`.
    fn gpui_refinement_order(self) -> u8 {
        match self {
            Self::Regular => 0,
            Self::InFocus => 1,
            Self::Focus => 2,
            Self::GroupHover => 3,
            Self::Hover => 4,
            Self::GroupActive => 5,
            Self::Active => 6,
        }
    }

    /// Selects the declaration Tailwind would retain for one shared property.
    fn tailwind_winner(
        left: Self,
        left_important: bool,
        right: Self,
        right_important: bool,
    ) -> Self {
        match (left_important, right_important) {
            (true, false) => left,
            (false, true) => right,
            _ if left.tailwind_precedence() > right.tailwind_precedence() => left,
            _ => right,
        }
    }

    /// Selects the declaration GPUI retains after all active refinements.
    fn gpui_winner(left: Self, right: Self) -> Self {
        if left.gpui_refinement_order() > right.gpui_refinement_order() {
            left
        } else {
            right
        }
    }
}

/// Splits the optional variant from a utility while rejecting stacked variants.
fn split_variant(source: &str, span: Span) -> Result<(Variant, &str)> {
    let Some((variant, utility)) = source.split_once(':') else {
        return Ok((Variant::Regular, source));
    };
    if utility.contains(':') {
        return Err(Error::new(
            span,
            format!(
                "stacked variants are not supported yet in `{source}`; supported single variants: in-focus, hover, group-hover, active, group-active, focus"
            ),
        ));
    }
    let variant = match variant {
        "in-focus" => Variant::InFocus,
        "hover" => Variant::Hover,
        "group-hover" => Variant::GroupHover,
        "active" => Variant::Active,
        "group-active" => Variant::GroupActive,
        "focus" => Variant::Focus,
        "focus-within" => {
            return Err(Error::new(
                span,
                "Tailwind `focus-within:` selects a target whose self or descendant has focus, but GPUI 0.2.2 exposes no fluent style seam for `contains_focused`; GPUI `in_focus` instead selects a target inside a focused ancestor, available as `in-focus:`",
            ));
        }
        unknown => {
            return Err(Error::new(
                span,
                format!(
                    "unsupported class variant `{unknown}:`; supported: in-focus, hover, group-hover, active, group-active, focus"
                ),
            ));
        }
    };
    if utility.is_empty() {
        return Err(Error::new(span, format!("missing utility in `{source}`")));
    }
    Ok((variant, utility))
}

/// The winning declaration for every property in one interaction state.
#[derive(Debug, Default)]
struct Cascade {
    /// Property declarations in their final source order.
    declarations: Vec<Declaration>,
}

impl Cascade {
    /// Inserts a utility using important and partial canonical utility order.
    fn insert(&mut self, utility: Utility, important: bool) {
        for utility_declaration in utility.declarations {
            let UtilityDeclaration {
                property,
                value,
                canonical_rank,
            } = utility_declaration;
            if let Some(index) = self
                .declarations
                .iter()
                .position(|declaration| declaration.property == property)
            {
                if self.declarations[index].important && !important {
                    continue;
                }
                if self.declarations[index].important == important
                    && self.declarations[index].canonical_rank > canonical_rank
                {
                    continue;
                }
                self.declarations.remove(index);
            }
            self.declarations.push(Declaration {
                property,
                value,
                important,
                canonical_rank,
            });
        }
    }

    /// Returns this cascade's resolved declaration for one property slot.
    fn declaration(&self, property: Property) -> Option<&Declaration> {
        self.declarations
            .iter()
            .find(|declaration| declaration.property == property)
    }

    /// Reports whether a regular important declaration suppresses this state.
    fn blocks_state(&self, state: &Declaration) -> bool {
        self.has_important(state.property) && !state.important
    }

    /// Reports whether this state contributes a non-retained style field after
    /// regular-state importance is applied.
    fn has_effective_style_declarations(&self, regular: &Self) -> bool {
        self.declarations.iter().any(|declaration| {
            !regular.blocks_state(declaration) && !declaration.value.requires_stateful_id()
        })
    }

    /// Emits this cascade, respecting important declarations from a base state.
    fn apply(
        &self,
        target: &TokenStream,
        crate_path: &TokenStream,
        base: Option<&Self>,
    ) -> TokenStream {
        let mut output = target.clone();
        for declaration in &self.declarations {
            let base_blocks = base.is_some_and(|base| {
                base.has_important(declaration.property) && !declaration.important
            });
            if !base_blocks && !declaration.value.requires_stateful_id() {
                output = declaration.value.apply(&output, crate_path);
            }
        }
        output
    }

    /// Emits retained-state operations after GPUI identity lowering.
    fn apply_stateful(&self, target: &TokenStream, crate_path: &TokenStream) -> TokenStream {
        let mut output = target.clone();
        for declaration in &self.declarations {
            if declaration.value.requires_stateful_id() {
                output = declaration.value.apply(&output, crate_path);
            }
        }
        output
    }

    /// Reports whether the resolved cascade contains a retained-state operation.
    fn requires_stateful_id(&self) -> bool {
        self.declarations
            .iter()
            .any(|declaration| declaration.value.requires_stateful_id())
    }

    /// Reports whether `property` has an important declaration in this cascade.
    fn has_important(&self, property: Property) -> bool {
        self.declarations
            .iter()
            .any(|declaration| declaration.property == property && declaration.important)
    }
}

/// One resolved assignment to a typed GPUI style property.
#[derive(Debug)]
struct Declaration {
    /// The conflict slot assigned by this declaration.
    property: Property,
    /// The GPUI operation that writes the property.
    value: PropertyValue,
    /// Whether this assignment beats all non-important assignments.
    important: bool,
    /// Stylesheet order within a supported conflict family.
    canonical_rank: u128,
}

/// A parsed utility represented as one or more typed assignments.
#[derive(Debug)]
struct Utility {
    /// The assignments made by this utility; shorthands are expanded here.
    declarations: Vec<UtilityDeclaration>,
}

/// One property assignment before importance and winner resolution are known.
#[derive(Debug)]
struct UtilityDeclaration {
    /// The independently cascading GPUI field written by this assignment.
    property: Property,
    /// An operation that writes only `property`.
    value: PropertyValue,
    /// Tailwind stylesheet order within this property's conflict family.
    canonical_rank: u128,
}

impl Utility {
    /// Parses a supported utility into its GPUI property assignments.
    fn parse(class: &str, span: Span) -> Result<Self> {
        if let Some(utility) = parse_radius_utility(class, span) {
            return Ok(utility);
        }
        if let Some(utility) = parse_grid_tracks(class, span)? {
            return Ok(utility);
        }
        if let Some(utility) = parse_grid_placement(class, span)? {
            return Ok(utility);
        }
        if let Some(utility) = parse_aspect_ratio(class, span)? {
            return Ok(utility);
        }
        if let Some(utility) = parse_line_height(class, span)? {
            return Ok(utility);
        }
        if let Some(utility) = parse_alignment_utility(class, span)? {
            return Ok(utility);
        }
        if let Some(utility) = parse_exact_utility(class, span) {
            return Ok(utility);
        }
        reject_unrepresentable_layout_utility(class, span)?;
        if let Some(utility) = parse_border_width(class, span)? {
            return Ok(utility);
        }
        if let Some(utility) = parse_length_utility(class, span)? {
            return Ok(utility);
        }
        if let Some(weight) = class.strip_prefix("font-").and_then(font_weight) {
            return Ok(Self::single(
                Property::FontWeight,
                PropertyValue::FontWeight(format_ident!("{weight}", span = span)),
            ));
        }
        if let Some(size) = class.strip_prefix("text-").and_then(text_size) {
            return Ok(Self::single(
                Property::FontSize,
                PropertyValue::TextSize(size),
            ));
        }
        if let Some(utility) = parse_color_utility(class, span)? {
            return Ok(utility);
        }
        if let Some(opacity) = parse_opacity(class, span)? {
            return Ok(Self::single(
                Property::Opacity,
                PropertyValue::Opacity(opacity),
            ));
        }
        if let Some(lines) = class
            .strip_prefix("line-clamp-")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|lines| *lines > 0)
        {
            let line_rank = u128::try_from(lines).unwrap_or(u128::MAX);
            return Ok(Self {
                declarations: vec![
                    UtilityDeclaration {
                        property: Property::OverflowX,
                        value: PropertyValue::Method(format_ident!(
                            "overflow_x_hidden",
                            span = span
                        )),
                        canonical_rank: 0,
                    },
                    UtilityDeclaration {
                        property: Property::OverflowY,
                        value: PropertyValue::Method(format_ident!(
                            "overflow_y_hidden",
                            span = span
                        )),
                        canonical_rank: 0,
                    },
                    UtilityDeclaration {
                        property: Property::LineClamp,
                        value: PropertyValue::LineClamp(lines),
                        canonical_rank: line_rank,
                    },
                ],
            });
        }

        Err(Error::new(
            span,
            format!("unknown gpui-vue Tailwind utility `{class}`"),
        ))
    }

    /// Creates a utility that writes exactly one default-ranked property slot.
    fn single(property: Property, value: PropertyValue) -> Self {
        Self::single_ranked(property, value, 0)
    }

    /// Creates a utility that writes exactly one ranked property slot.
    fn single_ranked(property: Property, value: PropertyValue, canonical_rank: u128) -> Self {
        Self {
            declarations: vec![UtilityDeclaration {
                property,
                value,
                canonical_rank,
            }],
        }
    }

    /// Creates a default-ranked method-call utility that writes one property slot.
    fn method(property: Property, method: &str, span: Span) -> Self {
        Self::single(
            property,
            PropertyValue::Method(format_ident!("{method}", span = span)),
        )
    }

    /// Creates a ranked method-call utility that writes one property slot.
    fn ranked_method(property: Property, method: &str, span: Span, canonical_rank: u128) -> Self {
        Self::single_ranked(
            property,
            PropertyValue::Method(format_ident!("{method}", span = span)),
            canonical_rank,
        )
    }

    /// Creates a ranked stateful method that must be emitted after `.id(...)`.
    fn ranked_stateful_method(
        property: Property,
        method: &str,
        span: Span,
        canonical_rank: u128,
    ) -> Self {
        Self::single_ranked(
            property,
            PropertyValue::StatefulMethod(format_ident!("{method}", span = span)),
            canonical_rank,
        )
    }

    /// Creates a ranked direct assignment to one public GPUI style enum.
    fn ranked_style_enum(
        property: Property,
        field: &str,
        enum_type: &str,
        variant: &str,
        span: Span,
        canonical_rank: u128,
    ) -> Self {
        Self {
            declarations: vec![style_enum_declaration(
                property,
                field,
                enum_type,
                variant,
                span,
                canonical_rank,
            )],
        }
    }

    /// Creates a utility from declarations whose methods each mutate one slot.
    fn ranked_methods<const N: usize>(
        declarations: [(Property, &str, u128); N],
        span: Span,
    ) -> Self {
        Self {
            declarations: declarations
                .into_iter()
                .map(|(property, method, canonical_rank)| UtilityDeclaration {
                    property,
                    value: PropertyValue::Method(format_ident!("{method}", span = span)),
                    canonical_rank,
                })
                .collect(),
        }
    }

    /// Reports whether any resolved declaration needs GPUI's stateful wrapper.
    fn requires_stateful_id(&self) -> bool {
        self.declarations
            .iter()
            .any(|declaration| declaration.value.requires_stateful_id())
    }
}

/// Every independently cascading style property supported by the P0 compiler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Property {
    /// CSS display.
    Display,
    /// CSS visibility.
    Visibility,
    /// Flex direction.
    FlexDirection,
    /// Flex wrapping.
    FlexWrap,
    /// Flex growth.
    FlexGrow,
    /// Flex shrinking.
    FlexShrink,
    /// Flex basis.
    FlexBasis,
    /// Cross-axis item alignment.
    AlignItems,
    /// Per-item cross-axis alignment.
    AlignSelf,
    /// Main-axis content justification.
    JustifyContent,
    /// Multi-line content alignment.
    AlignContent,
    /// CSS positioning mode.
    Position,
    /// Horizontal overflow.
    OverflowX,
    /// Vertical overflow.
    OverflowY,
    /// Pointer cursor.
    Cursor,
    /// Text whitespace behavior.
    WhiteSpace,
    /// Text overflow marker behavior.
    TextOverflow,
    /// Maximum number of laid-out text lines.
    LineClamp,
    /// Text alignment.
    TextAlign,
    /// Font style.
    FontStyle,
    /// Text decoration.
    TextDecoration,
    /// Border rendering style.
    BorderStyle,
    /// Border top width.
    BorderTop,
    /// Border right width.
    BorderRight,
    /// Border bottom width.
    BorderBottom,
    /// Border left width.
    BorderLeft,
    /// Top-left corner radius.
    RadiusTopLeft,
    /// Top-right corner radius.
    RadiusTopRight,
    /// Bottom-right corner radius.
    RadiusBottomRight,
    /// Bottom-left corner radius.
    RadiusBottomLeft,
    /// Box-shadow group.
    Shadow,
    /// Background color.
    BackgroundColor,
    /// Foreground text color.
    TextColor,
    /// Border color.
    BorderColor,
    /// Font weight.
    FontWeight,
    /// Font size.
    FontSize,
    /// Element opacity.
    Opacity,
    /// Top margin.
    MarginTop,
    /// Right margin.
    MarginRight,
    /// Bottom margin.
    MarginBottom,
    /// Left margin.
    MarginLeft,
    /// Top padding.
    PaddingTop,
    /// Right padding.
    PaddingRight,
    /// Bottom padding.
    PaddingBottom,
    /// Left padding.
    PaddingLeft,
    /// Width.
    Width,
    /// Height.
    Height,
    /// Minimum width.
    MinWidth,
    /// Minimum height.
    MinHeight,
    /// Maximum width.
    MaxWidth,
    /// Maximum height.
    MaxHeight,
    /// Horizontal layout gap.
    GapX,
    /// Vertical layout gap.
    GapY,
    /// Top positioned inset.
    InsetTop,
    /// Right positioned inset.
    InsetRight,
    /// Bottom positioned inset.
    InsetBottom,
    /// Left positioned inset.
    InsetLeft,
    /// Explicit grid-template column count.
    GridColumns,
    /// Explicit grid-template row count.
    GridRows,
    /// Grid column-start placement.
    GridColumnStart,
    /// Grid column-end placement.
    GridColumnEnd,
    /// Grid row-start placement.
    GridRowStart,
    /// Grid row-end placement.
    GridRowEnd,
    /// Preferred width-to-height ratio.
    AspectRatio,
    /// Text line height.
    LineHeight,
}

/// One GPUI grid-location axis selected by a placement declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GridAxis {
    /// The column range.
    Column,
    /// The row range.
    Row,
}

/// One endpoint within a GPUI grid-location range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GridEdge {
    /// Range start.
    Start,
    /// Range end.
    End,
}

/// A GPUI grid placement that can be lowered without parsing at runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GridPlacementValue {
    /// Automatic placement.
    Auto,
    /// An explicit nonzero grid line.
    Line(i16),
    /// A positive track span.
    Span(u16),
}

/// A GPUI builder operation stored in a typed property slot.
#[derive(Debug)]
enum PropertyValue {
    /// A zero-argument `Styled` method.
    Method(Ident),
    /// A zero-argument method available only after GPUI identity lowering.
    StatefulMethod(Ident),
    /// A color setter and packed color value.
    Color {
        /// GPUI color setter method.
        method: Ident,
        /// Parsed packed color.
        color: PackedColor,
    },
    /// A GPUI font-weight constant.
    FontWeight(Ident),
    /// A font size measured in rem units.
    TextSize(f32),
    /// An opacity in the inclusive zero-to-one range.
    Opacity(f32),
    /// A GPUI length setter and parsed length.
    Length {
        /// GPUI length setter method.
        method: Ident,
        /// Static length passed to the setter.
        value: LengthValue,
    },
    /// A uniform count of `minmax(0, 1fr)` grid tracks.
    GridTracks {
        /// GPUI grid-track setter method.
        method: Ident,
        /// Positive track count accepted by GPUI 0.2.2.
        count: u16,
    },
    /// One endpoint of a row or column grid placement.
    GridPlacement {
        /// Row or column range.
        axis: GridAxis,
        /// Start or end endpoint.
        edge: GridEdge,
        /// Static placement value.
        value: GridPlacementValue,
    },
    /// A direct floating-point assignment to one GPUI style-refinement field.
    StyleFloat {
        /// GPUI `StyleRefinement` field name.
        field: Ident,
        /// Value assigned to that field.
        value: f32,
    },
    /// A direct assignment of one public GPUI style enum.
    StyleEnum {
        /// GPUI `StyleRefinement` field name.
        field: Ident,
        /// Public GPUI enum (or type-alias) name.
        enum_type: Ident,
        /// Exact enum variant assigned to the field.
        variant: Ident,
    },
    /// A positive number of visible text lines.
    LineClamp(usize),
}

impl PropertyValue {
    /// Appends this operation to an element or state-style token expression.
    fn apply(&self, target: &TokenStream, crate_path: &TokenStream) -> TokenStream {
        match self {
            Self::Method(method) | Self::StatefulMethod(method) => quote!(#target.#method()),
            Self::Color { method, color } => {
                let color = color.tokens(crate_path);
                quote!(#target.#method(#color))
            }
            Self::FontWeight(weight) => {
                quote!(#target.font_weight(#crate_path::gpui::FontWeight::#weight))
            }
            Self::TextSize(rems) => quote!(#target.text_size(#crate_path::gpui::rems(#rems))),
            Self::Opacity(opacity) => quote!(#target.opacity(#opacity)),
            Self::Length { method, value } => {
                let value = value.tokens(crate_path);
                quote!(#target.#method(#value))
            }
            Self::GridTracks { method, count } => quote!(#target.#method(#count)),
            Self::GridPlacement { axis, edge, value } => {
                let axis = match axis {
                    GridAxis::Column => format_ident!("column"),
                    GridAxis::Row => format_ident!("row"),
                };
                let edge = match edge {
                    GridEdge::Start => format_ident!("start"),
                    GridEdge::End => format_ident!("end"),
                };
                let value = match value {
                    GridPlacementValue::Auto => quote!(#crate_path::gpui::GridPlacement::Auto),
                    GridPlacementValue::Line(line) => {
                        quote!(#crate_path::gpui::GridPlacement::Line(#line))
                    }
                    GridPlacementValue::Span(span) => {
                        quote!(#crate_path::gpui::GridPlacement::Span(#span))
                    }
                };
                quote!({
                    let mut __gpui_vue_styled = #target;
                    __gpui_vue_styled
                        .style()
                        .grid_location_mut()
                        .#axis
                        .#edge = #value;
                    __gpui_vue_styled
                })
            }
            Self::StyleFloat { field, value } => quote!({
                let mut __gpui_vue_styled = #target;
                __gpui_vue_styled.style().#field = ::core::option::Option::Some(#value);
                __gpui_vue_styled
            }),
            Self::StyleEnum {
                field,
                enum_type,
                variant,
            } => quote!({
                let mut __gpui_vue_styled = #target;
                __gpui_vue_styled.style().#field = ::core::option::Option::Some(
                    #crate_path::gpui::#enum_type::#variant,
                );
                __gpui_vue_styled
            }),
            Self::LineClamp(lines) => quote!({
                let mut __gpui_vue_styled = #target;
                __gpui_vue_styled
                    .text_style()
                    .get_or_insert_with(::core::default::Default::default)
                    .line_clamp = ::core::option::Option::Some(#lines);
                __gpui_vue_styled
            }),
        }
    }

    /// Reports whether this operation needs GPUI's stateful element wrapper.
    fn requires_stateful_id(&self) -> bool {
        matches!(self, Self::StatefulMethod(_))
    }
}

/// A packed sRGB or sRGBA color accepted by GPUI's color constructors.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PackedColor {
    /// A 24-bit red-green-blue color.
    Rgb(u32),
    /// A 32-bit red-green-blue-alpha color.
    Rgba(u32),
    /// A packed RGB color with a non-byte-quantized alpha channel.
    RgbAlpha {
        /// Packed 24-bit red-green-blue channels.
        rgb: u32,
        /// Alpha in GPUI's inclusive zero-to-one range.
        alpha: f32,
    },
}

impl PackedColor {
    /// Multiplies this color's alpha by a Tailwind opacity modifier.
    fn with_alpha(self, opacity: f32) -> Self {
        match self {
            Self::Rgb(rgb) => Self::RgbAlpha {
                rgb,
                alpha: opacity,
            },
            Self::Rgba(rgba) => Self::RgbAlpha {
                rgb: rgba >> 8,
                alpha: f32::from(rgba.to_be_bytes()[3]) / 255.0 * opacity,
            },
            Self::RgbAlpha { rgb, alpha } => Self::RgbAlpha {
                rgb,
                alpha: alpha * opacity,
            },
        }
    }

    /// Produces the appropriate GPUI color-constructor expression.
    fn tokens(self, crate_path: &TokenStream) -> TokenStream {
        match self {
            Self::Rgb(value) => quote!(#crate_path::gpui::rgb(#value)),
            Self::Rgba(value) => quote!(#crate_path::gpui::rgba(#value)),
            Self::RgbAlpha { rgb, alpha } => quote!(#crate_path::gpui::Rgba {
                a: #alpha,
                ..#crate_path::gpui::rgb(#rgb)
            }),
        }
    }
}

/// A length that GPUI can represent without a runtime parser.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LengthValue {
    /// Device-independent pixels.
    Pixels(f32),
    /// Root-em units.
    Rems(f32),
    /// A fraction of the containing dimension.
    Relative(f32),
    /// Layout-computed automatic length.
    Auto,
}

impl LengthValue {
    /// Negates a length when its utility family allows negative values.
    fn negate(self, span: Span, class: &str) -> Result<Self> {
        match self {
            Self::Pixels(value) => Ok(Self::Pixels(-value)),
            Self::Rems(value) => Ok(Self::Rems(-value)),
            Self::Relative(value) => Ok(Self::Relative(-value)),
            Self::Auto => Err(Error::new(
                span,
                format!("`{class}` cannot negate an automatic length"),
            )),
        }
    }

    /// Produces the GPUI constructor for this statically parsed length.
    fn tokens(self, crate_path: &TokenStream) -> TokenStream {
        match self {
            Self::Pixels(value) => quote!(#crate_path::gpui::px(#value)),
            Self::Rems(value) => quote!(#crate_path::gpui::rems(#value)),
            Self::Relative(value) => quote!(#crate_path::gpui::relative(#value)),
            Self::Auto => quote!(#crate_path::gpui::auto()),
        }
    }
}

/// One destination written by a dynamic length utility.
#[derive(Clone, Copy, Debug)]
struct LengthTarget {
    /// Typed conflict slot for the GPUI field.
    property: Property,
    /// GPUI custom-value setter for the field.
    method: &'static str,
}

/// Metadata for one Tailwind spacing, sizing, gap, or inset prefix.
#[derive(Clone, Copy, Debug)]
struct LengthPrefix {
    /// Source prefix including its final hyphen.
    source: &'static str,
    /// Individual fields written by this prefix.
    targets: &'static [LengthTarget],
    /// Whether the family accepts a leading negative sign.
    negative: bool,
    /// Whether the family accepts `auto`.
    auto: bool,
    /// Whether named `full` and fraction values are available.
    relative: bool,
    /// Canonical broad-to-specific conflict rank.
    canonical_rank: u128,
}

/// Constructs a table entry for a dynamic length destination.
const fn length_target(property: Property, method: &'static str) -> LengthTarget {
    LengthTarget { property, method }
}

/// Dynamic length families, ordered longest-first to avoid prefix ambiguity.
const LENGTH_PREFIXES: &[LengthPrefix] = &[
    LengthPrefix {
        source: "min-w-",
        targets: &[length_target(Property::MinWidth, "min_w")],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "min-h-",
        targets: &[length_target(Property::MinHeight, "min_h")],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "max-w-",
        targets: &[length_target(Property::MaxWidth, "max_w")],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "max-h-",
        targets: &[length_target(Property::MaxHeight, "max_h")],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "basis-",
        targets: &[length_target(Property::FlexBasis, "flex_basis")],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 100,
    },
    LengthPrefix {
        source: "inset-x-",
        targets: &[
            length_target(Property::InsetLeft, "left"),
            length_target(Property::InsetRight, "right"),
        ],
        negative: true,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "inset-y-",
        targets: &[
            length_target(Property::InsetTop, "top"),
            length_target(Property::InsetBottom, "bottom"),
        ],
        negative: true,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "inset-",
        targets: &[
            length_target(Property::InsetTop, "top"),
            length_target(Property::InsetRight, "right"),
            length_target(Property::InsetBottom, "bottom"),
            length_target(Property::InsetLeft, "left"),
        ],
        negative: true,
        auto: true,
        relative: true,
        canonical_rank: 0,
    },
    LengthPrefix {
        source: "gap-x-",
        targets: &[length_target(Property::GapX, "gap_x")],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "gap-y-",
        targets: &[length_target(Property::GapY, "gap_y")],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "size-",
        targets: &[
            length_target(Property::Width, "w"),
            length_target(Property::Height, "h"),
        ],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 0,
    },
    LengthPrefix {
        source: "top-",
        targets: &[length_target(Property::InsetTop, "top")],
        negative: true,
        auto: true,
        relative: true,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "right-",
        targets: &[length_target(Property::InsetRight, "right")],
        negative: true,
        auto: true,
        relative: true,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "bottom-",
        targets: &[length_target(Property::InsetBottom, "bottom")],
        negative: true,
        auto: true,
        relative: true,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "left-",
        targets: &[length_target(Property::InsetLeft, "left")],
        negative: true,
        auto: true,
        relative: true,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "gap-",
        targets: &[
            length_target(Property::GapX, "gap_x"),
            length_target(Property::GapY, "gap_y"),
        ],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 0,
    },
    LengthPrefix {
        source: "px-",
        targets: &[
            length_target(Property::PaddingLeft, "pl"),
            length_target(Property::PaddingRight, "pr"),
        ],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "py-",
        targets: &[
            length_target(Property::PaddingTop, "pt"),
            length_target(Property::PaddingBottom, "pb"),
        ],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "pt-",
        targets: &[length_target(Property::PaddingTop, "pt")],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "pr-",
        targets: &[length_target(Property::PaddingRight, "pr")],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "pb-",
        targets: &[length_target(Property::PaddingBottom, "pb")],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "pl-",
        targets: &[length_target(Property::PaddingLeft, "pl")],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "p-",
        targets: &[
            length_target(Property::PaddingTop, "pt"),
            length_target(Property::PaddingRight, "pr"),
            length_target(Property::PaddingBottom, "pb"),
            length_target(Property::PaddingLeft, "pl"),
        ],
        negative: false,
        auto: false,
        relative: false,
        canonical_rank: 0,
    },
    LengthPrefix {
        source: "mx-",
        targets: &[
            length_target(Property::MarginLeft, "ml"),
            length_target(Property::MarginRight, "mr"),
        ],
        negative: true,
        auto: true,
        relative: false,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "my-",
        targets: &[
            length_target(Property::MarginTop, "mt"),
            length_target(Property::MarginBottom, "mb"),
        ],
        negative: true,
        auto: true,
        relative: false,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "mt-",
        targets: &[length_target(Property::MarginTop, "mt")],
        negative: true,
        auto: true,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "mr-",
        targets: &[length_target(Property::MarginRight, "mr")],
        negative: true,
        auto: true,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "mb-",
        targets: &[length_target(Property::MarginBottom, "mb")],
        negative: true,
        auto: true,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "ml-",
        targets: &[length_target(Property::MarginLeft, "ml")],
        negative: true,
        auto: true,
        relative: false,
        canonical_rank: 2,
    },
    LengthPrefix {
        source: "m-",
        targets: &[
            length_target(Property::MarginTop, "mt"),
            length_target(Property::MarginRight, "mr"),
            length_target(Property::MarginBottom, "mb"),
            length_target(Property::MarginLeft, "ml"),
        ],
        negative: true,
        auto: true,
        relative: false,
        canonical_rank: 0,
    },
    LengthPrefix {
        source: "w-",
        targets: &[length_target(Property::Width, "w")],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
    LengthPrefix {
        source: "h-",
        targets: &[length_target(Property::Height, "h")],
        negative: false,
        auto: true,
        relative: true,
        canonical_rank: 1,
    },
];

/// Parses spacing, sizing, gap, and inset families using one numeric pipeline.
fn parse_length_utility(class: &str, span: Span) -> Result<Option<Utility>> {
    let (negative, positive_class) = match class.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, class),
    };
    let Some((prefix, raw_value)) = LENGTH_PREFIXES.iter().find_map(|prefix| {
        positive_class
            .strip_prefix(prefix.source)
            .map(|value| (prefix, value))
    }) else {
        return Ok(None);
    };

    if negative && !prefix.negative {
        return Err(Error::new(
            span,
            format!("negative values are not valid for `{}`", prefix.source),
        ));
    }
    let mut value = parse_length(raw_value, prefix, span, class)?;
    if negative {
        value = value.negate(span, class)?;
    }
    let canonical_rank = if prefix.source == "basis-" {
        match raw_value {
            // Tailwind emits functional basis values before its two named
            // terminal candidates. All basis declarations follow `flex`.
            "auto" => 101,
            "full" => 102,
            _ => prefix.canonical_rank,
        }
    } else {
        prefix.canonical_rank
    };

    let declarations = prefix
        .targets
        .iter()
        .map(|target| UtilityDeclaration {
            property: target.property,
            value: PropertyValue::Length {
                method: format_ident!("{}", target.method, span = span),
                value,
            },
            canonical_rank,
        })
        .collect();
    Ok(Some(Utility { declarations }))
}

/// Parses a named, numeric, fraction, or bracketed GPUI-compatible length.
fn parse_length(
    raw_value: &str,
    prefix: &LengthPrefix,
    span: Span,
    class: &str,
) -> Result<LengthValue> {
    if let Some(arbitrary) = raw_value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let value = parse_arbitrary_length(arbitrary).ok_or_else(|| {
            Error::new(
                span,
                format!(
                    "unsupported arbitrary length in `{class}`; supported units: px, rem, %, auto"
                ),
            )
        })?;
        if value == LengthValue::Auto && !prefix.auto {
            return Err(Error::new(
                span,
                format!("`auto` is not valid for `{}`", prefix.source),
            ));
        }
        return Ok(value);
    }
    if raw_value == "auto" {
        return if prefix.auto {
            Ok(LengthValue::Auto)
        } else {
            Err(Error::new(
                span,
                format!("`auto` is not valid for `{}`", prefix.source),
            ))
        };
    }
    if raw_value == "px" {
        return Ok(LengthValue::Pixels(1.0));
    }
    if raw_value == "full" {
        return named_relative(prefix, span, class, 1.0);
    }
    if let Some((numerator, denominator)) = parse_fraction(raw_value) {
        return named_relative(prefix, span, class, numerator / denominator);
    }
    let spacing = raw_value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0 && !raw_value.starts_with('+'));
    spacing.map_or_else(
        || {
            Err(Error::new(
                span,
                format!("invalid length value in Tailwind utility `{class}`"),
            ))
        },
        |value| Ok(LengthValue::Rems(value * 0.25)),
    )
}

/// Validates and returns a named relative length for a utility family.
fn named_relative(
    prefix: &LengthPrefix,
    span: Span,
    class: &str,
    fraction: f32,
) -> Result<LengthValue> {
    if prefix.relative {
        Ok(LengthValue::Relative(fraction))
    } else {
        Err(Error::new(
            span,
            format!(
                "named relative values are not valid for `{class}`; use an arbitrary `%` value"
            ),
        ))
    }
}

/// Parses a positive `numerator/denominator` fraction.
fn parse_fraction(value: &str) -> Option<(f32, f32)> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<f32>().ok()?;
    let denominator = denominator.parse::<f32>().ok()?;
    (numerator.is_finite() && denominator.is_finite() && numerator >= 0.0 && denominator > 0.0)
        .then_some((numerator, denominator))
}

/// Parses the intentionally safe arbitrary-length subset.
fn parse_arbitrary_length(value: &str) -> Option<LengthValue> {
    if value == "auto" {
        return Some(LengthValue::Auto);
    }
    for (suffix, constructor) in [
        ("rem", LengthValue::Rems as fn(f32) -> LengthValue),
        ("px", LengthValue::Pixels as fn(f32) -> LengthValue),
        ("%", |percent| LengthValue::Relative(percent / 100.0)),
    ] {
        let Some(number) = value.strip_suffix(suffix) else {
            continue;
        };
        let number = number.parse::<f32>().ok()?;
        if number.is_finite() && number >= 0.0 && !number.is_sign_negative() {
            return Some(constructor(number));
        }
        return None;
    }
    None
}

/// Metadata for a directional border-width prefix.
#[derive(Clone, Copy, Debug)]
struct BorderPrefix {
    /// Tailwind source name without a trailing value separator.
    source: &'static str,
    /// Individual border sides written by the prefix.
    targets: &'static [LengthTarget],
    /// Canonical broad-to-specific conflict rank.
    canonical_rank: u128,
}

/// Border-width families ordered longest-first.
const BORDER_PREFIXES: &[BorderPrefix] = &[
    BorderPrefix {
        source: "border-x",
        targets: &[
            length_target(Property::BorderLeft, "border_l"),
            length_target(Property::BorderRight, "border_r"),
        ],
        canonical_rank: 1,
    },
    BorderPrefix {
        source: "border-y",
        targets: &[
            length_target(Property::BorderTop, "border_t"),
            length_target(Property::BorderBottom, "border_b"),
        ],
        canonical_rank: 1,
    },
    BorderPrefix {
        source: "border-t",
        targets: &[length_target(Property::BorderTop, "border_t")],
        canonical_rank: 2,
    },
    BorderPrefix {
        source: "border-r",
        targets: &[length_target(Property::BorderRight, "border_r")],
        canonical_rank: 2,
    },
    BorderPrefix {
        source: "border-b",
        targets: &[length_target(Property::BorderBottom, "border_b")],
        canonical_rank: 2,
    },
    BorderPrefix {
        source: "border-l",
        targets: &[length_target(Property::BorderLeft, "border_l")],
        canonical_rank: 2,
    },
    BorderPrefix {
        source: "border",
        targets: &[
            length_target(Property::BorderTop, "border_t"),
            length_target(Property::BorderRight, "border_r"),
            length_target(Property::BorderBottom, "border_b"),
            length_target(Property::BorderLeft, "border_l"),
        ],
        canonical_rank: 0,
    },
];

/// Parses border widths while leaving color candidates for the color parser.
fn parse_border_width(class: &str, span: Span) -> Result<Option<Utility>> {
    for prefix in BORDER_PREFIXES {
        let raw_value = if class == prefix.source {
            "1"
        } else if let Some(value) = class.strip_prefix(prefix.source) {
            let Some(value) = value.strip_prefix('-') else {
                continue;
            };
            value
        } else {
            continue;
        };

        let value = if let Some(arbitrary) = raw_value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            match parse_arbitrary_length(arbitrary) {
                Some(value @ (LengthValue::Pixels(_) | LengthValue::Rems(_))) => value,
                Some(_) => {
                    return Err(Error::new(
                        span,
                        format!("border width `{class}` must use px or rem"),
                    ));
                }
                None => return Ok(None),
            }
        } else if let Ok(value) = raw_value.parse::<f32>() {
            if !value.is_finite() || value < 0.0 {
                return Err(Error::new(span, format!("invalid border width `{class}`")));
            }
            LengthValue::Pixels(value)
        } else {
            return Ok(None);
        };

        let declarations = prefix
            .targets
            .iter()
            .map(|target| UtilityDeclaration {
                property: target.property,
                value: PropertyValue::Length {
                    method: format_ident!("{}", target.method, span = span),
                    value,
                },
                canonical_rank: prefix.canonical_rank,
            })
            .collect();
        return Ok(Some(Utility { declarations }));
    }
    Ok(None)
}

/// Metadata for one physical border-radius utility prefix.
#[derive(Clone, Copy, Debug)]
struct RadiusPrefix {
    /// Tailwind spelling before the optional radius suffix.
    source: &'static str,
    /// Individual GPUI corner setters written by this prefix.
    targets: &'static [LengthTarget],
    /// Tailwind 4.3.2 stylesheet group order.
    canonical_rank: u128,
}

/// Physical radius families in longest-first parser order.
///
/// The ranks retain Tailwind's generated order: broad, top, left, top-left,
/// right, top-right, bottom, bottom-right, then bottom-left. Each declaration
/// still writes one corner so a later family cannot accidentally reset a
/// sibling corner.
const RADIUS_PREFIXES: &[RadiusPrefix] = &[
    RadiusPrefix {
        source: "rounded-tl",
        targets: &[length_target(Property::RadiusTopLeft, "rounded_tl")],
        canonical_rank: 48,
    },
    RadiusPrefix {
        source: "rounded-tr",
        targets: &[length_target(Property::RadiusTopRight, "rounded_tr")],
        canonical_rank: 80,
    },
    RadiusPrefix {
        source: "rounded-br",
        targets: &[length_target(Property::RadiusBottomRight, "rounded_br")],
        canonical_rank: 112,
    },
    RadiusPrefix {
        source: "rounded-bl",
        targets: &[length_target(Property::RadiusBottomLeft, "rounded_bl")],
        canonical_rank: 128,
    },
    RadiusPrefix {
        source: "rounded-t",
        targets: &[
            length_target(Property::RadiusTopLeft, "rounded_tl"),
            length_target(Property::RadiusTopRight, "rounded_tr"),
        ],
        canonical_rank: 16,
    },
    RadiusPrefix {
        source: "rounded-r",
        targets: &[
            length_target(Property::RadiusTopRight, "rounded_tr"),
            length_target(Property::RadiusBottomRight, "rounded_br"),
        ],
        canonical_rank: 64,
    },
    RadiusPrefix {
        source: "rounded-b",
        targets: &[
            length_target(Property::RadiusBottomRight, "rounded_br"),
            length_target(Property::RadiusBottomLeft, "rounded_bl"),
        ],
        canonical_rank: 96,
    },
    RadiusPrefix {
        source: "rounded-l",
        targets: &[
            length_target(Property::RadiusTopLeft, "rounded_tl"),
            length_target(Property::RadiusBottomLeft, "rounded_bl"),
        ],
        canonical_rank: 32,
    },
    RadiusPrefix {
        source: "rounded",
        targets: &[
            length_target(Property::RadiusTopLeft, "rounded_tl"),
            length_target(Property::RadiusTopRight, "rounded_tr"),
            length_target(Property::RadiusBottomRight, "rounded_br"),
            length_target(Property::RadiusBottomLeft, "rounded_bl"),
        ],
        canonical_rank: 0,
    },
];

/// Parses broad, side, and individual physical corner-radius utilities.
fn parse_radius_utility(class: &str, span: Span) -> Option<Utility> {
    for prefix in RADIUS_PREFIXES {
        let suffix = if class == prefix.source {
            ""
        } else if let Some(suffix) = class
            .strip_prefix(prefix.source)
            .and_then(|suffix| suffix.strip_prefix('-'))
        {
            suffix
        } else {
            continue;
        };
        let (value, suffix_rank) = radius_value(suffix)?;
        let declarations = prefix
            .targets
            .iter()
            .map(|target| UtilityDeclaration {
                property: target.property,
                value: PropertyValue::Length {
                    method: format_ident!("{}", target.method, span = span),
                    value,
                },
                canonical_rank: prefix.canonical_rank + suffix_rank,
            })
            .collect();
        return Some(Utility { declarations });
    }
    None
}

/// Maps Tailwind 4.3.2 radius suffixes to values and generated order.
fn radius_value(suffix: &str) -> Option<(LengthValue, u128)> {
    Some(match suffix {
        "" => (LengthValue::Rems(0.25), 0),
        "2xl" => (LengthValue::Rems(1.0), 1),
        "3xl" => (LengthValue::Rems(1.5), 2),
        "4xl" => (LengthValue::Rems(2.0), 3),
        "full" => (LengthValue::Pixels(9_999.0), 4),
        "lg" => (LengthValue::Rems(0.5), 5),
        "md" => (LengthValue::Rems(0.375), 6),
        "none" => (LengthValue::Pixels(0.0), 7),
        "sm" => (LengthValue::Rems(0.25), 8),
        "xl" => (LengthValue::Rems(0.75), 9),
        "xs" => (LengthValue::Rems(0.125), 10),
        _ => return None,
    })
}

/// Parses uniform positive grid column and row counts representable by GPUI.
fn parse_grid_tracks(class: &str, span: Span) -> Result<Option<Utility>> {
    for (prefix, property, method) in [
        ("grid-cols-", Property::GridColumns, "grid_cols"),
        ("grid-rows-", Property::GridRows, "grid_rows"),
    ] {
        let Some(raw_count) = class.strip_prefix(prefix) else {
            continue;
        };
        if raw_count.is_empty() || !raw_count.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(None);
        }
        let count = raw_count.parse::<u32>().map_err(|_| {
            Error::new(
                span,
                format!("`{class}` exceeds GPUI's supported grid track count"),
            )
        })?;
        let count = u16::try_from(count)
            .ok()
            .filter(|count| *count > 0)
            .ok_or_else(|| {
                Error::new(
                    span,
                    format!("`{class}` requires a grid track count from 1 through 65535"),
                )
            })?;
        return Ok(Some(Utility::single_ranked(
            property,
            PropertyValue::GridTracks {
                method: format_ident!("{method}", span = span),
                count,
            },
            u128::from(count),
        )));
    }
    Ok(None)
}

/// Tailwind rank after every supported numeric grid shorthand.
const GRID_FULL_RANK: u128 = u16::MAX as u128 + 1;

/// Tailwind rank base for negative grid-line longhands.
const GRID_NEGATIVE_LINE_BASE: u128 = 100_000;

/// Tailwind rank base for positive grid-line longhands.
const GRID_POSITIVE_LINE_BASE: u128 = 200_000;

/// Tailwind rank for the terminal `auto` grid-line longhand.
const GRID_AUTO_LINE_RANK: u128 = 300_000;

/// Parses grid column and row span, full, start, end, and auto placement.
fn parse_grid_placement(class: &str, span: Span) -> Result<Option<Utility>> {
    for (name, axis, start_property, end_property) in [
        (
            "col",
            GridAxis::Column,
            Property::GridColumnStart,
            Property::GridColumnEnd,
        ),
        (
            "row",
            GridAxis::Row,
            Property::GridRowStart,
            Property::GridRowEnd,
        ),
    ] {
        if class == format!("{name}-auto") {
            return Ok(Some(grid_placement_pair(
                axis,
                start_property,
                end_property,
                GridPlacementValue::Auto,
                GridPlacementValue::Auto,
                0,
            )));
        }

        let span_prefix = format!("{name}-span-");
        if let Some(raw_span) = class.strip_prefix(&span_prefix) {
            if raw_span == "full" {
                return Ok(Some(grid_placement_pair(
                    axis,
                    start_property,
                    end_property,
                    GridPlacementValue::Line(1),
                    GridPlacementValue::Line(-1),
                    GRID_FULL_RANK,
                )));
            }
            let track_span = parse_positive_u16(raw_span).ok_or_else(|| {
                invalid_grid_placement(
                    span,
                    class,
                    "a positive span from 1 through 65535 or `full`",
                )
            })?;
            return Ok(Some(grid_placement_pair(
                axis,
                start_property,
                end_property,
                GridPlacementValue::Span(track_span),
                GridPlacementValue::Span(track_span),
                u128::from(track_span),
            )));
        }

        for (edge_name, edge, property) in [
            ("start", GridEdge::Start, start_property),
            ("end", GridEdge::End, end_property),
        ] {
            let positive_prefix = format!("{name}-{edge_name}-");
            if let Some(raw_line) = class.strip_prefix(&positive_prefix) {
                if raw_line == "auto" {
                    return Ok(Some(grid_placement_single(
                        axis,
                        edge,
                        property,
                        GridPlacementValue::Auto,
                        GRID_AUTO_LINE_RANK,
                    )));
                }
                let line = parse_positive_grid_line(raw_line).ok_or_else(|| {
                    invalid_grid_placement(
                        span,
                        class,
                        "`auto` or a nonzero line from 1 through 32767",
                    )
                })?;
                return Ok(Some(grid_placement_single(
                    axis,
                    edge,
                    property,
                    GridPlacementValue::Line(line),
                    GRID_POSITIVE_LINE_BASE + u128::from(line.unsigned_abs()),
                )));
            }

            let negative_prefix = format!("-{name}-{edge_name}-");
            if let Some(raw_line) = class.strip_prefix(&negative_prefix) {
                let line = parse_negative_grid_line(raw_line).ok_or_else(|| {
                    invalid_grid_placement(span, class, "a negative line from -1 through -32768")
                })?;
                return Ok(Some(grid_placement_single(
                    axis,
                    edge,
                    property,
                    GridPlacementValue::Line(line),
                    GRID_NEGATIVE_LINE_BASE + u128::from(line.unsigned_abs()),
                )));
            }
        }
    }
    Ok(None)
}

/// Parses a nonzero positive `u16` without accepting signs or decimals.
fn parse_positive_u16(value: &str) -> Option<u16> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<u16>().ok())
        .flatten()
        .filter(|value| *value > 0)
}

/// Parses a positive GPUI grid line.
fn parse_positive_grid_line(value: &str) -> Option<i16> {
    let line = parse_positive_u16(value)?;
    i16::try_from(line).ok()
}

/// Parses a magnitude into a negative GPUI grid line, including `i16::MIN`.
fn parse_negative_grid_line(value: &str) -> Option<i16> {
    let magnitude = parse_positive_u16(value)?;
    match magnitude {
        1..=32_767 => Some(-i16::try_from(magnitude).ok()?),
        32_768 => Some(i16::MIN),
        _ => None,
    }
}

/// Builds one independently cascading grid-location endpoint.
fn grid_placement_single(
    axis: GridAxis,
    edge: GridEdge,
    property: Property,
    value: GridPlacementValue,
    canonical_rank: u128,
) -> Utility {
    Utility::single_ranked(
        property,
        PropertyValue::GridPlacement { axis, edge, value },
        canonical_rank,
    )
}

/// Expands a grid placement shorthand into independent start and end slots.
fn grid_placement_pair(
    axis: GridAxis,
    start_property: Property,
    end_property: Property,
    start: GridPlacementValue,
    end: GridPlacementValue,
    canonical_rank: u128,
) -> Utility {
    Utility {
        declarations: vec![
            UtilityDeclaration {
                property: start_property,
                value: PropertyValue::GridPlacement {
                    axis,
                    edge: GridEdge::Start,
                    value: start,
                },
                canonical_rank,
            },
            UtilityDeclaration {
                property: end_property,
                value: PropertyValue::GridPlacement {
                    axis,
                    edge: GridEdge::End,
                    value: end,
                },
                canonical_rank,
            },
        ],
    }
}

/// Produces a specific compile-time diagnostic for invalid grid placement.
fn invalid_grid_placement(span: Span, class: &str, expected: &str) -> Error {
    Error::new(
        span,
        format!("invalid grid placement `{class}`; expected {expected}"),
    )
}

/// Rank category for functional aspect-ratio fractions.
const ASPECT_FRACTION_RANK: u128 = 1 << 96;

/// Rank category for bracketed aspect-ratio fractions.
const ASPECT_ARBITRARY_FRACTION_RANK: u128 = 2 << 96;

/// Rank for Tailwind's named square aspect ratio.
const ASPECT_SQUARE_RANK: u128 = 3 << 96;

/// Rank for Tailwind's named video aspect ratio.
const ASPECT_VIDEO_RANK: u128 = 4 << 96;

/// Parses named and safe positive finite fraction aspect ratios.
fn parse_aspect_ratio(class: &str, span: Span) -> Result<Option<Utility>> {
    let Some(value) = class.strip_prefix("aspect-") else {
        return Ok(None);
    };
    let (ratio, canonical_rank) = match value {
        "square" => (1.0, ASPECT_SQUARE_RANK),
        "video" => (16.0 / 9.0, ASPECT_VIDEO_RANK),
        "auto" => {
            return Err(Error::new(
                span,
                "`aspect-auto` cannot reliably clear an inherited GPUI aspect ratio",
            ));
        }
        _ => {
            let (fraction, rank_base) = if value.starts_with('[') || value.ends_with(']') {
                (
                    value
                        .strip_prefix('[')
                        .and_then(|value| value.strip_suffix(']'))
                        .ok_or_else(|| invalid_aspect_ratio(span, class))?,
                    ASPECT_ARBITRARY_FRACTION_RANK,
                )
            } else {
                (value, ASPECT_FRACTION_RANK)
            };
            let (numerator, denominator) = fraction
                .split_once('/')
                .filter(|(_, denominator)| !denominator.contains('/'))
                .ok_or_else(|| invalid_aspect_ratio(span, class))?;
            let numerator =
                parse_positive_f32(numerator).ok_or_else(|| invalid_aspect_ratio(span, class))?;
            let denominator =
                parse_positive_f32(denominator).ok_or_else(|| invalid_aspect_ratio(span, class))?;
            let ratio = numerator / denominator;
            if !ratio.is_finite() || ratio <= 0.0 {
                return Err(invalid_aspect_ratio(span, class));
            }
            let rank = rank_base
                | (u128::from(numerator.to_bits()) << 32)
                | u128::from(denominator.to_bits());
            (ratio, rank)
        }
    };
    Ok(Some(Utility::single_ranked(
        Property::AspectRatio,
        PropertyValue::StyleFloat {
            field: format_ident!("aspect_ratio", span = span),
            value: ratio,
        },
        canonical_rank,
    )))
}

/// Parses a strictly positive finite floating-point number without a sign.
fn parse_positive_f32(value: &str) -> Option<f32> {
    let value = (!(value.starts_with('+') || value.starts_with('-')))
        .then(|| value.parse::<f32>().ok())
        .flatten()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

/// Produces the supported aspect-ratio syntax diagnostic.
fn invalid_aspect_ratio(span: Span, class: &str) -> Error {
    Error::new(
        span,
        format!(
            "invalid aspect ratio `{class}`; use aspect-square, aspect-video, aspect-N/D, or aspect-[N/D] with positive finite numbers"
        ),
    )
}

/// Rank category after numeric line-height utilities.
const NAMED_LINE_HEIGHT_RANK: u128 = 1 << 64;

/// Parses exact numeric spacing and named Tailwind line-height utilities.
fn parse_line_height(class: &str, span: Span) -> Result<Option<Utility>> {
    let Some(value) = class.strip_prefix("leading-") else {
        return Ok(None);
    };
    let (line_height, canonical_rank) = match value {
        "loose" => (LengthValue::Relative(2.0), NAMED_LINE_HEIGHT_RANK),
        "none" => (LengthValue::Relative(1.0), NAMED_LINE_HEIGHT_RANK + 1),
        "normal" => (LengthValue::Relative(1.5), NAMED_LINE_HEIGHT_RANK + 2),
        "relaxed" => (LengthValue::Relative(1.625), NAMED_LINE_HEIGHT_RANK + 3),
        "snug" => (LengthValue::Relative(1.375), NAMED_LINE_HEIGHT_RANK + 4),
        "tight" => (LengthValue::Relative(1.25), NAMED_LINE_HEIGHT_RANK + 5),
        _ => {
            let spacing = value.parse::<f32>().map_err(|_| {
                Error::new(
                    span,
                    format!(
                        "invalid line height `{class}`; supported: nonnegative numeric spacing and none, tight, snug, normal, relaxed, loose"
                    ),
                )
            })?;
            let rems = spacing * 0.25;
            if !spacing.is_finite()
                || spacing.is_sign_negative()
                || value.starts_with('+')
                || !rems.is_finite()
            {
                return Err(Error::new(
                    span,
                    format!(
                        "invalid line height `{class}`; numeric spacing must be finite and nonnegative"
                    ),
                ));
            }
            (LengthValue::Rems(rems), u128::from(spacing.to_bits()))
        }
    };
    Ok(Some(Utility::single_ranked(
        Property::LineHeight,
        PropertyValue::Length {
            method: format_ident!("line_height", span = span),
            value: line_height,
        },
        canonical_rank,
    )))
}

/// Parses background, text, and border colors.
fn parse_color_utility(class: &str, span: Span) -> Result<Option<Utility>> {
    for (prefix, property, method) in [
        ("bg-", Property::BackgroundColor, "bg"),
        ("text-", Property::TextColor, "text_color"),
        ("border-", Property::BorderColor, "border_color"),
    ] {
        let Some(name) = class.strip_prefix(prefix) else {
            continue;
        };
        let (name, alpha) = name
            .split_once('/')
            .map_or((name, None), |(name, alpha)| (name, Some(alpha)));
        let color = if name == "transparent" {
            Some(PackedColor::Rgba(0x0000_0000))
        } else if let Some(arbitrary) = name
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            parse_hex_color(arbitrary)
                .ok_or_else(|| {
                    Error::new(
                        span,
                        format!(
                            "invalid arbitrary color `{class}`; use #rgb, #rrggbb, or #rrggbbaa"
                        ),
                    )
                })?
                .into()
        } else {
            color_value(name)
        };
        if let Some(mut color) = color {
            if let Some(alpha) = alpha {
                let opacity = parse_color_alpha(alpha).ok_or_else(|| {
                    Error::new(
                        span,
                        format!(
                            "invalid color alpha in `{class}`; use /0 through /100, /[0 through 1], or /[0% through 100%]"
                        ),
                    )
                })?;
                color = color.with_alpha(opacity);
            }
            return Ok(Some(Utility::single(
                property,
                PropertyValue::Color {
                    method: format_ident!("{method}", span = span),
                    color,
                },
            )));
        }
    }
    Ok(None)
}

/// Parses Tailwind's safe numeric and arbitrary color-alpha subset.
fn parse_color_alpha(value: &str) -> Option<f32> {
    let (number, divisor) = if let Some(arbitrary) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        arbitrary
            .strip_suffix('%')
            .map_or((arbitrary, 1.0), |percent| (percent, 100.0))
    } else {
        (value, 100.0)
    };
    let alpha = number.parse::<f32>().ok()? / divisor;
    (alpha.is_finite() && (0.0..=1.0).contains(&alpha)).then_some(alpha)
}

/// Parses `#rgb`, `#rrggbb`, or `#rrggbbaa` into a GPUI packed color.
fn parse_hex_color(value: &str) -> Option<PackedColor> {
    let hexadecimal = value.strip_prefix('#')?;
    match hexadecimal.len() {
        3 => {
            let mut expanded = String::with_capacity(6);
            for digit in hexadecimal.chars() {
                expanded.push(digit);
                expanded.push(digit);
            }
            u32::from_str_radix(&expanded, 16)
                .ok()
                .map(PackedColor::Rgb)
        }
        6 => u32::from_str_radix(hexadecimal, 16)
            .ok()
            .map(PackedColor::Rgb),
        8 => u32::from_str_radix(hexadecimal, 16)
            .ok()
            .map(PackedColor::Rgba),
        _ => None,
    }
}

/// Parses Tailwind's percentage scale and safe arbitrary opacity subset.
fn parse_opacity(class: &str, span: Span) -> Result<Option<f32>> {
    let Some(value) = class.strip_prefix("opacity-") else {
        return Ok(None);
    };
    let invalid = || {
        Error::new(
            span,
            format!(
                "invalid opacity `{class}`; use opacity-0 through opacity-100, opacity-[0 through 1], or opacity-[0% through 100%]"
            ),
        )
    };

    let opacity = if value.starts_with('[') || value.ends_with(']') {
        let arbitrary = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .ok_or_else(invalid)?;
        if let Some(percent) = arbitrary.strip_suffix('%') {
            let percent = percent.parse::<f32>().map_err(|_| invalid())?;
            if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
                return Err(invalid());
            }
            percent / 100.0
        } else {
            let opacity = arbitrary.parse::<f32>().map_err(|_| invalid())?;
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                return Err(invalid());
            }
            opacity
        }
    } else {
        let percent = value.parse::<f32>().map_err(|_| invalid())?;
        if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
            return Err(invalid());
        }
        percent / 100.0
    };
    Ok(Some(opacity))
}

/// Builds one direct public-enum assignment for the typed cascade.
fn style_enum_declaration(
    property: Property,
    field: &str,
    enum_type: &str,
    variant: &str,
    span: Span,
    canonical_rank: u128,
) -> UtilityDeclaration {
    UtilityDeclaration {
        property,
        value: PropertyValue::StyleEnum {
            field: format_ident!("{field}", span = span),
            enum_type: format_ident!("{enum_type}", span = span),
            variant: format_ident!("{variant}", span = span),
        },
        canonical_rank,
    }
}

/// Expands `place-content-*` into its two independently cascading longhands.
fn place_content_utility(variant: &str, span: Span, canonical_rank: u128) -> Utility {
    Utility {
        declarations: vec![
            style_enum_declaration(
                Property::AlignContent,
                "align_content",
                "AlignContent",
                variant,
                span,
                canonical_rank,
            ),
            style_enum_declaration(
                Property::JustifyContent,
                "justify_content",
                "JustifyContent",
                variant,
                span,
                canonical_rank,
            ),
        ],
    }
}

/// Metadata for one exact single-field alignment utility.
#[derive(Clone, Copy, Debug)]
struct AlignmentUtility {
    /// Tailwind class name.
    class: &'static str,
    /// Independently cascading GPUI property.
    property: Property,
    /// Public GPUI style-refinement field.
    field: &'static str,
    /// Public GPUI enum or type alias.
    enum_type: &'static str,
    /// Exact enum variant.
    variant: &'static str,
    /// Tailwind 4.3.2 candidate order.
    canonical_rank: u128,
}

/// Constructs one static alignment table entry.
const fn alignment_utility(
    class: &'static str,
    property: Property,
    field: &'static str,
    enum_type: &'static str,
    variant: &'static str,
    canonical_rank: u128,
) -> AlignmentUtility {
    AlignmentUtility {
        class,
        property,
        field,
        enum_type,
        variant,
        canonical_rank,
    }
}

/// Exact single-field alignment utilities in Tailwind 4.3.2 candidate order.
const ALIGNMENT_UTILITIES: &[AlignmentUtility] = &[
    alignment_utility(
        "content-around",
        Property::AlignContent,
        "align_content",
        "AlignContent",
        "SpaceAround",
        7,
    ),
    alignment_utility(
        "content-between",
        Property::AlignContent,
        "align_content",
        "AlignContent",
        "SpaceBetween",
        8,
    ),
    alignment_utility(
        "content-center",
        Property::AlignContent,
        "align_content",
        "AlignContent",
        "Center",
        9,
    ),
    alignment_utility(
        "content-end",
        Property::AlignContent,
        "align_content",
        "AlignContent",
        "FlexEnd",
        10,
    ),
    alignment_utility(
        "content-evenly",
        Property::AlignContent,
        "align_content",
        "AlignContent",
        "SpaceEvenly",
        11,
    ),
    alignment_utility(
        "content-start",
        Property::AlignContent,
        "align_content",
        "AlignContent",
        "FlexStart",
        13,
    ),
    alignment_utility(
        "content-stretch",
        Property::AlignContent,
        "align_content",
        "AlignContent",
        "Stretch",
        14,
    ),
    alignment_utility(
        "items-baseline",
        Property::AlignItems,
        "align_items",
        "AlignItems",
        "Baseline",
        15,
    ),
    alignment_utility(
        "items-center",
        Property::AlignItems,
        "align_items",
        "AlignItems",
        "Center",
        16,
    ),
    alignment_utility(
        "items-end",
        Property::AlignItems,
        "align_items",
        "AlignItems",
        "FlexEnd",
        17,
    ),
    alignment_utility(
        "items-start",
        Property::AlignItems,
        "align_items",
        "AlignItems",
        "FlexStart",
        18,
    ),
    alignment_utility(
        "items-stretch",
        Property::AlignItems,
        "align_items",
        "AlignItems",
        "Stretch",
        19,
    ),
    alignment_utility(
        "justify-around",
        Property::JustifyContent,
        "justify_content",
        "JustifyContent",
        "SpaceAround",
        20,
    ),
    alignment_utility(
        "justify-between",
        Property::JustifyContent,
        "justify_content",
        "JustifyContent",
        "SpaceBetween",
        21,
    ),
    alignment_utility(
        "justify-center",
        Property::JustifyContent,
        "justify_content",
        "JustifyContent",
        "Center",
        22,
    ),
    alignment_utility(
        "justify-end",
        Property::JustifyContent,
        "justify_content",
        "JustifyContent",
        "FlexEnd",
        23,
    ),
    alignment_utility(
        "justify-evenly",
        Property::JustifyContent,
        "justify_content",
        "JustifyContent",
        "SpaceEvenly",
        24,
    ),
    alignment_utility(
        "justify-start",
        Property::JustifyContent,
        "justify_content",
        "JustifyContent",
        "FlexStart",
        25,
    ),
    alignment_utility(
        "justify-stretch",
        Property::JustifyContent,
        "justify_content",
        "JustifyContent",
        "Stretch",
        26,
    ),
    alignment_utility(
        "self-baseline",
        Property::AlignSelf,
        "align_self",
        "AlignSelf",
        "Baseline",
        27,
    ),
    alignment_utility(
        "self-center",
        Property::AlignSelf,
        "align_self",
        "AlignSelf",
        "Center",
        28,
    ),
    alignment_utility(
        "self-end",
        Property::AlignSelf,
        "align_self",
        "AlignSelf",
        "FlexEnd",
        29,
    ),
    alignment_utility(
        "self-start",
        Property::AlignSelf,
        "align_self",
        "AlignSelf",
        "FlexStart",
        30,
    ),
    alignment_utility(
        "self-stretch",
        Property::AlignSelf,
        "align_self",
        "AlignSelf",
        "Stretch",
        31,
    ),
];

/// Place-content values in Tailwind 4.3.2 candidate order.
const PLACE_CONTENT_UTILITIES: &[(&str, &str, u128)] = &[
    ("place-content-around", "SpaceAround", 0),
    ("place-content-between", "SpaceBetween", 1),
    ("place-content-center", "Center", 2),
    ("place-content-end", "End", 3),
    ("place-content-evenly", "SpaceEvenly", 4),
    ("place-content-start", "Start", 5),
    ("place-content-stretch", "Stretch", 6),
];

/// Parses Tailwind 4.3.2 alignment values exposed exactly by GPUI 0.2.2.
fn parse_alignment_utility(class: &str, span: Span) -> Result<Option<Utility>> {
    if let Some((_, variant, canonical_rank)) = PLACE_CONTENT_UTILITIES
        .iter()
        .find(|(candidate, _, _)| *candidate == class)
    {
        return Ok(Some(place_content_utility(variant, span, *canonical_rank)));
    }
    if let Some(utility) = ALIGNMENT_UTILITIES
        .iter()
        .find(|utility| utility.class == class)
    {
        return Ok(Some(Utility::ranked_style_enum(
            utility.property,
            utility.field,
            utility.enum_type,
            utility.variant,
            span,
            utility.canonical_rank,
        )));
    }

    if matches!(class, "content-normal" | "justify-normal" | "self-auto") {
        return Err(Error::new(
            span,
            format!(
                "Tailwind `{class}` clears or inherits an alignment value, but GPUI 0.2.2 `StyleRefinement` uses `None` to mean no override; this reset cannot be represented faithfully across base and state cascades"
            ),
        ));
    }
    if matches!(
        class,
        "place-content-center-safe"
            | "place-content-end-safe"
            | "place-content-baseline"
            | "content-center-safe"
            | "content-end-safe"
            | "content-baseline"
            | "items-center-safe"
            | "items-end-safe"
            | "items-baseline-last"
            | "justify-center-safe"
            | "justify-end-safe"
            | "justify-baseline"
            | "self-center-safe"
            | "self-end-safe"
            | "self-baseline-last"
    ) {
        return Err(Error::new(
            span,
            format!(
                "Tailwind `{class}` uses safe-overflow or baseline alignment semantics absent from GPUI 0.2.2's public alignment enums"
            ),
        ));
    }
    Ok(None)
}
/// Rejects known Tailwind layout values that would require host substitution.
fn reject_unrepresentable_layout_utility(class: &str, span: Span) -> Result<()> {
    if class.starts_with("order-") {
        return Err(Error::new(
            span,
            format!(
                "Tailwind `{class}` sets CSS layout order, but GPUI 0.2.2 exposes no public order field"
            ),
        ));
    }

    if matches!(
        class,
        "overflow-auto"
            | "overflow-x-auto"
            | "overflow-y-auto"
            | "overflow-clip"
            | "overflow-x-clip"
            | "overflow-y-clip"
            | "overflow-visible"
            | "overflow-x-visible"
            | "overflow-y-visible"
    ) {
        return Err(Error::new(
            span,
            format!(
                "Tailwind `{class}` cannot be represented faithfully by GPUI 0.2.2 retained overflow APIs; supported exact values are hidden and static scroll"
            ),
        ));
    }

    if matches!(
        class,
        "inline"
            | "inline-block"
            | "inline-flex"
            | "inline-grid"
            | "flow-root"
            | "contents"
            | "table"
            | "inline-table"
            | "table-caption"
            | "table-cell"
            | "table-column"
            | "table-column-group"
            | "table-footer-group"
            | "table-header-group"
            | "table-row-group"
            | "table-row"
            | "list-item"
    ) {
        return Err(Error::new(
            span,
            format!(
                "Tailwind display utility `{class}` has no exact GPUI 0.2.2 display mode; supported exact modes are block, flex, grid, and hidden"
            ),
        ));
    }

    Ok(())
}

/// Resolves exact layout and visual utilities backed by zero-argument methods.
fn parse_exact_utility(class: &str, span: Span) -> Option<Utility> {
    parse_compound_utility(class, span)
        .or_else(|| {
            exact_layout_method(class)
                .map(|(property, method)| Utility::method(property, method, span))
        })
        .or_else(|| {
            exact_text_method(class).map(|(property, method, canonical_rank)| {
                Utility::ranked_method(property, method, span, canonical_rank)
            })
        })
        .or_else(|| {
            exact_cursor_method(class)
                .map(|(property, method)| Utility::method(property, method, span))
        })
        .or_else(|| {
            exact_visual_method(class)
                .map(|(property, method)| Utility::method(property, method, span))
        })
}

/// Lowers utilities whose GPUI convenience methods mutate multiple fields.
fn parse_compound_utility(class: &str, span: Span) -> Option<Utility> {
    Some(match class {
        // Tailwind emits the four flex shorthand rules in this order. Their
        // longhand declarations are emitted later and use rank 100 or above.
        "flex-1" => flex_shorthand(1.0, 1.0, LengthValue::Relative(0.0), 0, span),
        "flex-auto" => flex_shorthand(1.0, 1.0, LengthValue::Auto, 1, span),
        "flex-initial" => flex_shorthand(0.0, 1.0, LengthValue::Auto, 2, span),
        "flex-none" => flex_shorthand(0.0, 0.0, LengthValue::Auto, 3, span),
        "shrink" => Utility::single_ranked(
            Property::FlexShrink,
            PropertyValue::StyleFloat {
                field: format_ident!("flex_shrink", span = span),
                value: 1.0,
            },
            100,
        ),
        "shrink-0" => Utility::single_ranked(
            Property::FlexShrink,
            PropertyValue::StyleFloat {
                field: format_ident!("flex_shrink", span = span),
                value: 0.0,
            },
            101,
        ),
        "grow" => Utility::single_ranked(
            Property::FlexGrow,
            PropertyValue::StyleFloat {
                field: format_ident!("flex_grow", span = span),
                value: 1.0,
            },
            100,
        ),
        "grow-0" => Utility::single_ranked(
            Property::FlexGrow,
            PropertyValue::StyleFloat {
                field: format_ident!("flex_grow", span = span),
                value: 0.0,
            },
            101,
        ),
        "truncate" => Utility::ranked_methods(
            [
                (Property::OverflowX, "overflow_x_hidden", 1),
                (Property::OverflowY, "overflow_y_hidden", 1),
                (Property::TextOverflow, "text_ellipsis", 1),
                (Property::WhiteSpace, "whitespace_nowrap", 1),
            ],
            span,
        ),
        "overflow-hidden" => Utility::ranked_methods(
            [
                (Property::OverflowX, "overflow_x_hidden", 2),
                (Property::OverflowY, "overflow_y_hidden", 2),
            ],
            span,
        ),
        "overflow-scroll" => Utility {
            declarations: vec![
                UtilityDeclaration {
                    property: Property::OverflowX,
                    value: PropertyValue::StatefulMethod(format_ident!(
                        "overflow_x_scroll",
                        span = span
                    )),
                    canonical_rank: 3,
                },
                UtilityDeclaration {
                    property: Property::OverflowY,
                    value: PropertyValue::StatefulMethod(format_ident!(
                        "overflow_y_scroll",
                        span = span
                    )),
                    canonical_rank: 3,
                },
            ],
        },
        "overflow-x-hidden" => {
            Utility::ranked_method(Property::OverflowX, "overflow_x_hidden", span, 4)
        }
        "overflow-x-scroll" => {
            Utility::ranked_stateful_method(Property::OverflowX, "overflow_x_scroll", span, 5)
        }
        "overflow-y-hidden" => {
            Utility::ranked_method(Property::OverflowY, "overflow_y_hidden", span, 6)
        }
        "overflow-y-scroll" => {
            Utility::ranked_stateful_method(Property::OverflowY, "overflow_y_scroll", span, 7)
        }
        _ => return None,
    })
}

/// Expands one CSS flex shorthand into three independently cascading fields.
fn flex_shorthand(
    grow: f32,
    shrink: f32,
    basis: LengthValue,
    canonical_rank: u128,
    span: Span,
) -> Utility {
    Utility {
        declarations: vec![
            UtilityDeclaration {
                property: Property::FlexGrow,
                value: PropertyValue::StyleFloat {
                    field: format_ident!("flex_grow", span = span),
                    value: grow,
                },
                canonical_rank,
            },
            UtilityDeclaration {
                property: Property::FlexShrink,
                value: PropertyValue::StyleFloat {
                    field: format_ident!("flex_shrink", span = span),
                    value: shrink,
                },
                canonical_rank,
            },
            UtilityDeclaration {
                property: Property::FlexBasis,
                value: PropertyValue::Length {
                    method: format_ident!("flex_basis", span = span),
                    value: basis,
                },
                canonical_rank,
            },
        ],
    }
}

/// Maps exact display, flexbox, positioning, and overflow utilities.
fn exact_layout_method(class: &str) -> Option<(Property, &'static str)> {
    Some(match class {
        "block" => (Property::Display, "block"),
        "flex" => (Property::Display, "flex"),
        "grid" => (Property::Display, "grid"),
        "hidden" => (Property::Display, "hidden"),
        "visible" => (Property::Visibility, "visible"),
        "invisible" => (Property::Visibility, "invisible"),
        "flex-col" => (Property::FlexDirection, "flex_col"),
        "flex-col-reverse" => (Property::FlexDirection, "flex_col_reverse"),
        "flex-row" => (Property::FlexDirection, "flex_row"),
        "flex-row-reverse" => (Property::FlexDirection, "flex_row_reverse"),
        "flex-wrap" => (Property::FlexWrap, "flex_wrap"),
        "flex-wrap-reverse" => (Property::FlexWrap, "flex_wrap_reverse"),
        "flex-nowrap" => (Property::FlexWrap, "flex_nowrap"),
        "relative" => (Property::Position, "relative"),
        "absolute" => (Property::Position, "absolute"),
        _ => return None,
    })
}

/// Maps exact typography utilities.
fn exact_text_method(class: &str) -> Option<(Property, &'static str, u128)> {
    Some(match class {
        // These ranks mirror Tailwind's generated utility stylesheet after
        // the compound `truncate` rule at rank 1.
        "text-ellipsis" => (Property::TextOverflow, "text_ellipsis", 2),
        "whitespace-normal" => (Property::WhiteSpace, "whitespace_normal", 2),
        "whitespace-nowrap" => (Property::WhiteSpace, "whitespace_nowrap", 3),
        "text-left" => (Property::TextAlign, "text_left", 0),
        "text-center" => (Property::TextAlign, "text_center", 0),
        "text-right" => (Property::TextAlign, "text_right", 0),
        "italic" => (Property::FontStyle, "italic", 0),
        "not-italic" => (Property::FontStyle, "not_italic", 0),
        "underline" => (Property::TextDecoration, "underline", 0),
        "line-through" => (Property::TextDecoration, "line_through", 0),
        _ => return None,
    })
}

/// Maps the GPUI-supported Tailwind cursor vocabulary.
fn exact_cursor_method(class: &str) -> Option<(Property, &'static str)> {
    Some((
        Property::Cursor,
        match class {
            "cursor-default" => "cursor_default",
            "cursor-pointer" => "cursor_pointer",
            "cursor-text" => "cursor_text",
            "cursor-move" => "cursor_move",
            "cursor-not-allowed" => "cursor_not_allowed",
            "cursor-context-menu" => "cursor_context_menu",
            "cursor-crosshair" => "cursor_crosshair",
            "cursor-vertical-text" => "cursor_vertical_text",
            "cursor-alias" => "cursor_alias",
            "cursor-copy" => "cursor_copy",
            "cursor-no-drop" => "cursor_no_drop",
            "cursor-grab" => "cursor_grab",
            "cursor-grabbing" => "cursor_grabbing",
            "cursor-ew-resize" => "cursor_ew_resize",
            "cursor-ns-resize" => "cursor_ns_resize",
            "cursor-nesw-resize" => "cursor_nesw_resize",
            "cursor-nwse-resize" => "cursor_nwse_resize",
            "cursor-col-resize" => "cursor_col_resize",
            "cursor-row-resize" => "cursor_row_resize",
            "cursor-n-resize" => "cursor_n_resize",
            "cursor-e-resize" => "cursor_e_resize",
            "cursor-s-resize" => "cursor_s_resize",
            "cursor-w-resize" => "cursor_w_resize",
            _ => return None,
        },
    ))
}

/// Maps exact border and shadow utilities.
fn exact_visual_method(class: &str) -> Option<(Property, &'static str)> {
    Some(match class {
        "border-dashed" => (Property::BorderStyle, "border_dashed"),
        "shadow-none" => (Property::Shadow, "shadow_none"),
        "shadow-2xs" => (Property::Shadow, "shadow_2xs"),
        "shadow-xs" => (Property::Shadow, "shadow_xs"),
        "shadow" | "shadow-sm" => (Property::Shadow, "shadow_sm"),
        "shadow-md" => (Property::Shadow, "shadow_md"),
        "shadow-lg" => (Property::Shadow, "shadow_lg"),
        "shadow-xl" => (Property::Shadow, "shadow_xl"),
        "shadow-2xl" => (Property::Shadow, "shadow_2xl"),
        _ => return None,
    })
}

/// Maps Tailwind font-weight names to GPUI constants.
fn font_weight(value: &str) -> Option<&'static str> {
    Some(match value {
        "thin" => "THIN",
        "extralight" => "EXTRA_LIGHT",
        "light" => "LIGHT",
        "normal" => "NORMAL",
        "medium" => "MEDIUM",
        "semibold" => "SEMIBOLD",
        "bold" => "BOLD",
        "extrabold" => "EXTRA_BOLD",
        "black" => "BLACK",
        _ => return None,
    })
}

/// Maps Tailwind's default font-size scale to rem units.
fn text_size(value: &str) -> Option<f32> {
    Some(match value {
        "xs" => 0.75,
        "sm" => 0.875,
        "base" => 1.0,
        "lg" => 1.125,
        "xl" => 1.25,
        "2xl" => 1.5,
        "3xl" => 1.875,
        "4xl" => 2.25,
        "5xl" => 3.0,
        "6xl" => 3.75,
        "7xl" => 4.5,
        "8xl" => 6.0,
        "9xl" => 8.0,
        _ => return None,
    })
}

/// Maps a Tailwind 4.3.2 default color name to GPUI's packed sRGB form.
fn color_value(name: &str) -> Option<PackedColor> {
    tailwind_palette::rgb(name).map(PackedColor::Rgb)
}

#[cfg(test)]
mod tests {
    //! Unit tests for parsing, cascade resolution, and token lowering.

    use super::*;

    /// Reads the resolved declaration assigned to `property` in a test cascade.
    fn resolved_declaration(cascade: &Cascade, property: Property) -> &Declaration {
        cascade
            .declarations
            .iter()
            .find(|declaration| declaration.property == property)
            .expect("the requested property should be assigned")
    }

    /// Reads the resolved length assigned to `property` in a test cascade.
    fn resolved_length(cascade: &Cascade, property: Property) -> LengthValue {
        let declaration = resolved_declaration(cascade, property);
        match &declaration.value {
            PropertyValue::Length { value, .. } => *value,
            _ => panic!("the requested property should hold a length"),
        }
    }

    /// Reads the resolved direct style float assigned to `property`.
    fn resolved_style_float(cascade: &Cascade, property: Property) -> f32 {
        match &resolved_declaration(cascade, property).value {
            PropertyValue::StyleFloat { value, .. } => *value,
            _ => panic!("the requested property should hold a style float"),
        }
    }

    /// Reads the resolved color assigned to `property` in a test cascade.
    fn resolved_color(cascade: &Cascade, property: Property) -> PackedColor {
        match &resolved_declaration(cascade, property).value {
            PropertyValue::Color { color, .. } => *color,
            _ => panic!("the requested property should hold a color"),
        }
    }

    /// Reads the resolved uniform grid-track count assigned to `property`.
    fn resolved_grid_tracks(cascade: &Cascade, property: Property) -> u16 {
        match &resolved_declaration(cascade, property).value {
            PropertyValue::GridTracks { count, .. } => *count,
            _ => panic!("the requested property should hold a grid-track count"),
        }
    }

    /// Reads one independently resolved grid-location endpoint.
    fn resolved_grid_placement(cascade: &Cascade, property: Property) -> GridPlacementValue {
        match &resolved_declaration(cascade, property).value {
            PropertyValue::GridPlacement { value, .. } => *value,
            _ => panic!("the requested property should hold a grid placement"),
        }
    }

    /// Reads the resolved element opacity in a test cascade.
    fn resolved_opacity(cascade: &Cascade) -> f32 {
        match &resolved_declaration(cascade, Property::Opacity).value {
            PropertyValue::Opacity(opacity) => *opacity,
            _ => panic!("the opacity property should hold a scalar"),
        }
    }

    /// Asserts an exactly represented flex factor without approximate semantics.
    fn assert_resolved_style_float(cascade: &Cascade, property: Property, expected: f32) {
        assert_eq!(
            resolved_style_float(cascade, property).to_bits(),
            expected.to_bits()
        );
    }

    /// Reads the GPUI method selected for `property`.
    fn resolved_method(cascade: &Cascade, property: Property) -> String {
        match &resolved_declaration(cascade, property).value {
            PropertyValue::Method(method) => method.to_string(),
            _ => panic!("the requested property should hold a method"),
        }
    }

    /// Reads the public GPUI enum variant assigned to `property`.
    fn resolved_style_enum_variant(cascade: &Cascade, property: Property) -> String {
        match &resolved_declaration(cascade, property).value {
            PropertyValue::StyleEnum { variant, .. } => variant.to_string(),
            _ => panic!("the requested property should hold a style enum"),
        }
    }

    /// Reads the retained-state GPUI method selected for `property`.
    fn resolved_stateful_method(cascade: &Cascade, property: Property) -> String {
        match &resolved_declaration(cascade, property).value {
            PropertyValue::StatefulMethod(method) => method.to_string(),
            _ => panic!("the requested property should hold a stateful method"),
        }
    }

    /// Parses common state variants without retaining runtime class strings.
    #[test]
    fn parses_variants_without_runtime_strings() {
        let classes = CompiledClasses::parse(&LitStr::new(
            "flex p-4 bg-slate-950 hover:bg-blue-500 active:bg-blue-700",
            Span::call_site(),
        ))
        .expect("the fixture should be supported");
        assert_eq!(classes.regular.declarations.len(), 6);
        assert_eq!(classes.hover.declarations.len(), 1);
        assert!(classes.needs_stateful_id());
        assert!(!classes.needs_focusable());
    }

    /// Lowers groups plus GPUI's native ancestor-or-self focus-tree seam.
    #[test]
    fn lowers_groups_and_gpui_ancestor_or_self_in_focus() {
        let group = CompiledClasses::parse(&LitStr::new("group", Span::call_site()))
            .expect("the native GPUI group marker should compile");
        let target = CompiledClasses::parse(&LitStr::new(
            "in-focus:bg-slate-800 group-hover:text-white group-active:opacity-50",
            Span::call_site(),
        ))
        .expect("native GPUI group and in-focus variants should compile");

        assert!(group.group);
        assert_eq!(target.in_focus.declarations.len(), 1);
        assert_eq!(target.group_hover.declarations.len(), 1);
        assert_eq!(target.group_active.declarations.len(), 1);
        assert!(target.needs_stateful_id());
        assert!(target.needs_focusable());

        let regular = group
            .apply_regular(quote!(::gpui::div()), &quote!(::gpui_vue))
            .to_string();
        assert!(regular.contains("group"));
        assert!(regular.contains(DEFAULT_GROUP_NAME));

        let variants = target
            .apply_variants(quote!(element), &quote!(::gpui_vue))
            .to_string();
        assert!(variants.contains("in_focus"));
        assert!(variants.contains("group_hover"));
        assert!(variants.contains("group_active"));
    }

    /// Adds an inert hover refinement for GPUI's active-state hitbox gap only.
    #[test]
    fn active_states_alone_create_a_native_target_hitbox() {
        for (fixture, expected_method) in [
            ("active:bg-blue-700", "active"),
            ("group-active:bg-blue-700", "group_active"),
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("active state should compile");
            let tokens = classes
                .apply_variants(quote!(element), &quote!(::gpui_vue))
                .to_string();
            assert!(tokens.contains("hover"));
            assert!(tokens.contains(expected_method));
        }

        let with_hover = CompiledClasses::parse(&LitStr::new(
            "hover:text-white group-active:bg-blue-700",
            Span::call_site(),
        ))
        .expect("ordinary hover should provide the required hitbox");
        let tokens = with_hover
            .apply_variants(quote!(element), &quote!(::gpui_vue))
            .to_string();
        assert_eq!(tokens.matches("hover").count(), 1);
    }

    /// Covers every supported state pair, both source orders, and all four
    /// normal/important combinations against the official and host orders.
    #[test]
    fn simultaneous_state_pairs_require_the_same_property_winner() {
        // (prefix, Tailwind effective precedence, GPUI refinement order).
        let states = [
            ("in-focus", 1_u8, 1_u8),
            ("group-hover", 2, 3),
            ("group-active", 3, 5),
            ("hover", 4, 4),
            ("focus", 5, 2),
            ("active", 6, 6),
        ];

        for (left_index, left) in states.iter().enumerate() {
            for right in &states[left_index + 1..] {
                for (left_important, right_important) in
                    [(false, false), (true, false), (false, true), (true, true)]
                {
                    let tailwind_left_wins = match (left_important, right_important) {
                        (true, false) => true,
                        (false, true) => false,
                        _ => left.1 > right.1,
                    };
                    let gpui_left_wins = left.2 > right.2;
                    let should_compile = tailwind_left_wins == gpui_left_wins;
                    let left_utility = if left_important { "block!" } else { "block" };
                    let right_utility = if right_important { "flex!" } else { "flex" };

                    for fixture in [
                        format!("{}:{left_utility} {}:{right_utility}", left.0, right.0),
                        format!("{}:{right_utility} {}:{left_utility}", right.0, left.0),
                    ] {
                        let result =
                            CompiledClasses::parse(&LitStr::new(&fixture, Span::call_site()));
                        assert_eq!(
                            result.is_ok(),
                            should_compile,
                            "unexpected simultaneous-state result for `{fixture}`: {result:?}",
                        );
                        if let Err(error) = result {
                            let message = error.to_string();
                            assert!(message.contains(left.0));
                            assert!(message.contains(right.0));
                            assert!(message.contains("Display"));
                            if left_important != right_important {
                                assert!(message.contains("cross-state `!important`"));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Keeps aligned state pairs, importance-restored pairs, and independent
    /// property slots available instead of rejecting every multi-state class.
    #[test]
    fn allows_state_pairs_whose_runtime_winner_is_equivalent() {
        for fixture in [
            "hover:block active:flex",
            "active:flex hover:block",
            "hover:block group-active:flex!",
            "group-active:flex! hover:block",
            "focus:block hover:opacity-50",
            "hover:opacity-50 focus:block",
        ] {
            CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .unwrap_or_else(|error| panic!("`{fixture}` should remain supported: {error}"));
        }
    }

    /// Preserves the existing regular-versus-state important boundary before
    /// checking conflicts among declarations that reach state callbacks.
    #[test]
    fn regular_important_still_controls_each_state_property() {
        for fixture in [
            "block! hover:flex",
            "hover:flex block!",
            "block! focus:flex hover:grid",
            "focus:flex hover:grid block!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("regular important should suppress ordinary state declarations");
            let tokens = classes
                .apply_variants(quote!(element), &quote!(::gpui_vue))
                .to_string();
            assert!(!tokens.contains("flex"));
            assert!(!tokens.contains("grid"));
        }

        for fixture in [
            "block hover:flex!",
            "hover:flex! block",
            "block! hover:flex!",
            "hover:flex! block!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("an important state should refine either regular importance level");
            let tokens = classes
                .apply_variants(quote!(element), &quote!(::gpui_vue))
                .to_string();
            assert!(tokens.contains("flex"));
        }
    }

    /// Erases fully blocked states before identity, focus tracking, callbacks,
    /// and the active hitbox fallback are selected.
    #[test]
    fn fully_blocked_states_require_no_identity_or_callbacks() {
        for variant in [
            "in-focus",
            "hover",
            "group-hover",
            "active",
            "group-active",
            "focus",
        ] {
            for fixture in [
                format!("block! {variant}:flex"),
                format!("{variant}:flex block!"),
            ] {
                let classes = CompiledClasses::parse(&LitStr::new(&fixture, Span::call_site()))
                    .unwrap_or_else(|error| panic!("`{fixture}` should compile: {error}"));
                assert!(!classes.needs_stateful_id());
                assert!(!classes.needs_focusable());
                assert_eq!(
                    classes
                        .apply_variants(quote!(element), &quote!(::gpui_vue))
                        .to_string(),
                    "element",
                );
            }
        }

        let grouped = CompiledClasses::parse(&LitStr::new(
            "group block! group-active:flex",
            Span::call_site(),
        ))
        .expect("a fully blocked group-active field should leave only regular group styling");
        let expanded = grouped.apply_regular(quote!(::gpui::div()), &quote!(::gpui_vue));
        let expanded = grouped
            .apply_variants(expanded, &quote!(::gpui_vue))
            .to_string();
        assert!(expanded.contains("block"));
        assert!(expanded.contains("group"));
        assert!(!expanded.contains("group_active"));
        assert!(!expanded.contains("hover"));
        assert!(!expanded.contains("flex"));
    }

    /// Keeps partially effective, important, and retained regular declarations
    /// on their existing lowering paths.
    #[test]
    fn effective_and_retained_declarations_still_lower() {
        let partial = CompiledClasses::parse(&LitStr::new("px-4! active:p-2", Span::call_site()))
            .expect("the unblocked vertical fields should keep active state effective");
        assert!(partial.needs_stateful_id());
        let active_style = partial
            .active
            .apply(
                &quote!(__gpui_vue_style),
                &quote!(::gpui_vue),
                Some(&partial.regular),
            )
            .to_string();
        assert!(active_style.contains(". pt ("));
        assert!(active_style.contains(". pb ("));
        assert!(!active_style.contains(". pl ("));
        assert!(!active_style.contains(". pr ("));
        let partial_tokens = partial
            .apply_variants(quote!(element), &quote!(::gpui_vue))
            .to_string();
        assert!(partial_tokens.contains("active"));
        assert!(partial_tokens.contains("hover"));

        let important =
            CompiledClasses::parse(&LitStr::new("block! focus:flex!", Span::call_site()))
                .expect("an important focus declaration should remain effective");
        assert!(important.needs_stateful_id());
        assert!(important.needs_focusable());
        let important_tokens = important
            .apply_variants(quote!(element), &quote!(::gpui_vue))
            .to_string();
        assert!(important_tokens.contains("focus"));
        assert!(important_tokens.contains("flex"));

        let retained = CompiledClasses::parse(&LitStr::new(
            "overflow-x-scroll block! active:flex",
            Span::call_site(),
        ))
        .expect("regular retained scroll should remain independent of blocked state styling");
        assert!(retained.needs_stateful_id());
        let retained_tokens = retained
            .apply_variants(quote!(element), &quote!(::gpui_vue))
            .to_string();
        assert!(retained_tokens.contains("overflow_x_scroll"));
        assert!(!retained_tokens.contains("active"));
        assert!(!retained_tokens.contains("hover"));
    }

    /// Rejects only the native group-active self-hitbox divergence; group-hover
    /// is computed before the element pushes its own group hitbox and remains
    /// equivalent to Tailwind's descendant-only selector on the same element.
    #[test]
    fn group_active_self_rejection_uses_effective_emitted_fields() {
        for fixture in ["group group-active:block", "group-active:block group"] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("GPUI group-active can self-match its own group hitbox");
            let message = error.to_string();
            assert!(message.contains("group-active"));
            assert!(message.contains("same element"));
            assert!(message.contains("descendants"));
        }

        for fixture in [
            "group block! group-active:flex",
            "group group-active:flex block!",
            "group p-4! group-active:px-2",
            "group group-active:px-2 p-4!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .unwrap_or_else(|error| {
                    panic!(
                        "fully blocked group-active fields in `{fixture}` should compile: {error}"
                    )
                });
            let effective_style = classes
                .group_active
                .apply(
                    &quote!(__gpui_vue_style),
                    &quote!(::gpui_vue),
                    Some(&classes.regular),
                )
                .to_string();
            assert_eq!(effective_style, "__gpui_vue_style");
        }

        for fixture in [
            "group px-4! group-active:p-2",
            "group group-active:p-2 px-4!",
            "group block! group-active:flex!",
            "group group-active:flex! block!",
        ] {
            CompiledClasses::parse(&LitStr::new(fixture, Span::call_site())).expect_err(
                "one unblocked compound field or an important group-active field can self-match",
            );
        }

        CompiledClasses::parse(&LitStr::new("group group-hover:block", Span::call_site()))
            .expect("group-hover cannot see the element's not-yet-pushed group hitbox");
    }

    /// Rejects focus-within because GPUI exposes only the inverse fluent seam.
    #[test]
    fn rejects_focus_within_without_a_native_style_seam() {
        let error =
            CompiledClasses::parse(&LitStr::new("focus-within:bg-blue-500", Span::call_site()))
                .expect_err("focus-within has no GPUI 0.2.2 fluent style seam");
        assert!(error.to_string().contains("contains_focused"));
        assert!(error.to_string().contains("in-focus"));
    }

    /// Resolves alignment compounds and longhands in official stylesheet order.
    #[test]
    fn alignment_slots_follow_stylesheet_order_in_both_source_orders() {
        let forward = CompiledClasses::parse(&LitStr::new(
            "place-content-evenly content-start justify-end items-center items-stretch self-end self-start",
            Span::call_site(),
        ))
        .expect("the alignment fixture should be supported");
        let reverse = CompiledClasses::parse(&LitStr::new(
            "self-start self-end items-stretch items-center justify-end content-start place-content-evenly",
            Span::call_site(),
        ))
        .expect("the reverse alignment fixture should be supported");

        for cascade in [&forward.regular, &reverse.regular] {
            assert_eq!(
                resolved_style_enum_variant(cascade, Property::AlignContent),
                "FlexStart"
            );
            assert_eq!(
                resolved_style_enum_variant(cascade, Property::JustifyContent),
                "FlexEnd"
            );
            assert_eq!(
                resolved_style_enum_variant(cascade, Property::AlignItems),
                "Stretch"
            );
            assert_eq!(
                resolved_style_enum_variant(cascade, Property::AlignSelf),
                "FlexStart"
            );
        }

        for fixture in [
            "justify-evenly justify-stretch",
            "justify-stretch justify-evenly",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("GPUI exposes both exact justification values");
            assert_eq!(
                resolved_style_enum_variant(&classes.regular, Property::JustifyContent),
                "Stretch"
            );
        }
    }

    /// Keeps both fields of important `place-content-*` above ordinary longhands.
    #[test]
    fn important_place_content_blocks_each_ordinary_longhand() {
        for fixture in [
            "place-content-stretch! content-start justify-end",
            "justify-end content-start place-content-stretch!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("the important place-content fixture should be supported");
            for property in [Property::AlignContent, Property::JustifyContent] {
                assert_eq!(
                    resolved_style_enum_variant(&classes.regular, property),
                    "Stretch"
                );
                assert!(resolved_declaration(&classes.regular, property).important);
            }
        }
    }

    /// Emits retained overflow methods only after the element becomes stateful.
    #[test]
    fn scroll_overflow_is_fieldwise_canonical_and_post_identity() {
        for fixture in [
            "overflow-scroll overflow-x-hidden overflow-y-scroll",
            "overflow-y-scroll overflow-x-hidden overflow-scroll",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("the static scroll fixture should be supported");
            assert_eq!(
                resolved_method(&classes.regular, Property::OverflowX),
                "overflow_x_hidden"
            );
            assert_eq!(
                resolved_stateful_method(&classes.regular, Property::OverflowY),
                "overflow_y_scroll"
            );
            assert!(classes.needs_stateful_id());

            let regular = classes
                .apply_regular(quote!(element), &quote!(::gpui_vue))
                .to_string();
            assert!(!regular.contains("overflow_y_scroll"));
            let post_identity = classes
                .apply_variants(quote!(element), &quote!(::gpui_vue))
                .to_string();
            assert!(post_identity.contains("overflow_y_scroll"));
        }

        for fixture in [
            "overflow-scroll! overflow-x-hidden overflow-y-hidden",
            "overflow-y-hidden overflow-x-hidden overflow-scroll!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("important broad scroll should be supported");
            assert_eq!(
                resolved_stateful_method(&classes.regular, Property::OverflowX),
                "overflow_x_scroll"
            );
            assert_eq!(
                resolved_stateful_method(&classes.regular, Property::OverflowY),
                "overflow_y_scroll"
            );
            assert!(resolved_declaration(&classes.regular, Property::OverflowX).important);
            assert!(resolved_declaration(&classes.regular, Property::OverflowY).important);
        }
    }

    /// Rejects layout values whose CSS semantics have no faithful GPUI seam.
    #[test]
    fn rejects_unrepresentable_layout_values_and_scroll_variants() {
        for (fixture, expected) in [
            ("hover:overflow-scroll", "retained overflow state"),
            ("content-normal", "cannot be represented faithfully"),
            ("self-auto", "cannot be represented faithfully"),
            ("items-center-safe", "safe-overflow"),
            ("order-1", "no public order field"),
            ("overflow-clip", "cannot be represented faithfully"),
            ("inline-flex", "no exact GPUI"),
        ] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("host-different layout utilities should be rejected");
            assert!(
                error.to_string().contains(expected),
                "expected `{fixture}` diagnostic to contain `{expected}`, got `{error}`"
            );
        }
    }

    /// Resolves broad, axis, and side shorthands independent of token order.
    #[test]
    fn canonical_shorthand_order_is_source_order_independent() {
        let forward = CompiledClasses::parse(&LitStr::new("p-4 px-2 pl-1", Span::call_site()))
            .expect("the fixture should be supported");
        let reverse = CompiledClasses::parse(&LitStr::new("pl-1 px-2 p-4", Span::call_site()))
            .expect("the reverse fixture should be supported");

        for cascade in [&forward.regular, &reverse.regular] {
            assert_eq!(
                resolved_length(cascade, Property::PaddingTop),
                LengthValue::Rems(1.0)
            );
            assert_eq!(
                resolved_length(cascade, Property::PaddingRight),
                LengthValue::Rems(0.5)
            );
            assert_eq!(
                resolved_length(cascade, Property::PaddingBottom),
                LengthValue::Rems(1.0)
            );
            assert_eq!(
                resolved_length(cascade, Property::PaddingLeft),
                LengthValue::Rems(0.25)
            );
        }
    }

    /// Keeps an important shorthand above a later ordinary declaration.
    #[test]
    fn important_suffix_beats_later_normal_candidate() {
        let classes = CompiledClasses::parse(&LitStr::new("p-4! px-2", Span::call_site()))
            .expect("the fixture should be supported");
        for property in [Property::PaddingLeft, Property::PaddingRight] {
            let declaration = classes
                .regular
                .declarations
                .iter()
                .find(|declaration| declaration.property == property)
                .expect("the padding side should exist");
            assert!(declaration.important);
            assert!(matches!(
                declaration.value,
                PropertyValue::Length {
                    value: LengthValue::Rems(1.0),
                    ..
                }
            ));
        }
    }

    /// Resolves broad, side, and corner radii in Tailwind stylesheet order.
    #[test]
    fn physical_radius_slots_follow_stylesheet_order_in_both_source_orders() {
        let forward = CompiledClasses::parse(&LitStr::new(
            "rounded-3xl rounded-t-lg rounded-l-sm rounded-tl-none rounded-r-xl rounded-tr-xs rounded-b-md rounded-br-4xl rounded-bl-full",
            Span::call_site(),
        ))
        .expect("the physical radius fixture should be supported");
        let reverse = CompiledClasses::parse(&LitStr::new(
            "rounded-bl-full rounded-br-4xl rounded-b-md rounded-tr-xs rounded-r-xl rounded-tl-none rounded-l-sm rounded-t-lg rounded-3xl",
            Span::call_site(),
        ))
        .expect("the reverse physical radius fixture should be supported");

        for cascade in [&forward.regular, &reverse.regular] {
            assert_eq!(
                resolved_length(cascade, Property::RadiusTopLeft),
                LengthValue::Pixels(0.0)
            );
            assert_eq!(
                resolved_length(cascade, Property::RadiusTopRight),
                LengthValue::Rems(0.125)
            );
            assert_eq!(
                resolved_length(cascade, Property::RadiusBottomRight),
                LengthValue::Rems(2.0)
            );
            assert_eq!(
                resolved_length(cascade, Property::RadiusBottomLeft),
                LengthValue::Pixels(9_999.0)
            );
        }

        for (fixture, count) in [("rounded-4xl", 4), ("rounded-t-lg", 2), ("rounded-tl", 1)] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("each radius shorthand should expand into typed corner slots");
            assert_eq!(classes.regular.declarations.len(), count);
        }

        for fixture in ["rounded-xs rounded-4xl", "rounded-4xl rounded-xs"] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("radius suffix order should be independent of class order");
            for property in [
                Property::RadiusTopLeft,
                Property::RadiusTopRight,
                Property::RadiusBottomRight,
                Property::RadiusBottomLeft,
            ] {
                assert_eq!(
                    resolved_length(&classes.regular, property),
                    LengthValue::Rems(0.125)
                );
            }
        }
    }

    /// Accepts every physical prefix with every Tailwind 4.3.2 named radius.
    #[test]
    fn parses_the_complete_physical_named_radius_matrix() {
        for (prefix, expected_slots) in [
            ("rounded", 4),
            ("rounded-t", 2),
            ("rounded-r", 2),
            ("rounded-b", 2),
            ("rounded-l", 2),
            ("rounded-tl", 1),
            ("rounded-tr", 1),
            ("rounded-br", 1),
            ("rounded-bl", 1),
        ] {
            for suffix in [
                "", "-none", "-xs", "-sm", "-md", "-lg", "-xl", "-2xl", "-3xl", "-4xl", "-full",
            ] {
                let literal = LitStr::new(&format!("{prefix}{suffix}"), Span::call_site());
                let classes = CompiledClasses::parse(&literal)
                    .expect("every physical named radius should lower");
                assert_eq!(classes.regular.declarations.len(), expected_slots);
            }
        }
    }

    /// Applies important radius shorthands independently to all four corners.
    #[test]
    fn important_radius_compounds_block_each_ordinary_corner() {
        for fixture in [
            "rounded-3xl! rounded-t-lg rounded-tl-none",
            "rounded-tl-none rounded-t-lg rounded-3xl!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("the important radius fixture should be supported");
            for property in [
                Property::RadiusTopLeft,
                Property::RadiusTopRight,
                Property::RadiusBottomRight,
                Property::RadiusBottomLeft,
            ] {
                assert_eq!(
                    resolved_length(&classes.regular, property),
                    LengthValue::Rems(1.5)
                );
                assert!(resolved_declaration(&classes.regular, property).important);
            }
        }

        let all_important = CompiledClasses::parse(&LitStr::new(
            "rounded-tl-none! rounded-t-lg! rounded-3xl!",
            Span::call_site(),
        ))
        .expect("important candidates should retain canonical family order");
        assert_eq!(
            resolved_length(&all_important.regular, Property::RadiusTopLeft),
            LengthValue::Pixels(0.0)
        );
        assert_eq!(
            resolved_length(&all_important.regular, Property::RadiusTopRight),
            LengthValue::Rems(0.5)
        );
    }

    /// Expands text and overflow compounds into independent field slots.
    #[test]
    fn compound_text_slots_follow_stylesheet_order_in_both_source_orders() {
        let forward = CompiledClasses::parse(&LitStr::new(
            "truncate overflow-hidden overflow-x-hidden overflow-y-hidden text-ellipsis whitespace-normal",
            Span::call_site(),
        ))
        .expect("the fixture should be supported");
        let reverse = CompiledClasses::parse(&LitStr::new(
            "whitespace-normal text-ellipsis overflow-y-hidden overflow-x-hidden overflow-hidden truncate",
            Span::call_site(),
        ))
        .expect("the reverse fixture should be supported");

        for cascade in [&forward.regular, &reverse.regular] {
            assert_eq!(
                resolved_method(cascade, Property::OverflowX),
                "overflow_x_hidden"
            );
            assert_eq!(
                resolved_method(cascade, Property::OverflowY),
                "overflow_y_hidden"
            );
            assert_eq!(
                resolved_method(cascade, Property::TextOverflow),
                "text_ellipsis"
            );
            assert_eq!(
                resolved_method(cascade, Property::WhiteSpace),
                "whitespace_normal"
            );
        }

        let overflow = CompiledClasses::parse(&LitStr::new("overflow-hidden", Span::call_site()))
            .expect("the broad overflow utility should be supported");
        assert!(
            [Property::OverflowX, Property::OverflowY]
                .into_iter()
                .all(
                    |property| resolved_declaration(&overflow.regular, property).property
                        == property
                )
        );
    }

    /// Resolves flex shorthands before Tailwind's shrink, grow, and basis rules.
    #[test]
    fn flex_slots_follow_stylesheet_order_in_both_source_orders() {
        let forward = CompiledClasses::parse(&LitStr::new(
            "flex-none grow shrink basis-full",
            Span::call_site(),
        ))
        .expect("the fixture should be supported");
        let reverse = CompiledClasses::parse(&LitStr::new(
            "basis-full shrink grow flex-none",
            Span::call_site(),
        ))
        .expect("the reverse fixture should be supported");

        for cascade in [&forward.regular, &reverse.regular] {
            assert_resolved_style_float(cascade, Property::FlexGrow, 1.0);
            assert_resolved_style_float(cascade, Property::FlexShrink, 1.0);
            assert_eq!(
                resolved_length(cascade, Property::FlexBasis),
                LengthValue::Relative(1.0)
            );
        }

        for fixture in ["flex-1 flex-none", "flex-none flex-1"] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("flex shorthand order should be supported");
            assert_resolved_style_float(&classes.regular, Property::FlexGrow, 0.0);
            assert_resolved_style_float(&classes.regular, Property::FlexShrink, 0.0);
            assert_eq!(
                resolved_length(&classes.regular, Property::FlexBasis),
                LengthValue::Auto
            );
        }

        for fixture in ["grow grow-0 shrink shrink-0", "shrink-0 shrink grow-0 grow"] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("flex longhand order should be supported");
            assert_resolved_style_float(&classes.regular, Property::FlexGrow, 0.0);
            assert_resolved_style_float(&classes.regular, Property::FlexShrink, 0.0);
        }
    }

    /// Keeps important compound fields above later ordinary longhands.
    #[test]
    fn important_compounds_block_each_conflicting_field() {
        for fixture in [
            "flex-none! grow shrink basis-full",
            "basis-full shrink grow flex-none!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("important compound fixture should be supported");
            assert_resolved_style_float(&classes.regular, Property::FlexGrow, 0.0);
            assert_resolved_style_float(&classes.regular, Property::FlexShrink, 0.0);
            assert_eq!(
                resolved_length(&classes.regular, Property::FlexBasis),
                LengthValue::Auto
            );
            for property in [
                Property::FlexGrow,
                Property::FlexShrink,
                Property::FlexBasis,
            ] {
                assert!(resolved_declaration(&classes.regular, property).important);
            }
        }

        for fixture in [
            "truncate! whitespace-normal overflow-x-hidden",
            "overflow-x-hidden whitespace-normal truncate!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("important truncate fixture should be supported");
            assert_eq!(
                resolved_method(&classes.regular, Property::WhiteSpace),
                "whitespace_nowrap"
            );
            assert!(resolved_declaration(&classes.regular, Property::WhiteSpace).important);
            assert!(resolved_declaration(&classes.regular, Property::OverflowX).important);
        }
    }

    /// Supports dynamic scales, negative safe families, and arbitrary units.
    #[test]
    fn parses_dynamic_and_arbitrary_lengths() {
        let classes = CompiledClasses::parse(&LitStr::new(
            "p-13.5 -mx-[4px] w-[62.5%] top-auto",
            Span::call_site(),
        ))
        .expect("the fixture should be supported");
        assert!(classes.regular.declarations.iter().any(|declaration| {
            matches!(
                declaration.value,
                PropertyValue::Length {
                    value: LengthValue::Rems(3.375),
                    ..
                }
            )
        }));
        assert!(classes.regular.declarations.iter().any(|declaration| {
            matches!(
                declaration.value,
                PropertyValue::Length {
                    value: LengthValue::Pixels(-4.0),
                    ..
                }
            )
        }));
        assert!(classes.regular.declarations.iter().any(|declaration| {
            matches!(
                declaration.value,
                PropertyValue::Length {
                    value: LengthValue::Relative(0.625),
                    ..
                }
            )
        }));
    }

    /// Distinguishes Tailwind's bare percentage scale from arbitrary alpha.
    #[test]
    fn parses_scaled_and_arbitrary_opacity() {
        for (fixture, expected) in [
            ("opacity-50", 0.5_f32),
            ("opacity-12.5", 0.125_f32),
            ("opacity-[.5]", 0.5_f32),
            ("opacity-[50%]", 0.5_f32),
            ("opacity-[.5%]", 0.005_f32),
            ("opacity-0", 0.0_f32),
            ("opacity-[1]", 1.0_f32),
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("valid opacity should lower");
            assert_eq!(
                resolved_opacity(&classes.regular).to_bits(),
                expected.to_bits()
            );
        }
    }

    /// Rejects non-finite, ambiguous, malformed, and out-of-range opacity.
    #[test]
    fn rejects_invalid_arbitrary_opacity() {
        for fixture in [
            "opacity-[50]",
            "opacity-[1.1]",
            "opacity-[-1]",
            "opacity-[101%]",
            "opacity-[NaN]",
            "opacity-[inf]",
            "opacity-[.5",
            "opacity-.5]",
            "opacity-50%",
            "opacity-",
        ] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("invalid opacity must be rejected");
            assert!(error.to_string().contains("invalid opacity"));
        }
    }

    /// Resolves functional, arbitrary-fraction, and named aspect ratios.
    #[test]
    fn aspect_ratio_follows_canonical_and_important_order() {
        for fixture in [
            "aspect-4/3 aspect-16/9 aspect-[1/2] aspect-square aspect-video",
            "aspect-video aspect-square aspect-[1/2] aspect-16/9 aspect-4/3",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("the aspect ratio fixture should be supported");
            assert_eq!(
                resolved_style_float(&classes.regular, Property::AspectRatio).to_bits(),
                (16.0_f32 / 9.0).to_bits()
            );
        }

        for fixture in ["aspect-16/9 aspect-[1/2]", "aspect-[1/2] aspect-16/9"] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("bracketed fractions follow functional fractions");
            assert_eq!(
                resolved_style_float(&classes.regular, Property::AspectRatio).to_bits(),
                0.5_f32.to_bits()
            );
        }

        let important = CompiledClasses::parse(&LitStr::new(
            "aspect-square! aspect-video",
            Span::call_site(),
        ))
        .expect("important aspect ratio should be supported");
        assert_eq!(
            resolved_style_float(&important.regular, Property::AspectRatio).to_bits(),
            1.0_f32.to_bits()
        );
        assert!(resolved_declaration(&important.regular, Property::AspectRatio).important);
    }

    /// Rejects ratios GPUI cannot safely represent or clear in every state.
    #[test]
    fn rejects_unsafe_aspect_ratio() {
        for fixture in [
            "aspect-auto",
            "aspect-0/1",
            "aspect-1/0",
            "aspect-inf/1",
            "aspect-[1.5]",
            "aspect-[4/3",
            "aspect-4/3/2",
        ] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("unsafe aspect ratio must be rejected");
            assert!(
                error.to_string().contains("aspect ratio")
                    || error.to_string().contains("aspect-auto")
            );
        }
    }

    /// Resolves numeric spacing and named line heights in stylesheet order.
    #[test]
    fn line_height_follows_canonical_and_important_order() {
        for fixture in [
            "leading-2 leading-10 leading-loose leading-tight",
            "leading-tight leading-loose leading-10 leading-2",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("the line-height fixture should be supported");
            assert_eq!(
                resolved_length(&classes.regular, Property::LineHeight),
                LengthValue::Relative(1.25)
            );
        }

        for fixture in ["leading-2 leading-10", "leading-10 leading-2"] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("numeric line height should follow numeric order");
            assert_eq!(
                resolved_length(&classes.regular, Property::LineHeight),
                LengthValue::Rems(2.5)
            );
        }

        let important = CompiledClasses::parse(&LitStr::new(
            "leading-loose! leading-tight",
            Span::call_site(),
        ))
        .expect("important line height should be supported");
        assert_eq!(
            resolved_length(&important.regular, Property::LineHeight),
            LengthValue::Relative(2.0)
        );
        assert!(resolved_declaration(&important.regular, Property::LineHeight).important);
    }

    /// Rejects arbitrary, signed, negative, and non-finite leading values.
    #[test]
    fn rejects_unsupported_line_height() {
        for fixture in [
            "leading-[1.5]",
            "leading-[20px]",
            "leading--1",
            "leading-+1",
            "leading-inf",
            "leading-",
        ] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("unsupported line height must be rejected");
            assert!(error.to_string().contains("line height"));
        }
    }

    /// Rejects negative padding because GPUI and Tailwind do not allow it.
    #[test]
    fn rejects_negative_non_whitelisted_family() {
        let error = CompiledClasses::parse(&LitStr::new("-p-2", Span::call_site()))
            .expect_err("negative padding must be rejected");
        assert!(error.to_string().contains("negative"));
    }

    /// Parses arbitrary hexadecimal colors without runtime work.
    #[test]
    fn parses_arbitrary_hex_colors() {
        let classes = CompiledClasses::parse(&LitStr::new(
            "bg-[#abc] text-[#123456] border-[#10203080]",
            Span::call_site(),
        ))
        .expect("the fixture should be supported");
        assert_eq!(classes.regular.declarations.len(), 3);
    }

    /// Lowers named and arbitrary color alpha without byte-quantizing opacity.
    #[test]
    fn parses_palette_and_arbitrary_color_alpha() {
        let classes = CompiledClasses::parse(&LitStr::new(
            "bg-blue-500/50 text-[#123456]/[12.5%] border-[#10203080]/[0.5]",
            Span::call_site(),
        ))
        .expect("the color alpha fixture should be supported");

        let expected = [
            (
                Property::BackgroundColor,
                tailwind_palette::rgb("blue-500").expect("blue should be in the palette"),
                0.5,
            ),
            (Property::TextColor, 0x0012_3456, 0.125),
            (
                Property::BorderColor,
                0x0010_2030,
                f32::from(0x80_u8) / 255.0 * 0.5,
            ),
        ];
        for (property, expected_rgb, expected_alpha) in expected {
            let PackedColor::RgbAlpha { rgb, alpha } = resolved_color(&classes.regular, property)
            else {
                panic!("an alpha modifier should produce an f32-alpha color");
            };
            assert_eq!(rgb, expected_rgb);
            assert_eq!(alpha.to_bits(), expected_alpha.to_bits());
        }
    }

    /// Rejects alpha values outside GPUI's valid inclusive range.
    #[test]
    fn rejects_unsafe_color_alpha() {
        for fixture in [
            "bg-blue-500/101",
            "text-red-500/[-0.1]",
            "border-[#123456]/[101%]",
        ] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("out-of-range alpha should be rejected");
            assert!(error.to_string().contains("color alpha"));
        }
    }

    /// Maps supported grid counts exactly and resolves them canonically.
    #[test]
    fn parses_uniform_grid_tracks_with_canonical_and_important_order() {
        let forward = CompiledClasses::parse(&LitStr::new(
            "grid grid-cols-2 grid-cols-12 grid-rows-3 grid-rows-7",
            Span::call_site(),
        ))
        .expect("the grid fixture should be supported");
        let reverse = CompiledClasses::parse(&LitStr::new(
            "grid-rows-7 grid-rows-3 grid-cols-12 grid-cols-2 grid",
            Span::call_site(),
        ))
        .expect("the reverse grid fixture should be supported");
        for cascade in [&forward.regular, &reverse.regular] {
            assert_eq!(resolved_grid_tracks(cascade, Property::GridColumns), 12);
            assert_eq!(resolved_grid_tracks(cascade, Property::GridRows), 7);
        }

        let important = CompiledClasses::parse(&LitStr::new(
            "grid-cols-2! grid-cols-12 grid-rows-65535",
            Span::call_site(),
        ))
        .expect("important grid tracks and GPUI's upper bound should compile");
        assert_eq!(
            resolved_grid_tracks(&important.regular, Property::GridColumns),
            2
        );
        assert!(resolved_declaration(&important.regular, Property::GridColumns).important);
        assert_eq!(
            resolved_grid_tracks(&important.regular, Property::GridRows),
            u16::MAX
        );
    }

    /// Rejects track counts that GPUI 0.2.2 cannot represent.
    #[test]
    fn rejects_unrepresentable_grid_tracks() {
        for fixture in ["grid-cols-0", "grid-rows-65536"] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("unrepresentable grid counts should be rejected");
            assert!(error.to_string().contains("grid track count"));
        }
    }

    /// Resolves grid shorthands and longhands into four independent endpoints.
    #[test]
    fn grid_placement_follows_canonical_order_in_both_source_orders() {
        let forward = CompiledClasses::parse(&LitStr::new(
            "col-span-3 col-start-2 -col-end-3 col-end-12 row-span-full row-start-2",
            Span::call_site(),
        ))
        .expect("the grid placement fixture should be supported");
        let reverse = CompiledClasses::parse(&LitStr::new(
            "row-start-2 row-span-full col-end-12 -col-end-3 col-start-2 col-span-3",
            Span::call_site(),
        ))
        .expect("the reverse grid placement fixture should be supported");

        for cascade in [&forward.regular, &reverse.regular] {
            assert_eq!(
                resolved_grid_placement(cascade, Property::GridColumnStart),
                GridPlacementValue::Line(2)
            );
            assert_eq!(
                resolved_grid_placement(cascade, Property::GridColumnEnd),
                GridPlacementValue::Line(12)
            );
            assert_eq!(
                resolved_grid_placement(cascade, Property::GridRowStart),
                GridPlacementValue::Line(2)
            );
            assert_eq!(
                resolved_grid_placement(cascade, Property::GridRowEnd),
                GridPlacementValue::Line(-1)
            );
        }

        for fixture in ["col-auto col-span-3", "col-span-3 col-auto"] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("numeric spans should follow auto shorthands");
            for property in [Property::GridColumnStart, Property::GridColumnEnd] {
                assert_eq!(
                    resolved_grid_placement(&classes.regular, property),
                    GridPlacementValue::Span(3)
                );
            }
        }
    }

    /// Applies important grid shorthands independently to both endpoints.
    #[test]
    fn important_grid_placement_blocks_each_ordinary_longhand() {
        for fixture in [
            "col-span-full! col-start-2 col-end-2",
            "col-end-2 col-start-2 col-span-full!",
        ] {
            let classes = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect("important grid shorthand should be supported");
            assert_eq!(
                resolved_grid_placement(&classes.regular, Property::GridColumnStart),
                GridPlacementValue::Line(1)
            );
            assert_eq!(
                resolved_grid_placement(&classes.regular, Property::GridColumnEnd),
                GridPlacementValue::Line(-1)
            );
            assert!(resolved_declaration(&classes.regular, Property::GridColumnStart).important);
            assert!(resolved_declaration(&classes.regular, Property::GridColumnEnd).important);
        }

        let important_longhand = CompiledClasses::parse(&LitStr::new(
            "row-span-2! row-start-auto!",
            Span::call_site(),
        ))
        .expect("important longhand should retain Tailwind order");
        assert_eq!(
            resolved_grid_placement(&important_longhand.regular, Property::GridRowStart),
            GridPlacementValue::Auto
        );
        assert_eq!(
            resolved_grid_placement(&important_longhand.regular, Property::GridRowEnd),
            GridPlacementValue::Span(2)
        );
    }

    /// Covers GPUI's exact placement bounds and rejects unsafe values.
    #[test]
    fn validates_grid_placement_bounds() {
        let boundaries = CompiledClasses::parse(&LitStr::new(
            "col-span-65535 -col-start-32768 row-end-32767",
            Span::call_site(),
        ))
        .expect("GPUI placement boundaries should compile");
        assert_eq!(
            resolved_grid_placement(&boundaries.regular, Property::GridColumnStart),
            GridPlacementValue::Line(i16::MIN)
        );
        assert_eq!(
            resolved_grid_placement(&boundaries.regular, Property::GridColumnEnd),
            GridPlacementValue::Span(u16::MAX)
        );
        assert_eq!(
            resolved_grid_placement(&boundaries.regular, Property::GridRowEnd),
            GridPlacementValue::Line(i16::MAX)
        );

        for fixture in [
            "col-span-0",
            "row-span-65536",
            "col-start-0",
            "col-start-32768",
            "-row-end-32769",
            "row-start-magic",
        ] {
            let error = CompiledClasses::parse(&LitStr::new(fixture, Span::call_site()))
                .expect_err("unsafe grid placement should be rejected");
            assert!(error.to_string().contains("grid placement"));
        }
    }

    /// Routes every Tailwind 4.3.2 palette family through utility lowering.
    #[test]
    fn parses_every_default_palette_family() {
        for family in [
            "red", "orange", "amber", "yellow", "lime", "green", "emerald", "teal", "cyan", "sky",
            "blue", "indigo", "violet", "purple", "fuchsia", "pink", "rose", "slate", "gray",
            "zinc", "neutral", "stone", "mauve", "olive", "mist", "taupe",
        ] {
            let literal = LitStr::new(&format!("bg-{family}-500"), Span::call_site());
            let classes = CompiledClasses::parse(&literal)
                .expect("every official default palette should lower");
            assert_eq!(classes.regular.declarations.len(), 1);
        }
    }

    /// Records that plain borders cannot synthesize Tailwind's `currentColor`.
    #[test]
    fn plain_border_only_emits_width() {
        let classes = CompiledClasses::parse(&LitStr::new("border", Span::call_site()))
            .expect("width-only border remains accepted with a documented limitation");
        assert_eq!(classes.regular.declarations.len(), 4);
        assert!(
            classes
                .regular
                .declarations
                .iter()
                .all(|declaration| declaration.property != Property::BorderColor)
        );
    }

    /// Reports an unsupported utility as a macro error.
    #[test]
    fn unknown_utility_is_an_error() {
        let error = CompiledClasses::parse(&LitStr::new("magic-42", Span::call_site()))
            .expect_err("the fixture should be rejected");
        assert!(error.to_string().contains("unknown"));
    }
}
