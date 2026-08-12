# Architecture decision: a Vue-inspired compile-time frontend for GPUI

## Decision

Use PocketJS and Vue Vapor as compiler-design references, but implement the default path as a Rust proc-macro frontend that emits ordinary GPUI element builders. Do not embed PocketJS's engine or a JavaScript runtime in the GPUI render path.

```text
view! / component! + enumerable classes
                   │ compile time
                   ▼
          native GPUI builders ◄──── Ref<T> / Local<T> + Context::notify()
                   │
                   ▼
      GPUI Element / Taffy layout / text / input
                   │
                   ▼
             GPUI GPU renderer
```

The proc macro is the compiler boundary. `crates/gpui-vue-macros/src/view.rs` owns template parsing, direct component-markup/slot-outlet lowering, and structural/component-host code generation; `tailwind.rs` validates candidates and resolves a typed property cascade; `tailwind_palette.rs` holds the pre-lowered Tailwind 4.3.2 default palette; `component.rs` emits ordinary Rust props/input/component/event/slot items, `AppContext` entity constructors, direct-markup `Render` implementations, and statically dispatched lifecycle hooks. Runtime `crates/gpui-vue/src/component.rs` retains keyed component entities in GPUI element state and transparently delegates them through `HostedEntity`; `crates/gpui-vue/src/slot.rs` supplies the typed lazy `Slot<P>` / `SlotContent` render boundary. `crates/gpui-vue/src/reactivity.rs` contains shared reactive handles, while `crates/gpui-vue/src/local.rs` contains inline local state, revision tokens, and explicit typed memo caches. GPUI remains the only host tree and renderer.

## Alternatives considered

| Option | Vue source compatibility | Runtime cost | GPUI integration | Decision |
| --- | --- | --- | --- | --- |
| Embed PocketJS core and Vue/QuickJS guest | High within PocketJS's supported subset | JS engine, FFI, mirror tree, and a second native UI engine | GPUI mostly becomes a window/surface host | Reject as the default |
| Vue `runtime-core` custom renderer over a GPUI host tree | Higher and officially supported for VDOM renderers | JS engine, FFI, and VDOM patching remain | Requires retained host-node reconciliation with GPUI's immediate elements | Possible compatibility experiment |
| Build a PocketJS-like Vue Vapor adapter and DOM-shaped facade | Vapor syntax and fine-grained guest mutations | Version-pinned facade/adapter compatibility and JS runtime | High maintenance; Vapor has no stable custom-renderer seam | Optional only if exact Vue is mandatory |
| Rust `view!` compiler directly to GPUI | Vue-inspired rather than source-compatible | No guest runtime, FFI, VDOM, or runtime class parser | Preserves all GPUI primitives and performance work | Selected |

## What “Vapor-inspired” means here

- Static syntax is analyzed at compile time.
- The template does not construct a framework-owned VNode tree.
- Literal class lists and statically enumerable `:class` branches become typed GPUI property assignments; a static base is merged into every dynamic leaf, and each reached Rust condition is evaluated once. Unknown classes and unsafe arbitrary values fail the build.
- The complete Tailwind 4.3.2 built-in color-name table is frozen as packed sRGB at compile time; bounded color-alpha modifiers are also validated and lowered ahead of time. Application rendering performs neither OKLCH conversion nor palette lookup.
- `component!` emits ordinary Rust types, typestate props builders, typed component input/slot storage, direct-markup `Render`, native events, and monomorphized lifecycle hooks rather than a framework-owned component, event, lifecycle, or slot tree.
- A self-closing or slot-bearing PascalCase component tag compiles to a native keyed `ComponentElement` or typed `ComponentEventElement`; the child entity, slot providers, and event subscription are reconciled through GPUI's own per-window element state rather than a VNode patcher.
- `v-if` becomes GPUI's conditional builder path; `v-for` becomes an iterator passed to `children`.
- Events become GPUI listeners and retain GPUI's element identity rules.

It does **not** currently mean per-text-node reactive effects or hoisted retained subtrees. GPUI 0.2.2 rebuilds a view's immediate element tree when its entity is notified. `Ref<T>` and `Local<T>` therefore invalidate at entity granularity. `Memo<T, D>` can avoid recomputing an explicitly keyed derived value, but it is not an automatic effect graph. A later optimizer must demonstrate a measurable benefit before introducing retained bindings or a second scheduler.

## Tailwind-native state and layout lowering

Every supported class becomes one or more typed property declarations before GPUI code is quoted. `place-content-*` is therefore not emitted as one broad builder call: it writes independent `AlignContent` and `JustifyContent` slots. Representable `items-*`, `content-*`, `justify-*`, and `self-*` values assign GPUI's public enums directly, including the flex-relative `FlexStart`/`FlexEnd` distinction that GPUI's `justify_start`/`justify_end` helpers do not preserve. Tailwind 4.3.2 candidate order and trailing `!` choose a winner per slot, so a longhand can replace only its half of `place-content-*` regardless of class-token order. These start/end values are exact GPUI/Taffy enum mappings for the exposed axes; they do not add browser writing-mode or direction resolution.

Each state owns a resolved cascade, but GPUI's runtime refinement order is fixed: `in-focus`, `focus`, `group-hover`, `hover`, `group-active`, then `active`. Tailwind 4.3.2's effective low-to-high precedence for the supported set is `in-focus`, `group-hover`, `group-active`, `hover`, `focus`, then `active`; `in-focus` stays lowest because its ancestor condition is under `:where(...)`. After regular important declarations suppress ordinary state fields, the compiler compares every remaining shared-property pair using Tailwind importance/precedence and GPUI's host order. It emits the pair only when both select the same winner and otherwise reports both variants and the property at the class literal. This permits independent fields and aligned state pairs without pretending that GPUI callbacks carry CSS importance.

The `group` marker plus `group-hover:` and `group-active:` lower to GPUI's native named-group APIs. GPUI paints same-name group hitboxes as a stack and resolves the last entry, so nested unqualified groups target the nearest group; Tailwind selectors can instead match any unqualified group ancestor. `group group-active:*` on the same element is rejected when any resolved group-active field survives regular importance: the group hitbox is pushed before that element registers its active mouse handler, allowing GPUI to capture itself, while Tailwind's selector requires a descendant. If regular important declarations suppress every group-active field, no visual self-match can occur and the spelling remains accepted. Group-hover style computation occurs before the element pushes its own group hitbox, so the analogous same-element spelling cannot self-match and stays supported. Tailwind's hover and group-hover output is additionally gated by `@media (hover: hover)` in a browser; GPUI hitbox hover has no hover-capability media-query layer.

`in-focus:` lowers to GPUI's native `in_focus` refinement. Its `within_focused` relation contains the focused ID itself as well as descendants, whereas Tailwind's implicit-ancestor form normally excludes self. The inverse `focus-within:` relation would require styling a target whose self or descendant is focused; GPUI 0.2.2 exposes no fluent `contains_focused` style callback, so that spelling is rejected explicitly.

Scroll is a separate post-identity lane. Hidden overflow remains an ordinary `Styled` refinement, but static `overflow-scroll`, `overflow-x-scroll`, and `overflow-y-scroll` resolve per axis to `StatefulInteractiveElement` methods. The final resolved cascade participates in stable-ID validation; regular style calls are emitted first, `.id(...)` changes the builder type, and only then are retained scroll methods and interaction callbacks appended. This is GPUI retained clipping and wheel scrolling; it does not synthesize browser UA scrollbar chrome or promise browser touch, keyboard, and accessibility behavior. Scroll under a state variant is rejected because a `StyleRefinement` callback cannot introduce retained scroll state. Alignment resets/inheritance, safe or last-baseline alignment, `order-*`, overflow auto/clip/visible, and display modes without an exact GPUI host value likewise fail at macro expansion rather than using an approximate substitution.

## Direct component template frontend

`component!` accepts `template(this, window, cx) { <direct markup> }` and lowers
that markup through the same typed template compiler as `view!`, with exact
component render bindings and declared-slot metadata. The earlier Rust-body form
remains compatible: it may return any `IntoElement` directly or invoke `view!`
explicitly. The direct form removes one layer of punctuation and enables
contextual slot providers; it does not parse a `.vue` file, JavaScript
expressions, `<script setup>`, or a Vue runtime. All expressions and scoped-slot
patterns remain Rust and are checked by rustc.

## Typed slots at the native boundary

A `component!` `slots { name: Props; }` section generates `<Component>Slots`
with typed `Slot<Props>` fields, empty `new`/`Default` construction, and fluent
`with_<name>` providers. The component's ordinary `new(props, cx)` path uses
empty slots; `new_with_slots(props, slots, cx)` passes explicit providers.
Rust-body templates can invoke `render` / `render_or_else` directly. Direct
component markup instead accepts child-side `<slot />` outlets, static
`name="actions"`, typed `:props={expr}`, and fallback children. Only a slot whose
declared props are syntactically `()` may omit `:props`; unknown/dynamic names and
missing or ill-typed props fail at compile time. Provider props are evaluated
only when a provider exists, and fallback roots are lowered only for the missing
branch. A nested missing outlet with no fallback contributes `None` to the
parent's children, preserving zero child cardinality; a missing outlet used as
the sole `Render` root becomes GPUI `Empty` to satisfy the return type.

A non-self-closing PascalCase tag maps ordinary children to the default slot and
direct `<template #name={RustPattern}>...</template>` children to named/scoped
providers. The complete `:slots={slots}` path remains available but is mutually
exclusive with declarative children. Each non-empty slot value stores one boxed
`'static` closure, is replaced on each parent render, and erases its concrete
`IntoElement` result once into one
[`AnyElement`](https://docs.rs/gpui/0.2.2/gpui/struct.AnyElement.html) inside
`SlotContent`; there is no VNode or collection wrapper.

Direct component templates use a contextual provider lane. The closure captures
the parent `WeakEntity`, then re-enters the live parent with `update` when the
child invokes it, binding the original `this` / `cx`, the supplied `window`, and
the scoped-props Rust pattern. This permits live parent reads and listeners
without a strong ownership cycle. If the owner has been released, the provider
returns `Empty`; because the provider itself was present, the child fallback is
not selected. Declarative providers authored in standalone `view!` have no
parent entity context and use ordinary owned-`'static` captures instead.

One declared slot currently permits only one outlet in a component template,
even across statically mutually exclusive branches. A repeated outlet is a
compile error because the zero-wrapper lane cannot yet assign distinct GPUI
identity to repeated provider content. Directives are rejected on the outlet
itself; conditional structure can wrap it. Multiple provider or render roots
still use the shared synthetic-root `div`, because GPUI 0.2.2 has no
wrapper-free slot-fragment host.

## Persistent native component boundary

Each generated component implements `NativeComponent` with one generated
`<Component>Input` containing comparable props and, when declared, typed slots.
A self-closing `<Child :label={label} />`, complete-value
`<Child :props={props} />`, or non-self-closing slot-bearing tag lowers to a
`ComponentElement` (or `ComponentEventElement` when listeners exist) whose
compile-site `ElementId` is stable; an optional user `key` / `:key` is nested
below that source-position identity. `Window::with_global_id` and
`Window::with_element_state` retain a native mount for that full identity.
Consecutive frames therefore reuse the native child `Entity`; changing the key
selects a different mount. The host returns a transparent `HostedEntity` adapter
whose request-layout, prepaint, and paint paths delegate directly to the entity,
introducing no layout or paint node. Its lifecycle render token is zero-sized
for a component without hooks, and that adapter has the same size as `Entity` in
the hook-free path.

During reconciliation, generated props are compared once with `PartialEq`,
stored, and notify the child entity only when changed. This requires every prop
field to implement `PartialEq`. Slot closures cannot be compared, so a declared
slots value is replaced every parent render without an extra notification; the
child renders later in that same frame and reads the current slots. A slot-bearing
input is conservatively reported lifecycle-dirty on every parent reconciliation,
because an opaque provider may change output, but this still adds no separate
`notify`. State initializers and `setup` run only when a new mount constructs the
entity, not on ordinary parent reconciliation.

The template frontend constructs props in one of two exclusive modes. A
`:props={...}` binding passes one already complete generated props value.
Otherwise every individual attribute becomes one exact-typed setter on
`<Component>Props::builder()`, followed by `build()`: `:foo={expr}` is a bound
expression, `:foo` reads the same-name Rust binding, `foo={expr}` is also a Rust
expression, a bare `enabled` passes `true`, and `label="literal"` passes the
literal `&str` value. Kebab-case source names normalize to snake-case setters
and same-name shorthand bindings.
The compiler does not insert `Into`, `to_owned`, or another string conversion,
so a `String` field requires an explicit Rust conversion. The macro rejects
mixing both construction modes and duplicate names after normalization; missing
required setters, unknown setters, and type mismatches intentionally surface as
the generated builder's rustc diagnostics.

## Visual lifecycle on the native host

Optional `mounted(this, window, cx)`, `updated(this, window, cx)`, and
`unmounted(this, cx)` sections generate one monomorphized
`ComponentLifecycleHooks` implementation. Hook-free components select `()` as
their mount state and register no observer, teardown listener, or lifecycle
allocation. A hook-bearing visual mount keeps one phase/dirty signal allocation;
only an `updated` declaration installs the self-notification observer, and only
an `unmounted` declaration keeps the weak entity/application teardown handles.

After a child entity delegates a successful draw, `HostedEntity` selects a
callback and schedules it with `Window::defer`. `mounted` runs once after the
first draw and `updated` after a later dirty draw, at the end of that GPUI effect
cycle. This is deliberately not Vue's DOM insertion or browser paint timing.
Dirty renders covered by an already queued callback are coalesced and consumed;
notifications that arrive after that render remain eligible for a later update.
Because descendant layout returns before ancestor layout, GPUI's FIFO deferred
effects give nested components child-before-parent `mounted` and `updated`
ordering.

Dropping keyed visual mount state first cancels its observer and invalidates any
queued mount/update callback. `unmounted` then runs at most once, and only for an
identity that completed a draw. If another owner keeps the component `Entity`
alive, a foreground task performs a weak update; if the host releases the last
strong reference, the entity's release listener is the fallback. Same-level
unmount order follows GPUI element-state teardown and is not guaranteed. During
application shutdown a queued foreground task is not guaranteed to be polled:
release covers the no-external-owner path, while an entity intentionally held by
an external owner through shutdown must not rely on `unmounted` for process
cleanup.

Lifecycle belongs to the persistent visual host, not mere entity existence.
Direct `Component::new` / `new_with_slots` construction does not attach it,
intrinsic `v-show` does not remove a visual identity, and an entity kept alive
after its tag disappears has nevertheless been visually unmounted. Dependency
injection and the rest of Vue's lifecycle surface remain separate work.

The generated host refers to props and input through `NativeComponent::Props`
and `NativeComponent::Input`, and to emitted events through
`NativeComponentEvents::Event`. Consequently a component imported under an
alias or through a module boundary does not require the parent macro to guess
new sibling names such as `AliasProps`, `AliasInput`, or `AliasEvent`.

GPUI includes the concrete element-state type in retained identity. The plain
host therefore retains the subscription-factory closure type in a zero-sized
marker, while the event host retains the concrete event and handler types. Two
macro expansions with the same source span and invocation-local ordinal cannot
accidentally share state merely because their textual `ElementId` is equal;
their distinct closure `TypeId`s keep the mounts separate without runtime
storage in the plain host.

## Typed component event boundary

`@change={handler}` and `on:change={handler}` on a PascalCase child subscribe to
the child's native generated event enum. Kebab-case names normalize to the
snake-case hidden dispatcher generated for the declared event. The handler is
fully typed as `FnMut(&<Component as NativeComponentEvents>::Event, &mut
Window, &mut App)`: even a listener for one variant receives the complete enum
and must pattern-match it. It does not receive Vue-style positional or
multi-argument payloads. A parent GPUI `Context::listener` is compatible with
this shape.

All listener expressions are evaluated once per parent render, in source
order, then captured by one concrete closure. One native `Window::subscribe`
callback dispatches that enum to every matching generated listener, so multiple
event names add neither a listener vector nor multiple subscriptions. On the
first frame for `(compile slot, key)`, the host creates one
`Rc<RefCell<Handler>>`; on later frames it replaces the concrete handler in
that existing cell before reconciling input. This keeps dispatch monomorphic
and avoids per-frame shared-cell allocation or resubscription. A component tag
with no listener selects the plain host and creates no handler cell or event
subscription.

The subscription is scoped to the direct child entity and does not bubble
through component ancestors. Mount ordering is also intentionally host-native:
the entity is constructed and its one-shot `setup` runs before
`Window::subscribe` installs the parent listener. An event emitted during that
construction window can therefore be missed. Later reconciliation replaces the
handler before applying child input, so synchronous reconciliation-time emits
observe the newest parent captures.

Every component-event modifier is rejected, including `.once`, `.stop`, and
the intrinsic click modifiers. Canonical duplicates such as `@value-change`
plus `on:value_change` are macro errors; an undeclared event is a missing Rust
dispatcher method, and an incompatible callback is a Rust type error. No DOM
event registry, bubbling layer, string lookup, boxed dispatcher, or erased
listener collection is introduced.

## Typestate props construction

Every generated props type retains `Props::new(...)`, default `with_*` setters,
and `Default` when all fields have defaults, and also exposes `Props::builder()`.
Required fields use inline, sealed `RequiredProp<T, State>` storage. The
`PropMissing` and `PropSet` markers are zero-sized; setters consume the builder,
take the declared Rust type, and move that field to `PropSet`. Only the builder
specialization where every required field is `PropSet` has `build()`. This makes
an omitted required prop a compile error while supporting move-only values and
avoiding a dynamic property map, trait object, or per-field heap allocation.

## Compile-time rules

1. A single unconditional intrinsic root lowers directly. Multiple roots, an explicit root fragment, a structural root, or a conditional root receive one synthetic `div` because GPUI 0.2.2 requires one `IntoElement` and has no wrapper-free `display: contents` host. Nested fragments and structural `<template>` children flatten into their parent.
2. `class` must be a literal. `:class` accepts a literal or nested Rust `if` tree whose leaves are literals; runtime class strings, arrays, and maps are rejected. A coexisting static class is prepended to each leaf before cascade resolution (and is the omitted-`else` fallback), so ordinary and state styles share one selected conditional traversal and each reached condition expression is emitted once.
3. Utilities lower into typed property slots. Important suffix `!` beats ordinary declarations; supported broad/axis/side shorthands resolve canonically independent of source order, while equal-rank conflicts remain later-wins. Flex shorthands split grow/shrink/basis fields, `truncate` and scroll split both overflow axes, `place-content-*` splits align/justify content, and physical radius utilities split per corner so a GPUI convenience method cannot reset another winning field. Unsupported classes, variants, and arbitrary-value forms are errors rather than silent degradation.
4. All 26 Tailwind 4.3.2 default color families and 11 shades are available to the implemented background, text, and border consumers. Their official OKLCH constants are preconverted to deterministic packed sRGB. These consumers accept bounded `/N`, `/[N%]`, and `/[0..1]` alpha; an alpha modifier on `#rrggbbaa` multiplies its normalized byte alpha and emits GPUI's `f32` alpha. GPUI 0.2.2 cannot retain wide-gamut components, CSS theme variables, or browser gamut-mapping behavior.
5. `@click`, `focusable`, `active:`, `group-active:`, `focus:`, `in-focus:`, and every resolved static scroll utility require a stable GPUI `ElementId`; focus and in-focus styling also make the element focusable. Scroll builders are emitted after `.id(...)`, before state callbacks. Click modifiers are evaluated in source order; `.passive` is rejected because GPUI has no DOM listener-registration equivalent.
6. Every `v-for` root requires a dynamic `:key` for per-item identity and to namespace stateful descendants.
7. Adjacent `v-if`/`v-else-if`/`v-else` siblings form one validated chain. When `v-if` and `v-for` share an element, the condition wraps the loop, so the loop alias is not in scope in the condition, matching Vue 3's precedence.
8. `v-show` lowers to a visibility refinement on a real host element. `<template v-show>` and `<template v-for>` are errors because a flattened structural node has no visibility or keyed fragment identity in GPUI 0.2.2.
9. `:id` and `:key` support an explicit braced Rust expression or the same-name shorthand; every loop still requires a dynamic key.
10. Rust expressions stay typed Rust; the compiler never string-evaluates bindings.
11. `grid-cols-N` and `grid-rows-N` map only positive `u16` counts (`1..=65535`) to GPUI's uniform tracks. Placement maps `col`/`row` auto, positive `u16` spans, full span, and independently cascading start/end lines onto GPUI [`GridPlacement`](https://docs.rs/gpui/0.2.2/gpui/enum.GridPlacement.html): explicit lines are restricted to its nonzero `i16` range (`-32768..=-1` or `1..=32767`). `none`, subgrid, arbitrary/custom-property placement, arbitrary track lists, and CSS template grammar have no equivalent in this mapping.
12. Element opacity uses GPUI's [`Styled::opacity(f32)`](https://docs.rs/gpui/0.2.2/gpui/trait.Styled.html#method.opacity). Bare `opacity-N` is a `0..=100` percentage; bracketed unit values are `0..=1`, and bracketed percentages carry `%`. The safe subset rejects ambiguous `opacity-[50]` rather than silently choosing percentage semantics.
13. Aspect ratios set GPUI's optional width/height `f32` refinement. `square`, `video`, and positive finite functional or bracketed `N/D` fractions are supported; `aspect-auto` is rejected because an absent refinement cannot reliably clear a ratio established by the regular or inherited style path.
14. Line height uses GPUI [`Styled::line_height`](https://docs.rs/gpui/0.2.2/gpui/trait.Styled.html#method.line_height): the six named values become relative definite lengths, while a bare finite nonnegative `leading-N` becomes `N × 0.25rem`. Bracket arbitrary leading is not accepted.
15. A declared slot may be invoked through the typed Rust API or one direct `<slot>` outlet. Static outlet names and scoped props are type checked; unit props may be omitted, non-unit props are required, and fallback is lazy. A second outlet for the same declared slot is conservatively rejected even across conditional branches. Providers yield one `SlotContent` / `AnyElement`; multiple roots receive a synthetic `div`.
16. A simple PascalCase identifier selects the component lane. Tags choose either a complete `:props={...}` value or individual attributes lowered through the generated typestate builder. They accept optional `:slots={...}`, or a non-self-closing body whose ordinary children provide `default` and whose direct `<template #name={pattern}>` children provide named slots; these modes cannot be mixed. `key` / `:key`, typed `@event` / `on:event` listeners, conditional chains, and keyed `v-for` are supported. Ordinary classes/IDs, `v-show`, and component-event modifiers remain rejected rather than introducing a semantic host wrapper.

## Evolution path

1. Grow the typed intrinsic/component surface, keyboard semantics, accessibility, and the documented class matrix.
2. Extend the persistent PascalCase host beyond the current typed prop/slot/event/lifecycle surface with carefully specified native event conveniences, provide/inject, and broader accessibility behavior while keeping `Render`/`Entity` as the ownership boundary.
3. Grow the explicit `Memo<T, D>` cache into dependency-aware computed/watch conveniences where entity subscriptions and GPUI's executor provide the correct ownership and disposal semantics.
4. If desired, add a `.vue`-like file frontend that emits the same internal template representation/Rust code. Keep it independent of the runtime.
5. Only if unmodified Vue TypeScript/SFC execution is a hard requirement, prototype a feature-gated compatibility crate with a pinned Vue version and differential tests.

## Version boundary

The implementation pins crates.io `gpui = 0.2.2`. GPUI is pre-1.0 and its API changes quickly; current Zed `main` documentation may not match this release. Upgrades should be explicit and should run macro expansion, reactivity, desktop example, and visual interaction tests.

Implemented, planned, and host-incompatible behaviors are tracked separately in
the [capability matrix](capability-matrix.md). A planned row there must not be
described as supported until the corresponding implementation and tests land.
