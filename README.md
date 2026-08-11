# gpui-vue

`gpui-vue` is a Vue-inspired, compile-time authoring layer for [GPUI](https://crates.io/crates/gpui). It borrows Vapor's compile-away direction while preserving GPUI's native entity, element, layout, input, text, and GPU renderer.

It currently provides:

- a compile-time `view!` template;
- a typed `component!` DSL with direct markup, props, local state, setup, visual lifecycle hooks, native `Render`, GPUI events, and lazy slots;
- persistent, keyed native hosts for self-closing and Vue-shaped non-self-closing PascalCase component tags;
- compile-time Tailwind-like static and conditional class lowering;
- fragments, conditional chains, `v-show`, keyed `v-for`, bindings, and click modifiers;
- allocation-free `Local<T>` state and revision-keyed `Memo<T, D>` caches;
- explicitly notified shared `Ref<T>` handles when shared ownership is intentional.

There is no JavaScript engine, DOM facade, VDOM, runtime class parser, FFI mutation queue, or second rendering tree.

## Counter

```rust,ignore
use gpui_vue::prelude::*;

component! {
    /// A retained native counter with one optional body slot.
    component Counter {
        state {
            /// Current count, stored inline in the GPUI entity.
            count: Local<i32> = Local::new(0),
        }

        slots {
            /// Optional content rendered below the button.
            default: ();
        }

        mounted(this, window, cx) {
            let _ = (this.count.get(), window.viewport_size(), cx.entity_id());
        }

        updated(this, window, cx) {
            let _ = (this.count.get(), window.viewport_size(), cx.entity_id());
        }

        unmounted(this, cx) {
            let _ = (this.count.get(), cx.background_executor());
        }

        template(this, _window, cx) {
            <view class="h-full flex flex-col items-center justify-center gap-4 bg-slate-950 text-white">
                <text class="text-3xl font-bold">{format!("Count: {}", this.count.get())}</text>
                <button
                    id="increment"
                    class="rounded-lg bg-blue-600 px-4 py-2 hover:bg-blue-500 active:bg-blue-700"
                    @click={cx.listener(|this, _, _, cx| {
                        this.count.update(|count| count + 1, cx);
                    })}
                >
                    "Increment"
                </button>
                <slot />
            </view>
        }
    }
}
```

Run the complete desktop example on Linux:

```bash
cargo run --example counter --features desktop
```

GPUI's desktop backend may require its documented platform development libraries. The default feature set is headless-friendly so compiler, state, and element-expansion tests do not need a display server.

## Template syntax

The intrinsic surface contains `div`, `view`, `text`, `span`, and `button`. All lower directly to native GPUI elements. `button` adds pointer, focus, tab-stop, keyboard activation, and GPUI click behavior, but is not yet a complete accessibility-role abstraction. Arbitrary GPUI elements can be inserted as Rust expressions.

```rust,ignore
view! {
    <>
        <template v-if={loading}>
            <text>"Loading"</text>
        </template>
        <text v-else-if={items.is_empty()}>"No items"</text>
        <div v-else class="flex flex-col gap-2">
            <div
                v-for={item in items}
                :key={("item", item.id)}
                class="rounded p-2"
                :class={if item.selected {
                    "bg-blue-600 hover:bg-blue-500"
                } else {
                    "bg-slate-900 hover:bg-slate-800"
                }}
            >
                {item.label}
            </div>
        </div>
        <button
            id="save"
            v-show={can_save}
            @click.stop.prevent={save_listener}
        >
            "Save"
        </button>
    </>
}
```

Interactive elements require a stable `id`. Every `v-for` root requires a dynamic `:key={...}` so each iteration namespaces its GPUI element state, including stateful descendants. `:id` and `:key` also accept Vue 3.4-style same-name shorthand.

Click handlers support `.stop`, `.prevent`, `.ctrl`, `.alt`, `.shift`, `.meta`, and `.exact` in source order. `.passive` is rejected because it configures a DOM listener rather than a GPUI event. Other event types and modifiers remain roadmap items.

Static `class` values must be literals. `:class` accepts a literal or nested Rust `if` expression whose leaves are literals. Every branch is compiled ahead of time, so runtime values select typed GPUI calls rather than parse strings. Unknown utilities, runtime class strings, mismatched tags, and unstable interactive nodes are compile errors.

Static and conditional classes are merged into one typed cascade per branch. Each reached condition is evaluated once, and each selected branch registers at most one GPUI callback for each supported interaction state, including when both `class` and `:class` contribute variants.

Nested fragments and structural `<template>` nodes flatten into their parent. GPUI 0.2.2 has no wrapper-free root fragment, so multiple roots receive one synthetic outer `div`. `<template v-for>` is rejected because GPUI cannot preserve keyed fragment identity without adding a layout node.

## Tailwind-oriented compiler

Supported behavior includes:

- flex/grid/display, positioning, cursors, hidden/static-scroll overflow, and typed alignment/content distribution;
- open numeric spacing, size, gap, and inset families, fractions, and safe negative margin/inset values;
- representable arbitrary lengths such as `w-[62.5%]`, `-mt-[4px]`, and `h-[2rem]`;
- font size/weight, canonical line-height values, line clamp, opacity, directional border width, radius, aspect ratio, and shadows;
- all 26 Tailwind 4.3.2 default color families and 286 shades, arbitrary hex colors, and compile-time color alpha modifiers;
- physical side/corner radius cascades, uniform grid tracks, and typed grid span/start/end placement;
- `hover:`, `active:`, `focus:`, `in-focus:`, `group-hover:`, and `group-active:` variants, plus the unqualified `group` marker;
- Tailwind v4's trailing important form, such as `bg-blue-500!`;
- property-slot conflict handling, including source-order-independent broad → axis → side precedence and fieldwise `place-content-*`/alignment conflicts.

Equal-rank conflicts still use the later candidate, stacked/responsive/data variants are not implemented, and many CSS-only families have no GPUI field. A plain Tailwind `border` relies on CSS `currentColor`, which GPUI 0.2.2 cannot reproduce; add an explicit `border-*` color. This is a growing Tailwind 4.3-oriented GPUI compiler, not complete CSS compatibility.

GPUI applies simultaneous state refinements in a fixed host order, while Tailwind 4.3.2 uses importance, selector specificity, and generated variant order. The compiler compares those winners per typed property slot: state combinations whose winner agrees are supported independent of class-token order, while a conflicting shared-property pair is a targeted compile error. This validation includes cross-state trailing `!`; regular-versus-state importance keeps its ordinary Tailwind behavior.

The unqualified `group` marker and `group-hover:` / `group-active:` use GPUI's native named-group hitbox stack. Nested same-name groups resolve to the nearest group in GPUI, while Tailwind's selector can match any unqualified group ancestor; named groups are not claimed. `group group-active:*` on one element is rejected when any group-active property can apply because GPUI can active-match the element's own group hitbox, unlike Tailwind's descendant selector; it is allowed when regular important declarations suppress every group-active property. Same-element `group-hover:` does not see that not-yet-pushed hitbox and remains accepted. Tailwind wraps hover variants in a browser hover-capability media query; GPUI's native hitbox hover has no equivalent media gate.

`in-focus:` uses GPUI's native ancestor-or-self focus relation, so it also matches the focused target itself, unlike Tailwind's usual implicit-ancestor selector. `focus-within:` asks the inverse question — whether self or a descendant has focus — and is a targeted compile error because GPUI 0.2.2 has no fluent `contains_focused` style seam.

Alignment values backed by GPUI's public enums include `items-stretch`, `justify-evenly`/`justify-stretch`, representable `self-*`, and `place-content-{around,between,center,end,evenly,start,stretch}`. `place-content-*` expands into independent align-content and justify-content slots; later Tailwind longhands and trailing `!` resolve per field in Tailwind 4.3.2 canonical order, independent of class-token order. The `start`/`end` mappings are exact for GPUI/Taffy's exposed layout axes, not a promise of browser writing-mode or direction behavior. Reset/inheritance forms (`content-normal`, `justify-normal`, `self-auto`), safe/last-baseline forms, and `order-*` fail at macro expansion rather than silently changing semantics.

`overflow-scroll`, `overflow-x-scroll`, and `overflow-y-scroll` are static-only retained-state utilities. They require a stable `id` (or loop key), and their GPUI stateful methods are emitted only after identity lowering. They provide GPUI's retained clipping and wheel-scrolling path, not browser UA scrollbar chrome or browser touch, keyboard, and accessibility parity. A scroll utility under an interaction variant is rejected because GPUI style callbacks cannot change retained scroll state; `overflow-auto`, `overflow-clip`, `overflow-visible`, and display modes without an exact GPUI counterpart also produce host-specific diagnostics.

Tailwind's default colors originate as OKLCH values. They are converted once into a checked, embedded sRGB table during development, so rendering does not parse colors or perform gamut conversion. This preserves the zero-parser hot path, while accepting that GPUI's packed sRGB color input cannot retain browser wide-gamut behavior exactly.

Color consumers accept `/N`, `/[N%]`, and `/[0..1]` alpha forms; an alpha modifier multiplies an existing `#rrggbbaa` alpha. The embedded RGB channels remain precomputed, while GPUI receives the final alpha as `f32`. Grid support includes positive uniform track counts plus representable row/column auto, span, full-span, start, and end placement. Reset forms such as `grid-cols-none` are rejected because GPUI cannot reliably clear inherited tracks through the same refinement API. `aspect-square`, `aspect-video`, positive finite fractions, and Tailwind's named/numeric leading scale are also compiled into typed GPUI refinements.

Element opacity distinguishes Tailwind's ordinary percentage scale (`opacity-50`) from arbitrary fractions (`opacity-[.5]`) and arbitrary percentages (`opacity-[50%]`); malformed, non-finite, ambiguous, or out-of-range values fail at macro expansion.

## Typed components

`component!` is the canonical Rust-native SFC frontend. It emits ordinary documented Rust items and a GPUI entity; it does not introduce another component runtime.

```rust,ignore
use gpui_vue::gpui::SharedString;
use gpui_vue::prelude::*;

/// Props passed to the counter's named action slot.
pub struct ActionSlotProps {
    /// Current counter value.
    pub count: i32,
}

component! {
    /// A retained counter component.
    pub component CounterCard {
        props {
            /// Heading shown above the count.
            pub label: SharedString,
            /// Initial counter value.
            pub initial: i32 = 0,
        }

        state {
            /// Allocation-free entity-local state.
            pub count: Local<i32> = Local::new(props.initial),
        }

        emits {
            /// Reports the new count through GPUI's native event channel.
            change(value: i32);
        }

        slots {
            /// Optional body supplied by the parent.
            default: ();
            /// Parent-rendered actions receiving the current count.
            actions: ActionSlotProps;
        }

        setup(this, props, _cx) {
            // Runs exactly once inside the entity constructor.
            debug_assert_eq!(this.count.get(), props.initial);
        }

        mounted(this, window, cx) {
            let _ = (this.count.get(), window.viewport_size(), cx.entity_id());
        }

        updated(this, window, cx) {
            let _ = (this.count.get(), window.viewport_size(), cx.entity_id());
        }

        unmounted(this, cx) {
            let _ = (this.count.get(), cx.background_executor());
        }

        template(this, window, cx) {
            <div class="flex flex-col gap-3 rounded-lg bg-slate-950 p-4 text-white">
                <button
                    id="increment"
                    class="rounded bg-blue-600 px-4 py-2 hover:bg-blue-500"
                    @click={cx.listener(|this, _, _, cx| {
                        this.count.update(|count| count + 1, cx);
                        CounterCard::emit_change(this.count.get(), cx);
                    })}
                >
                    {format!("{}: {}", this.props().label, this.count.get())}
                </button>
                <slot>
                    <text>"No body was supplied"</text>
                </slot>
                <slot
                    name="actions"
                    :props={ActionSlotProps { count: this.count.get() }}
                />
            </div>
        }
    }
}

fn counter_props() -> CounterCardProps {
    CounterCardProps::builder()
        .label("Clicks".into())
        .initial(2)
        .build()
}
```

Required props remain constructor parameters, defaulted props get `with_*` methods, and an all-default props type implements `Default`. Every props type also has a consuming typestate builder. Required fields are held inline by sealed `RequiredProp<T, State>` storage (`PropMissing` or `PropSet`); the zero-sized markers add no runtime state, and `build` exists only after every required field is `PropSet`. Setters take the declared Rust type exactly, support move-only values, and repeated calls replace the previous value without a dynamic map or boxed builder storage.

Generated props derive `PartialEq`, so every prop field must implement it. State initialization and `setup` occur only inside native entity construction; the template becomes a native GPUI `Render` implementation. The template may be a normal Rust block returning `IntoElement`, an explicit `view!`, or direct Vue-shaped markup as above. This remains a Rust proc-macro DSL—there is no `.vue` file parser, JavaScript template scope, or Vue runtime.

An `emits` section generates a documented `CounterCardEvent` enum, a native `EventEmitter<CounterCardEvent>` implementation, and typed `emit_*` helpers that call `Context::emit` directly. Unit events and named, multi-field payloads are supported without a string registry or event runtime. PascalCase parents can listen with `@change={handler}` or `on:change={handler}`; the handler receives `&CounterCardEvent`, `&mut Window`, and `&mut App`. It must inspect the complete enum itself rather than receive Vue-style positional payload arguments. A parent entity's `cx.listener(...)` produces the same native callback shape.

Typed slots are declared in the same component. Direct markup exposes them through `<slot />`, a static named outlet, typed scoped props, and lazy fallback children. Only a syntactic `()` slot may omit `:props`; a non-unit slot requires `:props={rust_expression}`. The props expression and fallback are evaluated only on the branch that needs them.

A direct-markup parent can provide ordinary children as the default slot and use `<template #name={RustPattern}>` for a named/scoped slot:

```rust,ignore
use gpui_vue::prelude::*;

component! {
    /// Parent that provides live, lazily rendered slot content.
    component Dashboard {
        state {
            /// Most recent child event value.
            latest: i32 = 0,
        }

        template(this, window, cx) {
            <CounterCard
                key="primary-counter"
                label={"Clicks".into()}
                initial={2}
                @change={cx.listener(|this, event: &CounterCardEvent, _window, cx| {
                    match event {
                        CounterCardEvent::Change { value } => this.latest = *value,
                    }
                    cx.notify();
                })}
            >
                <text>{format!("Latest: {}", this.latest)}</text>
                <template #actions={ActionSlotProps { count }}>
                    <text>{format!("Scoped count: {count} in {:?}", window.viewport_size())}</text>
                </template>
            </CounterCard>
        }
    }
}
```

`Slot<P>` stores one boxed render-time closure receiving typed scoped props plus GPUI `Window`/`App`; it erases one concrete element only when invoked and adds no `Vec` for the common one-node result. The declarative provider value is rebuilt and replaces the child's slot value on each parent render. In a direct component template, the provider captures only the parent's `WeakEntity`: invocation re-enters the live parent with `update`, so slot expressions and listeners can use current `this`, `cx`, and the supplied `window` without a strong ownership cycle. If that owner is gone, the provider yields `Empty`; it was still present, so the child's fallback is not selected. Declarative providers authored in standalone `view!` have no parent entity context and therefore use the ordinary owned-`'static` capture lane.

Components without a `slots` section emit no slot type, field, or alternate constructor. The generated `Slots` builder and explicit `:slots={slots}` escape hatch remain available, and Rust-body templates may still call `render` / `render_or_else` directly. A missing nested outlet without fallback contributes zero children; only a missing outlet that is itself the component's sole render root lowers to `Empty`. Multiple roots in either a component render boundary or a slot provider receive the usual synthetic `div`.

For now, one declared slot may appear at only one `<slot>` outlet in a component template—even across mutually exclusive branches. A second occurrence is conservatively rejected because the current zero-wrapper outlet lane cannot give repeated provider content distinct GPUI identity. Outlet names must be static and declared; outlets reject `v-if`, `v-for`, and `v-show` directly, so put conditional structure around them instead.

The PascalCase syntax accepts one simple generated component name. A component may be self-closing, or non-self-closing when ordinary children and/or direct `<template #name={pattern}>` providers supply its declared slots. Individual props lower in source order through the generated typestate builder:

- `:foo={expression}` passes a bound Rust expression;
- `:foo` is same-name shorthand and reads the Rust binding `foo`;
- `foo={expression}` passes an ordinary braced Rust expression;
- a bare boolean prop such as `enabled` passes `true`;
- `label="literal"` passes the string literal as `&str`—it does not allocate or implicitly convert to `String`;
- kebab-case such as `display-name` or `:owned-label` calls the generated `display_name` or `owned_label` setter.

Setter parameter types remain exact, so a `String` prop needs an explicit Rust conversion such as `label={"literal".to_owned()}`. Missing required props, unknown setters, and wrong types are ordinary rustc errors. The macro rejects duplicate canonical names (for example, `display-name` plus `display_name`) and rejects mixing individual props with the complete-value escape hatch `:props={CompleteProps}`.

Optional `:slots={TypedSlots}` and `key` / `:key` remain host bindings rather than props. `:slots` cannot be mixed with declarative children. PascalCase components participate in `v-if` / `v-else-if` / `v-else`, and `v-for` requires the same item-derived dynamic `:key` as intrinsic loops. This example deliberately uses `:props` to pass an already constructed value:

```rust,ignore
fn render_rows(rows: Vec<(u64, CounterCardProps)>) -> impl IntoElement {
    view! {
        <div>
            <CounterCard
                v-for={row in rows}
                :key={("counter", row.0)}
                :props={row.1}
            />
        </div>
    }
}
```

Each tag resolves generated types through the component's associated `NativeComponent::Props` / `Input` aliases, then lowers to a native `ComponentElement` or event-aware `ComponentEventElement`. This remains hygienic when a component is imported through a module alias. GPUI's keyed per-window element state retains the child `Entity` across consecutive parent frames, reconciles its input, and returns a transparent `HostedEntity` adapter that delegates request-layout, prepaint, and paint directly to that entity—there is no host `div` or other layout node. Components without lifecycle hooks keep a zero-sized render token, and `HostedEntity` has the same size as `Entity`. A key change selects a different mount identity. Props are compared once with `PartialEq`, replaced every parent render, and notify the child only when changed. Typed slots contain non-comparable closures, so they are replaced every parent render without an additional notification; the child renders later in the same frame and observes the replacement. Because an opaque slot replacement may affect output, a slot-bearing reconciliation is conservatively lifecycle-dirty on every parent render even though it does not issue an extra `notify`.

Component event names normalize from kebab-case to snake_case, so `@value-change` selects a generated `value_change` dispatcher. `@change` and `on:change` are aliases and therefore duplicates on one tag. Unknown event names and wrong handler signatures remain ordinary rustc errors; duplicate canonical names are macro errors. Every component-event modifier, including `.stop` and `.once`, is rejected rather than assigned DOM/Vue semantics.

All listener expressions on one child are evaluated exactly once per parent render and captured in one monomorphic closure. The first frame for a keyed identity creates one native GPUI subscription and one `Rc<RefCell<H>>`; later frames replace only the concrete `H` value, before child input reconciliation, without allocating another shared cell or resubscribing. Multiple typed listeners share that one subscription and closure, while a tag with no listeners creates no event handler cell or event subscription. This does not promise that arbitrary user handler expressions perform no allocations themselves.

Retained state also includes the concrete subscription-factory or event-handler type in GPUI's `TypeId` namespace. Repeated macro expansions that happen to carry the same source span and ordinal therefore remain distinct mounts without adding runtime storage to the plain host.

The subscription observes events emitted directly by that child entity; component events do not bubble through ancestor component tags. On first mount the child is constructed and its one-shot `setup` runs before the parent subscription is installed, so an event emitted during construction/setup may be missed by the declarative listener. This ordering is an explicit native-host divergence.

Visual lifecycle sections are statically dispatched and optional:

- `mounted(this, window, cx)` runs once after the first successful delegated draw, through `Window::defer` at the end of that GPUI effect cycle;
- `updated(this, window, cx)` runs after a later dirty delegated draw, coalescing dirty work already covered by a queued callback;
- `unmounted(this, cx)` runs at most once after a rendered keyed visual host disappears. It receives `App`, not a window-bound component context.

These names are Vue-shaped, but the timing is native GPUI timing rather than a DOM-paint contract. Descendant layout queues its deferred effect before its ancestor, so nested component `mounted` / `updated` callbacks run child before parent. Same-level `unmounted` order is not guaranteed. Removing with `v-if` or changing a component key removes a visual identity; intrinsic `v-show` only hides its existing element and does not unmount anything. `Component::new` / `new_with_slots` construct a naked entity outside the persistent visual host and therefore do not attach visual lifecycle state.

Visual teardown does not wait for an unrelated strong `Entity` clone: the host queues a weak-entity callback when an external owner remains, while entity release is the exactly-once fallback when the host held the last strong reference. During application shutdown a queued foreground callback is not guaranteed to be polled; release still covers the no-external-owner case, but code that intentionally holds an external entity through shutdown must not rely on `unmounted` as a process-finalization hook. Pending `mounted` / `updated` work is invalidated when the visual host disappears.

Provide/inject, DOM-compatible hook timing, and arbitrary Vue lifecycle APIs remain outside the current surface.

## State and memoization

For state owned by one component/entity, prefer `Local<T>`. It stores `T` inline, has no `Rc`, allocation, or dynamic borrow, and advances a compact `Revision` only for an effective change:

```rust,ignore
let mut count = Local::new(0);
count.set(1, cx);
count.update(|count| count + 1, cx);

let mut doubled = Memo::new();
let value = doubled.get_or_update(count.revision(), || count.get() * 2);
```

`Memo<T, D>` caches by an explicit typed dependency key, normally one `Revision` or a tuple. It is not an automatic Vue dependency graph; explicit revisions keep invalidation monomorphic and allocation-free.

`Ref<T>` is an `Rc<RefCell<T>>` handle for intentional shared clone semantics. Its `set` and `update` methods notify only the explicitly supplied `ChangeNotifier`, after the mutable borrow is released. A GPUI `Context` implements that trait.

`Ref<T>` does not collect dependencies. For state shared by multiple views, use a GPUI `Entity` with `observe`/`subscribe` so all readers are invalidated. `Ref::update` clones and compares `T` to suppress no-op redraws; `Local::update` derives a replacement through `&mut self` without cloning the old value.

## Why PocketJS is not embedded

[PocketJS](https://pocketjs.dev/docs/architecture/) is a useful reference for separating frontend ergonomics from native machinery and compiling utility classes. Its Rust core is also a complete retained UI engine with its own layout, text, animation, and draw list, while the wider stack supplies GPU backends. Embedding that stack in GPUI would create two UI trees and bypass GPUI's element, layout, focus, input, accessibility, and component ecosystem.

The selected design borrows the compile-away idea but targets GPUI builders directly. If exact, unmodified Vue SFC execution later becomes mandatory, it belongs in a separate optional QuickJS compatibility backend with explicit startup, memory, and maintenance costs.

See the [architecture decision](docs/architecture.md), detailed [capability matrix](docs/capability-matrix.md), and [counter example](examples/counter.rs).

## Quality gates

The workspace denies missing documentation for public and private items, forbids unsafe code, and denies Clippy's `all` and `pedantic` groups:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --no-default-features
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
cargo check --workspace --all-targets --features desktop
cargo clippy --workspace --all-targets --features desktop -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --no-default-features
```

The test suite includes compile-fail golden cases for unstable loops, structural-directive misuse, DOM-only modifiers, undocumented component declarations, duplicate component sections, and unsupported Tailwind utilities. The same locked commands run in GitHub Actions.

## Status

This repository contains a tested native compiler foundation, not a complete Vue 3 implementation. The largest remaining areas are external SFC files, arbitrary `<script setup>`, provide/inject, broader native event/modifier semantics, automatic dependency-tracked computed/watchers, transitions, full accessibility semantics, stacked/responsive/data Tailwind variants, and the remaining Tailwind vocabulary.
