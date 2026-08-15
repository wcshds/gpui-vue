# Vue 3 / Tailwind CSS / GPUI capability matrix

This document is a snapshot of the repository's **implemented behavior**, not
a promise of source compatibility. It uses Vue 3 and Tailwind CSS 4.3.2 as the
authoring-language targets and pins the native host to a compatible GPUI-CE
commit. `gpui-vue` does not depend on, invoke, or bundle Vue or Tailwind.

The status labels have deliberately narrow meanings:

- **Implemented** — present in the current source and exercised by a test or
  example. It does not imply complete Vue/Tailwind compatibility.
- **Partial** — a useful subset exists, with the missing behavior stated in the
  same row.
- **Not implemented** — absent from the current source and test suite.
- **Host-different** — DOM/CSS behavior has no direct GPUI equivalent. An
  analogous native behavior must not claim identical semantics.
- **Not targeted** — intentionally excluded from the native hot path.

The source of truth for an **Implemented** or **Partial** label is the code in
`crates/gpui-vue-macros/src`, `crates/gpui-vue/src/component.rs`,
`crates/gpui-vue/src/slot.rs`, `crates/gpui-vue/src/local.rs`,
`crates/gpui-vue/src/reactivity.rs`, `effects.rs`, `async_state.rs`,
`overlay.rs`, the curated `ui`, `paint`, `http`, `animation`, `media`,
`virtual_list`, and `desktop` bridge modules, and the repository tests. When
code and this document disagree, the code wins and this matrix must be
corrected.

## Decision summary

The selected architecture is a Rust compiler frontend:

```text
Vue-inspired Rust syntax + literal utilities
                    │ proc-macro expansion
                    ▼
       ordinary typed GPUI element builders
                    │
                    ▼
       GPUI layout, input, accessibility, paint
```

PocketJS is a valuable reference for a small mutation surface and compile-away
utility syntax, but embedding its runtime is not the default design. Doing so
would place PocketJS's retained UI/layout/draw machinery beside GPUI's own
element, layout, input, focus, accessibility, and rendering machinery. Running
unmodified Vue would additionally require a JavaScript engine, bindings, and a
DOM-shaped compatibility layer. The duplicate trees and cross-runtime
synchronization work against the reason for choosing GPUI.

Therefore:

- the default backend lowers directly to the pinned GPUI-compatible native host;
- PocketJS ideas may influence the compiler IR, but PocketJS is not a runtime
  dependency;
- exact JavaScript SFC execution, if ever required, belongs in a separate,
  feature-gated compatibility backend with explicit performance costs;
- Vue Vapor is a design reference for compile-time lowering, not a claim that
  this project implements Vue's Vapor runtime. Vue itself describes Vapor as an
  opt-in SFC compilation mode with a deliberately smaller API surface in the
  [Vue 3.6 release notes](https://github.com/vuejs/core/releases).

See the longer rationale in [architecture.md](architecture.md) and PocketJS's
[architecture overview](https://pocketjs.dev/docs/architecture/).

## Vue SFC and template alignment

Vue references: [SFC specification](https://vuejs.org/api/sfc-spec),
[`<script setup>`](https://vuejs.org/api/sfc-script-setup.html),
[template syntax](https://vuejs.org/guide/essentials/template-syntax.html),
[conditional rendering](https://vuejs.org/guide/essentials/conditional.html),
[list rendering](https://vuejs.org/guide/essentials/list.html), and
[event handling](https://vuejs.org/guide/essentials/event-handling.html), plus
[slots](https://vuejs.org/guide/components/slots.html).

| Area | Status | Current contract and gap |
| --- | --- | --- |
| File format | Not implemented | There is no `.vue` parser and no file-level `<template>`, `<script setup>`, or `<style>` block. Authoring happens inside Rust's `view!` and `component!` proc macros; a component `template(...)` may contain direct markup, but it is still a Rust DSL. The current design does not include an SFC-like frontend or another runtime. |
| Lexical bindings | Partial | `{ expression }` evaluates a statically typed Rust expression in the caller's lexical scope. It is analogous to top-level bindings being visible to a Vue template, but the grammar and value semantics are Rust, not JavaScript. String children are Rust literals; Vue-style double-brace interpolation is not accepted. |
| Intrinsic elements | Partial | `div`, `view`, `text`, `span`, `button`, and `img` parse today. The container-like tags lower to `gpui::div()`; `button` adds pointer/focus/tab-stop/click conveniences but is not yet a complete semantic or accessibility button. `img` lowers through the curated native image constructor, requires exactly one static `src` or dynamic `:src` (including same-name shorthand), rejects children, and accepts typed `:object-fit={ObjectFit}`, `:loading={Fn() -> AnyElement}`, and `:fallback={Fn() -> AnyElement}` bindings. Their values evaluate once in attribute source order, while replacement callbacks remain lazy. Real intrinsic hosts expose the identity, style, structural, focus/key-context, and native event subset described below. Semantic alt text is not implemented. Unknown tags are compile errors. |
| Custom components in `view!` | Partial | A typed native element/entity—including output from the curated `ui` or `paint` bridge—can be inserted with `{ component_expression }`; raw GPUI imports remain available for interoperability but are not required by those supported seams. A simple PascalCase identifier selects the generated native-component lane. Tags accept individual props or the complete-value `:props={ChildProps::new(...)}` escape hatch, typed `@event` / `on:event` listeners, optional `:slots={ChildSlots}`, `key` / `:key`, conditional chains, and keyed `v-for`. They may be non-self-closing: ordinary children provide `default`, and direct `<template #name={RustPattern}>` children provide named/scoped slots. `:slots` and declarative children are mutually exclusive. Paths/generic tags, class/ID/focus attrs, and `v-show` remain unsupported. |
| `component!` declaration | Partial | The item macro emits ordinary Rust props/builder/input/component/event/slot structs, generic `AppContext` constructors returning that context's `Result<Entity<Component>>`, native `NativeComponent`/`NativeComponentEvents`/`Render`/`EventEmitter` implementations, and optional monomorphized `ComponentLifecycleHooks`. `NativeComponent::Props` / `Input`, `NativeComponentSlots::Slots`, and `NativeComponentEvents::Event` let aliased or module-qualified child types resolve generated siblings hygienically. Declarations require documentation on the component and declared props, state, events, and slots. This is a typed construction DSL over GPUI ownership, not a second component runtime. |
| Component props | Partial | Required props remain exact-typed `Props::new(...)` parameters; defaulted props receive their declaration initializer and fluent `with_<name>` overrides, and `Default` exists only when every prop is defaulted. `Props::builder()` stores required values inline behind sealed `RequiredProp<T, State>` typestate (`PropMissing` or `PropSet`), with `build()` implemented only for the all-set state; it supports move-only values and uses no dynamic map or boxed builder storage. PascalCase individual attrs chain these exact setters, while `:props={...}` passes a complete value; mixing the modes or duplicate normalized names is a macro error, while missing/unknown/wrong-typed props remain rustc diagnostics. Generated props derive `PartialEq`, requiring every field to implement it. A persistent parent host compares and replaces props each render, notifying the child only on change. Fallthrough attrs, validators, `defineModel`, and a runtime defaults protocol remain absent. |
| Component state and setup | Partial | Typed state fields, including explicitly declared `Local<T>`, run their initializer once when a native mount constructs the entity; initializers can read props and the construction context. Optional `setup(this, props, cx)` runs once against a typed mutable state draft. Parent input reconciliation preserves this state and does not rerun setup. Optional visual `mounted`, `updated`, and `unmounted` sections are implemented with the native timing described below; Vue dependency injection and other lifecycle APIs remain absent. |
| Component template | Partial | `template(this, window, cx) { ... }` becomes the component's native `Render::render` body and runs whenever GPUI renders that entity. Its body may be direct Vue-shaped markup, the compatible explicit `view!` form, or any Rust block returning `IntoElement`. Direct markup supports intrinsic structure, PascalCase children, contextual declarative providers, and typed child `<slot>` outlets. Expressions and scoped-slot patterns are Rust, not JavaScript; this is not a `.vue` SFC or `<script setup>` implementation. |
| Persistent component host | Partial | Every generated component exposes associated `Props`/`Input` types through `NativeComponent`; slotted/emitting components expose associated `Slots`/`Event` types. `ComponentElement` nests an optional user key under a stable compile-site ID, and GPUI `Window::with_element_state` retains the child `Entity` across consecutive frames with no parallel tree. A transparent `HostedEntity` delegates the entity's request-layout, prepaint, and paint phases without adding a layout node; without hooks its render token is zero-sized and the adapter has `Entity` size. Plain mount state includes the subscription-factory closure type as a zero-sized identity marker; event mount state includes concrete event/handler types. Because GPUI also keys state by `TypeId`, repeated macro sites with the same source span/ordinal remain distinct. A changed key selects a new mount. |
| Root fragments | Partial | Multiple roots and explicit `<>...</>` roots are accepted. GPUI 0.2.2 still requires one `IntoElement` at the render boundary and has no `display: contents`, so these forms lower into one synthetic `div`. This is authoring-syntax alignment, not wrapper-free Vue fragment semantics; the boundary can participate in layout. |
| Nested fragments | Implemented | Nested `<>...</>` fragments are recursively flattened into their parent without adding an element. Only a fragment/multi-node render boundary needs the synthetic container described above. |
| Structural `<template>` | Partial | `<template>` flattens its children and supports structural conditional chains without producing a child element. It rejects ordinary attributes and `v-show`, because there is no rendered host to style. `<template v-for>` is an explicit compile error: GPUI 0.2.2 cannot assign identity to a flattened repeated fragment without inserting a layout-affecting host; put the loop and `:key` on a real child. |
| Static attributes | Partial | Intrinsics recognize `class`, `id`, `key`, `focusable`, `tab-index`/`tab_index`, a static string `key-context="Editor"`, and bare `occlude`. The latter lowers to GPUI's blocking pointer hitbox and requires stable identity; it does not create keyboard modality. `<img>` additionally accepts static `src`. On PascalCase tags, an ordinary `foo={expr}` passes the Rust expression, a bare `enabled` passes `true`, and `label="literal"` passes a literal `&str` without implicit `String` allocation or conversion. Kebab-case names normalize to generated snake-case setters. Host `key`, `:props`, and `:slots` remain reserved bindings; components deliberately do not inherit intrinsic attrs. There is no general HTML attribute bag, fallthrough, boolean-attribute coercion, ARIA mapping, or `v-bind="object"`. |
| Dynamic binding | Partial | Intrinsic `:id={rust_expression}` and `:key={rust_expression}` are implemented, including Vue 3.4-style same-name shorthand. `<img>` accepts the same explicit/shorthand `:src`, typed `:object-fit={ObjectFit}`, and lazy `:loading` / `:fallback` functions returning `AnyElement`; all four media values evaluate once in attribute source order, callbacks remain lazy, and pinned GPUI does not require image identity. DOM/CSS strings are not accepted for those typed policies. `:track-focus={&focus_handle}` and `:key-context={rust_expression}` configure the native focus handle and keyboard context; the static/dynamic context forms share one duplicate slot. Intrinsic `:style={refiner_expression}` accepts one typed runtime refinement callback as described below. Typed drag/drop adds paired single `:drag-payload` / `:drag-preview`, one type-erased `:can-drop`, and repeatable exact-payload-type `:drag-over` lanes; every expression is evaluated once in attribute source order and every participating host requires stable identity. PascalCase `:foo={rust_expression}` and `:foo` lower to the exact generated prop setter, with the shorthand reading `foo`; kebab-case normalizes to the snake-case Rust binding/setter. Tags may instead use complete `:props={complete_props}`, and may pass `:slots={typed_slots}` plus `:key={identity}`. Mixing complete and individual props is rejected. Dynamic arguments, `.prop`/`.attr`, and object spread are not implemented. |
| `v-if` | Implemented | Intrinsics, PascalCase components, and structural `<template>` nodes accept `v-if={bool_expression}` and lower to GPUI conditional builders. Root `v-if` is supported through the synthetic root boundary. The expression must type-check as Rust `bool`; JavaScript truthiness is not emulated. |
| `v-else-if` / `v-else` | Implemented | Adjacent sibling chains, including component branches, lower to nested GPUI `when_else` builders. Orphan `v-else-if`/`v-else`, non-adjacent branches, multiple conditionals on one element, a valued `v-else`, and a non-final `v-else` are compile errors. |
| `v-show` | Partial | A real intrinsic accepts `v-show={bool_expression}` and is always emitted, with GPUI `hidden()` applied when false. It never removes a keyed visual component identity and therefore does not cause `unmounted`. GPUI still rebuilds the entity's immediate element tree after notification rather than retaining a DOM node. Structural `<template>` has no host element, and PascalCase component hosts add no layout wrapper, so both reject `v-show`. |
| `v-text` | Implemented / Host-different | A parent intrinsic (`div`, `view`, `text`, `span`, or `button`) accepts `v-text={rust_expression}`. The expression is evaluated exactly once per render, bound to a local, and appended through native GPUI `.child(value)` semantics; it is therefore typed Rust/`IntoElement` content, not an HTML string or escaping/parser operation. It is mutually exclusive with every literal, expression, fragment, or element child. `img`, structural `template`, slot outlets, and PascalCase components are targeted compile errors. Structural directives and keyed loop identity continue through their existing lanes. Native builder ordering precedes the source-ordered interaction/content lane, so no cross-lane HTML attribute evaluation order is claimed. |
| `v-for` | Partial | A real intrinsic or PascalCase component accepts `v-for={pattern in rust_iterator}`; a top-level loop uses the synthetic render boundary. Every loop root requires a non-literal dynamic `:key`. Rust patterns and `IntoIterator` replace Vue's JavaScript alias/source grammar. `of`, JavaScript object enumeration semantics, and `<template v-for>` are not implemented; Rust ranges work only through normal `IntoIterator` behavior. |
| `v-if` with `v-for` | Implemented | `v-if` wraps the loop, so the loop alias is unavailable in the condition. This matches current Vue 3 directive precedence, although Vue recommends avoiding both directives on one node. |
| List keys | Partial | Every `v-for` root must use a non-literal dynamic `:key`. Vue recommends a key where possible and can patch unkeyed lists in place; this compiler intentionally rejects unkeyed loops to protect GPUI stateful descendant identity. Rust tuples may be useful GPUI IDs, so accepted key types are not restricted to Vue's primitive key types. |
| Events | Partial | Intrinsic `@click`, `@key-down`, `@key-up`, `@modifiers-changed`, `@mouse-down`, `@mouse-down-out`, `@mouse-move`, `@drag-move`, `@mouse-up`, `@mouse-up-out`, `@pinch`, `@scroll-wheel`, `@hover`, `@drop`, `@focus`, and `@blur` (plus canonical `on:` aliases) use pinned GPUI's typed native paths. `@drag-move` receives `&DragMoveEvent<T>` and `@drop` receives `&T`; both permit repeated lanes selected by exact payload `TypeId`, including `ExternalPaths`. A drag source pairs one `'static` payload with a preview constructor returning `Entity<Preview: Render>`; one `:can-drop` predicate gates matching drag-over style and drop dispatch. `@hover` receives `&bool`, true on entry and false on exit. Mouse down/down-out/up/up-out require one `.left`, `.right`, or `.middle` selector; different buttons may coexist, while the same event/button pair is a duplicate. GPUI exposes outside mouse-down as an any-button capture listener, so gpui-vue's typed wrapper filters `MouseDownEvent.button` before invoking the selected handler. `@focus` / `@blur` receive `&mut Window` and `&mut App`, require `:track-focus={&focus_handle}`, and retain exact `Context::on_focus` / `Context::on_blur` subscriptions with the stable element identity; they do not claim descendant focus-in/out semantics. A changed explicit handle is reconciled. `@pinch` receives native `PinchEvent` input on macOS and Wayland; Windows precision-trackpad pinch follows the Ctrl-wheel path handled through `@scroll-wheel`. Every listener, drag/drop binding, tracked focus, and key context requires stable `id` or loop key. Lowering orders identity before focus/context, then evaluates listeners and drag interactions in source order. A PascalCase child with declared emits accepts typed `@change={handler}` or `on:change={handler}`; its callback receives the complete `&<Component>Event`, `&mut Window`, and `&mut App`, observes only the direct child entity, and does not bubble. Text input/IME is supplied by the separate retained `TextInput`; native drag/drop deliberately does not emulate DOM `DataTransfer` or string MIME negotiation. |
| Event modifiers | Partial | Intrinsic click supports `.stop`, `.prevent`, `.ctrl`, `.alt`, `.shift`, `.meta`, and `.exact` in source order through one generated allocation-free wrapper; `.passive` and the other unsupported click modifiers are rejected. The mouse button selectors are mandatory routing arguments rather than common event modifiers. Key-down/up, modifiers-changed, mouse-move, drag-move, pinch, scroll-wheel, hover, drop, focus, and blur accept no modifiers. Every PascalCase component-event modifier is rejected, including `.stop` and `.once`. Canonical intrinsic/component duplicates and duplicate mouse event/button pairs are macro errors; typed drag-move/drop lanes are intentionally repeatable. Unknown event names and wrong callback types are ordinary rustc errors. No listener fallthrough, DOM capture/passive registration, `DataTransfer`, or Vue `once` behavior is claimed. |
| Static and conditional class | Partial | `class="literal"`, `:class={"literal"}`, and nested Rust `if condition { "literal" } else { ... }` trees are compiled. Each Rust condition is emitted once and evaluated once when its branch is reached during a render. A static `class` is prepended to every dynamic literal leaf (and becomes the fallback when `else` is omitted), so regular and state refinements are resolved together and applied by one selected conditional traversal instead of duplicate static/dynamic state-callback paths. Any stateful branch participates in ID/focus validation. Arbitrary runtime strings and Vue object/array class bindings are rejected, keeping this narrower than Vue's [class binding](https://vuejs.org/guide/essentials/class-and-style.html). |
| Inline style | Partial / Host-different | An intrinsic `:style={refiner_expression}` accepts a Rust `FnOnce(StyleRefinement) -> StyleRefinement`. The expression is evaluated once per render, receives a fresh typed refinement, and is merged after the selected regular `class` / `:class` styles. GPUI interaction-state callbacks refine the result later in native state order. Passing a refinement rather than the host element deliberately excludes stateful-only methods such as scroll overflow, which continue to use ID-validated static classes. Duplicate bindings and string literals are macro errors; arbitrary CSS strings/maps, arrays, browser cascade priority, component fallthrough style, and Vue's `:style` object/array normalization are not implemented. |
| Remaining built-in directives | Not implemented / Host-different | The `v-model` template attribute, custom directives, and component directives are not part of the current template compiler. Native entity handles already cover typed refs, while `TextModelBinding` supplies first-class controlled two-way `TextInput` synchronization without claiming the missing syntax or a general custom-component model convention. `v-html` is not targeted for intrinsic GPUI elements because there is no browser HTML parser/DOM insertion target. |
| Emits | Partial | An `emits` section declares documented unit or named-field payload events. It generates a typed `<Component>Event` enum, a native `EventEmitter` marker, `NativeComponentEvents<Event = ...>`, monomorphic `emit_*` helpers, and hidden typed variant dispatchers. Multiple PascalCase listeners are evaluated once each per parent render and captured by one concrete closure sharing one native subscription. The first mount allocates one `Rc<RefCell<H>>`; later same-identity frames replace `H` before input reconciliation without a new cell or resubscription. With no listeners the plain host creates neither. Because construction/setup precedes parent subscription installation, an event emitted there may be missed. Direct-child delivery, the complete enum argument, no modifiers, and no listener fallthrough differ from Vue. |
| Component slots | Partial | A documented `slots { name: Props; }` section generates a typed `<Component>Slots` value whose public `Slot<Props>` fields default to empty, plus `Slots::new()` and fluent `with_<name>` providers. `Component::new(props, cx)` supplies empty slots; components declaring slots also expose `new_with_slots(props, slots, cx)`. The explicit Rust `render` / `render_or_else` API remains available. In direct component markup, `<slot />` handles syntactic `()` props, while `<slot name="actions" :props={expr}>fallback</slot>` provides a static named/scoped outlet with lazy fallback. A non-self-closing PascalCase parent maps ordinary children to `default` and direct `<template #name={RustPattern}>` children to named providers; `:slots={...}` is the mutually exclusive complete-value escape hatch. Slot-bearing input is replaced and conservatively lifecycle-dirty on every parent reconciliation because opaque providers are not comparable, but slot-only replacement emits no extra child notification. Dynamic/unknown names, missing/wrong non-unit props, directives on outlets, duplicate providers, and a second outlet for one declared slot are compile errors. The repeated-outlet rejection is conservative even across mutually exclusive branches. `v-slot`'s full shorthand/dynamic/modifier surface is not implemented. |
| Slot provider and render boundary | Partial / Host-different | Every non-empty `Slot<Props>` stores one boxed lazy `'static` closure receiving `Props`, `&mut Window`, and `&mut App`; the declarative value is rebuilt/replaced on each parent render. Invoking it erases one concrete `IntoElement` into `SlotContent`, exactly one GPUI [`AnyElement`](https://docs.rs/gpui/0.2.2/gpui/struct.AnyElement.html), with no VNode/list wrapper. Direct component markup captures a parent `WeakEntity` and uses a live `update` at invocation, so provider content may read/mutate current parent state and create `cx` listeners without a strong cycle. If that owner is gone it yields `Empty`, not the outlet fallback, because a provider was present. Standalone `view!` providers instead require ordinary owned-`'static` captures. A nested missing/no-fallback outlet contributes zero children; only a sole render-root absence becomes `Empty`. Multi-root render/provider bodies use a synthetic root container because GPUI 0.2.2 has no wrapper-free slot-fragment equivalent. |
| Visual lifecycle | Partial / Host-different | Optional `mounted(this, window, cx)` and `updated(this, window, cx)` are queued with `Window::defer` after the relevant delegated draw and run at the end of that GPUI effect cycle, not at a DOM-paint boundary. Nested mount/update order is child before parent; dirty draws covered by an already queued hook are coalesced. `unmounted(this, cx)` runs at most once after a rendered keyed visual host disappears, invalidating pending hooks first; same-level teardown order is not guaranteed. `v-show` does not unmount, and naked `Component::new` / `new_with_slots` entities have no visual lifecycle. A weak task handles visual removal while an external owner keeps the entity alive, with entity release as the no-external-owner fallback. At application shutdown that task may not poll, so an entity deliberately held externally through shutdown must not use this hook for process cleanup. Hooks are monomorphized; a hook-free component uses unit mount state and registers nothing. Other Vue hooks and DOM-equivalent timing are absent. |
| Provide / inject | Not implemented | No dependency-injection layer exists. A convenience API must preserve GPUI `Entity`, `Context`, subscription, and task ownership rather than maintain a parallel component runtime. |
| Transitions and built-ins | Partial / Host-different | First-class native primitives now include keyed `Animation` timelines, `anchored_overlay` for window-aware placement, `deferred_overlay` for later paint, and owner-held `AsyncResource` for cancellable loading state. They do not implement Vue `Transition`, `TransitionGroup`, `KeepAlive`, `Teleport`, or descendant-aggregating `Suspense`: overlays stay in the same owner/element tree and there is no root registry or cross-tree target, while one resource does not discover async descendants or load a component factory. |
| SSR / hydration / DOM refs | Not targeted | The selected renderer is a native GPUI tree, not HTML. Browser SSR, DOM hydration, CSS selectors, and `HTMLElement` refs are outside this backend. |

## Curated native application bridges

The macro frontend remains the preferred authoring layer. These bridge modules
cover application boundaries that cannot be expressed as static markup without
creating another element tree or hiding dynamic native work.

| Bridge | Status | Current contract and gap |
| --- | --- | --- |
| `text_input` | Implemented | `TextInputHandle` retains a native single-line control that registers GPUI's platform input handler during paint. It converts platform UTF-16 ranges to boundary-safe UTF-8 ranges, supports marked composition, selection, grapheme-aware editing, clipboard shortcuts, horizontal caret scrolling, candidate-window geometry, and typed change/submit/escape events. `TextInputConfig` / `TextInputStyle` cover initial value, native dimensions, padding, colors, border, font, disabled/read-only policy, and a Unicode-grapheme limit for later user commits that permits longer intermediate composition without rewriting parent-controlled values. `TextModelBinding` owns controlled parent/`Local<String>`/`Ref<String>` synchronization; equal silent reconciliation preserves IME marked state. Multiline editing, secure entry, validation, accessibility metadata, and `v-model` template lowering remain separate work. |
| `ui` | Implemented | Re-exports a deliberately small set of common native element, event, context, focus, string, pixel, `Font`, `Hsla`, `Rgba`, and `StyleRefinement` types, plus `div`, `image`, `px`, `rgb`, `rgba`, and `hsla`. `write_clipboard_text` and `read_clipboard_text` expose plain-text clipboard semantics while keeping GPUI's multi-entry `ClipboardItem` representation behind the bridge. `ScreenPoint` avoids colliding with domain point types. It is not a complete GPUI wrapper; ordinary layout and controls should still prefer `view!`. |
| `paint` | Implemented | Exposes the native geometry, masks, paths, fills, shadows, and window/app types needed by precision visuals. `drawing_surface` retains the typed result of one prepaint closure and passes it to that frame's paint closure on the same native canvas. This is a low-level custom-paint seam, not a second renderer. |
| `http` | Implemented | Re-exports the native async body, request, response, URL, error, and `HttpClient` contract needed by API and image transports. The application still supplies the concrete transport and owns its network policy. |
| `animation` | Implemented | Re-exports the native `Animation`, `AnimationElement`, `AnimationExt`, and bounded easing functions used by retained elements. This is a keyed native timeline, not CSS transitions or Vue transition lifecycle coordination. |
| `media` | Implemented | Provides raster/animated `image`, asset-backed `svg_asset`, filesystem `external_svg`, and the native image/object-fit/transformation types. Remote sources still require an installed HTTP client and embedded paths require an asset source. |
| `virtual_list` | Implemented | Re-exports explicit uniform-row and variable-row virtualization, retained list state, measurement, alignment, offsets, scrolling, and strategy types. Ordinary `v-for` is not silently virtualized because row geometry and mount range are semantic choices. |
| `effects` / `async_state` | Implemented | `spawn` / `spawn_in` provide weak-owner foreground tasks; dropping the returned `Task` cancels work. `AsyncResource` owns one task, exposes idle/loading/ready/error state, cancels replacement requests, and rejects stale completions by generation. It does not aggregate descendant dependencies or provide an async component factory. |
| `overlay` | Implemented / Host-different | `anchored_overlay` configures corner, window/local position, offset, and overflow fitting; `deferred_overlay` changes paint order by priority. Both preserve the original owner, element tree, events, focus, and lifecycle, so they are neither Vue Teleport nor a global/root overlay registry. |
| `desktop` | Implemented / Feature-gated | With the `desktop` feature, `WindowConfig` configures title, centered/minimum size, titlebar and traffic lights, initial focus/visibility, window kind, move/resize/minimize policy, background appearance, app id, and tabbing identifier. `DesktopApp` installs assets/HTTP, setup callbacks or `AppPlugin`s, quit policy, open-URL/reopen callbacks, and runtime URL-scheme requests. It mounts a raw or lifecycle-aware initial root; `open_window` and `open_component_window` return fallible handles for additional native windows. Delivery, registration, reopen triggers, kind, and material remain platform-defined. |

KAGE Editor exercises five of these bridges: `component!` retains its root state,
`view!` authors the workspace, canvas input host, and reusable controls,
`text_input::TextInput` provides native IME-aware GlyphWiki queries,
`paint::drawing_surface` owns precision glyph drawing, `http` carries GlyphWiki
transport, and `desktop` owns launch. Its independent manifest has no direct
`gpui` dependency, and its source imports neither `gpui` nor `gpui_vue::gpui`.
On macOS its canvas consumes native trackpad pinch events and adjusts pan while
zooming so the gesture position remains anchored.

## Reactivity alignment

Vue's baseline is documented in the
[reactivity fundamentals](https://vuejs.org/guide/essentials/reactivity-fundamentals.html)
and [reactivity core API](https://vuejs.org/api/reactivity-core.html).

| Area | Status | Current contract and gap |
| --- | --- | --- |
| `Ref<T>` handle | Partial | `Ref<T>` is `Rc<RefCell<T>>` with shared clone semantics. `get`, `read`, `set`, `update`, and pointer equality are implemented. It is single-threaded and Rust-typed; `.value` auto-unwrapping is not emulated. |
| Change notification | Partial | `set` and `update` notify only the explicitly supplied `ChangeNotifier` after releasing the mutable borrow and suppress equality no-ops. A GPUI `Context` implements that trait. There is no dependency collection. |
| `Local<T>` state | Implemented | Component/entity-local state can be stored inline without allocation, shared ownership, or interior mutability. Mutation requires `&mut self`; `set` and replacement-style `update` suppress equal values, advance a revision, then notify the supplied `ChangeNotifier`. This is the preferred hot-path primitive when state is not shared. |
| `Revision` | Implemented | Each effective `Local` mutation advances a typed `u64` version token with deliberate wrapping arithmetic. Revisions expose `ZERO`, `MAX`, raw conversion, and ordering/hash traits and can be combined in tuples as memo dependency keys. |
| `Memo<T, D>` | Partial | `Memo` lazily stores one value and typed dependency key, reusing the result while `D: PartialEq` is unchanged. It supports explicit invalidation and tuples of `Revision`s, and itself allocates only if `T` or `D` does. Dependencies are supplied explicitly; this is not Vue's automatically tracked `computed`. |
| Render granularity | Host-different | Notification invalidates the GPUI entity, whose `Render` method rebuilds its immediate element tree. There are no per-text-node Vapor effects or a framework-owned VNode patcher. |
| `reactive`, shallow/readonly forms | Not implemented | Object proxying, `reactive`, `shallowRef`, `readonly`, and collection instrumentation are absent. The current Rust APIs make ownership and mutability explicit instead of imitating JavaScript Proxy behavior. |
| `computed` | Partial | `Memo<T, D>` provides a typed, explicit dependency cache suitable for computed values. Automatic dependency collection, chained invalidation, cycles, effect disposal, and Vue-compatible computed refs are not implemented. |
| `watch` / effects | Partial | `EffectScope` owns and cancels subscriptions; `watch_entity`, `watch_event` and their window-aware variants wrap GPUI observers/subscribers, while `next_frame`, `defer`, and `on_release` expose native scheduling and teardown. Automatic dependency collection, Vue-compatible `watchEffect`, cleanup callbacks, and flush timing modes are not implemented. |
| Async work | Partial / Host-different | `spawn` / `spawn_in` run owner-safe native futures with weak entity access, and `AsyncResource` owns cancellation, state notification, and stale-result protection for one request. GPUI remains the executor. There is no automatic dependency collection, descendant `<Suspense>` boundary, async component factory, delay/timeout policy, or code-splitting runtime. |
| Shared state | Host-different | A cloned `Ref` shares storage but a mutation notifies only the passed context. State shared by multiple views should currently live in a GPUI `Entity` observed/subscribed by each reader. |

## Tailwind CSS 4.3.2 alignment

The version number here names the desired vocabulary/behavior baseline; no
Tailwind package or CSS engine is installed. Tailwind's official documentation
is rolling, so exact parity must be locked by fixture tests before any broad
family is marked implemented. Relevant primary references are the
[v4.3 announcement](https://tailwindcss.com/blog/tailwindcss-v4-3),
[utility-first model](https://tailwindcss.com/docs/styling-with-utility-classes),
[state variants](https://tailwindcss.com/docs/hover-focus-and-other-states),
[responsive variants](https://tailwindcss.com/docs/responsive-design),
[arbitrary values and properties](https://tailwindcss.com/docs/adding-custom-styles),
[class detection](https://tailwindcss.com/docs/detecting-classes-in-source-files),
[colors](https://tailwindcss.com/docs/colors),
[opacity](https://tailwindcss.com/docs/opacity),
[grid templates](https://tailwindcss.com/docs/grid-template-columns),
[grid column placement](https://tailwindcss.com/docs/grid-column),
[grid row placement](https://tailwindcss.com/docs/grid-row),
[aspect ratio](https://tailwindcss.com/docs/aspect-ratio),
[line height](https://tailwindcss.com/docs/line-height),
[border radius](https://tailwindcss.com/docs/border-radius),
[align items](https://tailwindcss.com/docs/align-items),
[align self](https://tailwindcss.com/docs/align-self),
[align content](https://tailwindcss.com/docs/align-content),
[justify content](https://tailwindcss.com/docs/justify-content),
[place content](https://tailwindcss.com/docs/place-content), and
[overflow](https://tailwindcss.com/docs/overflow).

The checked-in default palette covers all 26 Tailwind 4.3.2 families — Red,
Orange, Amber, Yellow, Lime, Green, Emerald, Teal, Cyan, Sky, Blue, Indigo,
Violet, Purple, Fuchsia, Pink, Rose, Slate, Gray, Zinc, Neutral, Stone, Mauve,
Olive, Mist, and Taupe — at `50`, every hundred from `100` through `900`, and
`950` (286 named shades), plus black, white, and transparent. Coverage here
means those names are accepted by the implemented background, text, and border
color consumers; it does not imply a general CSS color system.

| Area | Status | Current contract and gap |
| --- | --- | --- |
| Class detection | Partial | A literal `class="..."` and the literal leaves of nested `:class={if ...}` branches are compiled. There is no project source scanner, safelist, `@source`, CSS input, or runtime parser. Conditions may be dynamic Rust booleans, but every possible utility list remains statically enumerable. |
| Lowering and cascade model | Implemented | Supported candidates expand into typed property-slot assignments and then direct GPUI builder calls. Shorthands are decomposed into the fields or sides they affect, and regular/in-focus/hover/group-hover/active/group-active/focus/focus-visible states own separate cascades. Shared-property state pairs are accepted only when Tailwind 4.3.2 and GPUI 0.2.2 choose the same simultaneous winner; mismatches are compile errors at the class literal. Unsupported utilities or variants are likewise compile errors. No stylesheet, selector matcher, CSS cascade, or class-name lookup ships at runtime. |
| Compound field cascade | Implemented | The supported `flex-1`/`flex-auto`/`flex-initial`/`flex-none` shorthands write independent grow, shrink, and basis slots; `truncate` and broad hidden/scroll overflow write independent axes; `place-content-*` writes independent align-content and justify-content slots. Supported longhands and `!` therefore resolve per field using Tailwind 4.3.2 canonical order rather than letting one GPUI convenience call reset an unrelated winner. This claim is limited to these implemented compounds. |
| Display and layout | Partial | Exact display modes `block`, `flex`, `grid`, and `hidden`, visibility forms, flex direction/wrap/grow/shrink, selected basis, uniform grid tracks and placement, bounded aspect ratios, relative/absolute positioning, and numeric/relative/auto inset families are supported. Inline, flow-root, contents, table, list-item, and other display modes have no exact GPUI 0.2.2 counterpart and receive host-specific errors. Non-uniform grid templates and most modern layout utilities remain absent. |
| Alignment and content distribution | Partial | `items-start/end/center/baseline/stretch`; `content-center/start/end/between/around/evenly/stretch`; `justify-start/end/center/between/around/evenly/stretch`; `self-start/end/center/baseline/stretch`; and `place-content-{around,between,center,end,evenly,start,stretch}` map directly to GPUI's public alignment enums. Static `content-normal` uses GPUI's native `Styled::content_normal()` reset and joins the ordinary canonical/important cascade; a state-prefixed `content-normal` is a targeted error because `None` inside a state refinement cannot clear an already-resolved base value. Flex-relative start/end remain distinct from the absolute start/end used by `place-content-*`. The place-content compound splits into two slots, later longhands win only their field, and Tailwind 4.3.2 canonical order plus `!` is source-order independent. Start/end are exact for GPUI/Taffy's exposed axes, not a browser writing-mode or direction guarantee. Other reset/inheritance (`justify-normal`, `self-auto`), safe-overflow, last-baseline, unsupported baseline distribution, and `order-*` are targeted errors because GPUI cannot represent the same semantics. |
| Overflow | Partial / Host-different | Hidden overflow is an ordinary per-axis style refinement. `overflow-clip` / `overflow-visible` and both axis forms write GPUI's exact non-retained `Overflow::Clip` / `Visible` enum, need no retained scroll state, and may be used in supported interaction variants. Clip excludes overflowing content from its parent's scroll region; visible preserves that contribution. Static `overflow-scroll`, `overflow-x-scroll`, and `overflow-y-scroll` instead lower to retained-state methods, require a stable `id` or loop key, and are emitted only after identity; a scroll utility under any interaction variant is rejected because a style callback cannot introduce retained state. Broad/axis candidates cascade independently in Tailwind canonical order. This native path provides clipping/layout and wheel scrolling, not browser UA scrollbar chrome or browser touch, keyboard, and accessibility parity. `overflow-auto` remains a targeted host error. |
| Uniform grid tracks | Partial | `grid-cols-N` and `grid-rows-N` accept decimal counts from `1` through `65535` and lower to GPUI 0.2.2's `u16` uniform column/row count. Numeric candidates use canonical count order independent of token order, and `!` wins per axis. `grid-cols-none`, `grid-rows-none`, zero/out-of-range counts, arbitrary track lists, subgrid, and CSS grid-template grammar are not representable by this mapping and remain unsupported. |
| Grid placement | Partial | `col-auto`/`row-auto`, `col-span-N`/`row-span-N` (`1..=65535`), `*-span-full`, and column/row `start`/`end` with `auto` or explicit lines are implemented. Positive lines are `1..=32767`; the leading-negative forms such as `-col-start-N` cover `-1..=-32768`. Shorthands split into four independently cascading endpoints and lower exactly to GPUI [`GridPlacement::Auto`, `Line(i16)`, or `Span(u16)`](https://docs.rs/gpui/0.2.2/gpui/enum.GridPlacement.html); full span is line `1` through line `-1`. Canonical order and `!` apply per endpoint. Zero, out-of-host-range, arbitrary, and custom-property placements are rejected. |
| Aspect ratio | Partial / Host-different | `aspect-square`, `aspect-video`, and positive finite `aspect-N/D` or `aspect-[N/D]` fractions set GPUI's width/height `f32` aspect-ratio field; canonical conflict order and `!` are implemented for this subset. `aspect-auto` is a dedicated compile error because GPUI 0.2.2's optional style refinement cannot reliably clear an inherited ratio. Standalone decimal arbitrary values, variables, and other CSS ratio forms remain unsupported. |
| Spacing, sizing, gap, and inset | Partial | `p*`, `m*`, `gap*`, `w`, `h`, `size`, min/max width/height, `inset*`, and side inset families share one numeric parser. Non-negative finite integer or decimal suffixes use Tailwind's default `0.25rem` spacing factor; supported sizing/inset families also accept `full` and positive fractions. Safe brackets accept non-negative `px`, `rem`, `%`, or `auto` where the family permits it. Viewport/container units, theme-variable spacing, CSS calculations, and logical block/inline axes remain absent. |
| Negative lengths | Partial | A leading `-` is accepted only for margin and inset families, including their axis/side forms and safe numeric/arbitrary values. Negative padding, gap, size, min/max, width/height, border width, and `auto` are compile errors. This is an explicit whitelist rather than general CSS negation. |
| Physical border radius | Partial | Named radii and safe `rounded-[Npx]` / `rounded-[Nrem]` values work for the broad, `t/r/b/l`, and `tl/tr/br/bl` physical prefixes. Every shorthand splits into typed corner slots; physical-prefix order, arbitrary candidate order, state cascades, and trailing `!` remain compile-time. Negative, non-finite, percentage, `auto`, and unknown-unit arbitrary radii are rejected. Logical start/end radius prefixes and elliptical two-axis CSS radii remain absent. |
| Typography | Partial | Text alignment, the default `text-xs` through `text-9xl` size scale, safe arbitrary `text-[Npx]` / `text-[Nrem]`, all mapped font weights, native `font-sans` (`.SystemUIFont`) and `font-mono` (`SF Mono`), italic, underline, line-through, whitespace, truncate/text-ellipsis, the line-height subset below, and positive-integer `line-clamp-*` are supported. Arbitrary font-family names, serif resolution, tracking, wrapping/breaking, decoration details, text transforms, lists, columns, and advanced text features remain absent. |
| Line height | Partial | `leading-none`, `tight`, `snug`, `normal`, `relaxed`, and `loose` lower to GPUI relative line heights; bare finite nonnegative `leading-N` (including decimals) uses the default spacing factor `N × 0.25rem`. Safe `leading-[Npx]` / `leading-[Nrem]` forms lower to typed absolute lengths; nonnegative unitless and percentage brackets lower to relative values (`leading-[1.5]` and `leading-[150%]` both become `relative(1.5)`). Zero is accepted. All forms share one typed line-height slot with canonical order and `!`. Signed/non-finite values, `auto`, custom properties, unsupported units, and font-size/line-height compound syntax remain unsupported. |
| Color vocabulary | Partial | `bg-*`, `text-*`, and `border-*` accept all 26 built-in families × 11 shades, black, white, transparent, and safe arbitrary `#rgb`, `#rrggbb`, or `#rrggbbaa`. These consumers also accept finite alpha modifiers `/N` for `0..=100`, `/[N%]` for `0%..=100%`, and `/[N]` for `0..=1`. The named vocabulary is complete only for these three consumers; `current`/inheritance, gradients, fill/stroke, ring/shadow colors, and other Tailwind color consumers remain absent. |
| Color representation | Host-different | The official package's 286 `oklch(...)` theme values are converted ahead of time to clamped, rounded 8-bit packed sRGB and checked into the proc-macro crate. Lookup and alpha validation occur only during macro expansion. Generated application code receives a numeric GPUI `rgb`/`rgba` value; a modifier produces GPUI's `f32` alpha, and for `#rrggbbaa` it multiplies the normalized 8-bit base alpha rather than replacing it. GPUI 0.2.2 cannot preserve OKLCH or out-of-sRGB components, and direct channel clamping is not browser color management or perceptual gamut mapping, so wide-gamut colors can render differently. Tailwind `@theme` overrides, CSS variables, `--alpha()`, theme opacity, and runtime palette changes do not alter this fixed table. |
| Border width, style, and shadow | Partial | `border`, `border-x/y/t/r/b/l`, numeric widths, and arbitrary `px`/`rem` widths use typed per-side slots with broad/axis/side precedence. Dashed style and GPUI's named `shadow-none` through `shadow-2xl` presets are mapped. Plain `border` emits only one-pixel widths: GPUI has no CSS `currentColor`, so an explicit `border-<color>` is required for portable visible output. Other line styles, outline, ring, divide, shadow colors/arbitrary shadows, and exact CSS visual parity remain absent. |
| Element opacity | Partial | Bare finite `opacity-N` is the inclusive `0..=100` percentage scale, including decimals. Brackets deliberately change the grammar: `opacity-[N]` accepts only the unit interval `0..=1` (`opacity-[.5]` is 50%), while `opacity-[N%]` accepts `0%..=100%` (`opacity-[50%]` is 50%). Ambiguous `opacity-[50]` is rejected instead of being guessed as a percentage, as are malformed, non-finite, and out-of-range values. The result is passed as `f32` to GPUI [`Styled::opacity`](https://docs.rs/gpui/0.2.2/gpui/trait.Styled.html#method.opacity), affecting the element and children; per-color alpha is the separate behavior above. |
| Interactivity | Partial | The GPUI-supported cursor set is substantially mapped, including context/copy/no-drop, grab, directional and axis resize forms. Intrinsic mouse down/down-out/move/up/up-out, pinch, scroll-wheel, hover, key-down/up, modifiers-changed, exact focus/blur, tracked-focus, and key-context bindings are implemented as described above. Tailwind pointer-event utilities, selection, resize behavior, scroll snap/behavior/margins, touch action, caret/accent, appearance, and the broader Tailwind 4.3 surface remain absent. |
| State variants | Partial / Host-different | One unstacked `hover:`, `active:`, `focus:`, `focus-visible:`, `in-focus:`, `group-hover:`, or `group-active:` prefix is supported, and unqualified `group` establishes GPUI's private default group. `focus-visible:` uses GPUI's exact-focus plus `last_input_was_keyboard` predicate; it is a native keyboard-modality heuristic rather than a promise of every browser's pseudo-class algorithm. `active:`, `group-active:`, `focus:`, `focus-visible:`, and `in-focus:` require an ID; all focus variants make the target focusable, while hover/group-hover alone do not. GPUI nested same-name groups resolve the nearest group hitbox, whereas Tailwind can match any unqualified group ancestor. `group group-active:*` on one element is rejected when any group-active property survives regular importance because GPUI can self-match the element's own group hitbox, unlike Tailwind's descendant selector; a fully regular-important-blocked group-active cascade is allowed, and same-element group-hover cannot see its own not-yet-pushed hitbox. Tailwind browser hover variants use `@media (hover: hover)`, while GPUI hitbox hover has no capability media gate. GPUI `in_focus` is ancestor-or-self, so the focused target itself matches even though Tailwind's implicit-ancestor `in-focus:` normally excludes self. `focus-within:` is the inverse self-or-descendant relationship and receives a targeted error because GPUI 0.2.2 has no fluent `contains_focused` style seam. Cross-state same-property combinations, including focus-visible with hover, are accepted only when Tailwind and GPUI choose the same winner; otherwise the macro emits a targeted error. Named groups, disabled/checked/open, structural selectors, data/ARIA, peer, `has`, not, pseudo-elements, and stacked/arbitrary variants remain absent. |
| Responsive/container variants | Not implemented / Host-different | `sm:` through `2xl:`, min/max, and container queries are absent. A native implementation would need to react to GPUI window/element bounds and invalidate the right entity; it cannot reuse CSS media/container query execution. |
| Dark/media variants | Not implemented / Host-different | `dark`, reduced motion, contrast, orientation, print, and other media variants are absent. Theme/window appearance can support native analogues; browser print and CSS media semantics are not generally equivalent. |
| Arbitrary values | Partial | The bounded compile-time grammar accepts length `px`/`rem`/`%`/`auto`, hexadecimal colors and their bounded bracketed alpha, border-width `px`/`rem`, unit-or-percent bracketed opacity, positive finite aspect fractions `[N/D]`, nonnegative `px`/`rem` font sizes, and nonnegative `px`/`rem`/unitless/percentage line heights. Supported values lower to typed GPUI values with no runtime parser. Arbitrary grid placement, parenthesis custom properties, CSS variables, `calc`/functions, URLs, type hints, arbitrary angles, standalone aspect decimals, and arbitrary values for other utility families remain rejected. |
| Arbitrary properties/variants | Host-different | Raw CSS properties and selector rewrites have no general native target. The escape hatch is a typed Rust element expression or a deliberately curated `ui` / `paint` native bridge, not a CSS interpreter hidden in the renderer. |
| Theme and plugins | Not implemented / Host-different | There is no `@theme`, CSS custom-property namespace, custom `@utility`, JavaScript plugin, preset, or config loader. The current native surface has no token/theme API and does not adopt CSS cascade semantics. |
| Important and conflicts | Partial | Tailwind 4's trailing important syntax (`p-4!`, including inside a supported state variant) is implemented per property slot. Importance is exact within one cascade and between regular style and each state: regular important blocks a normal state field, and an important state refines regular. Across two simultaneous state callbacks GPUI cannot carry importance, so the compiler permits a shared property only when GPUI's fixed refinement order selects the same declaration as Tailwind's importance/specificity/order calculation; otherwise it emits a targeted error. Supported spacing, size, gap, inset, border, physical radius, grid-count/placement, aspect, line-height, flex, truncate/overflow, alignment, and place-content conflicts use typed canonical slots independent of token order where documented. A coexisting static `class` is merged into every conditional `:class` leaf before this same cascade is resolved. Equal-rank conflicts still use the later candidate, and this bounded model is not a claim of general Tailwind stylesheet ordering. |
| Browser-only utility families | Host-different | Table layout, SVG/CSS filters, backdrop effects, masks, generated content, floats, columns, browser form controls, CSS transitions/keyframes, 3D CSS perspective, and similar families require case-by-case native primitives. They must not be reported as compatible merely because a class token can be parsed. |

## Intentional divergences

These rules are contracts, not accidental omissions:

1. **Synthetic fragment boundary:** multiple roots and a root `<>...</>` have
   Vue-like source syntax, but GPUI 0.2.2 requires one `IntoElement`; the macro
   therefore inserts one synthetic `div`. Nested fragments and structural
   `<template>` nodes flatten. `<template v-for>` remains rejected rather than
   silently inserting per-item layout wrappers.
2. **Slots return one host element:** `SlotContent` contains exactly one
   `AnyElement`, not a VNode/list fragment. Multi-root direct providers and
   `view!` providers therefore use the same synthetic boundary; wrapper-free
   Vue slot fragments are not claimed. One declared slot currently also permits
   only one outlet, including across mutually exclusive branches.
3. **Keys are stricter:** every loop root needs a dynamic `:key`; the key may be
   an idiomatic Rust GPUI ID such as a tuple. This rejects Vue's unkeyed
   in-place patch mode in exchange for predictable stateful descendants.
4. **Boolean means `bool`:** `v-if` and future conditions accept Rust `bool`.
   Numbers, strings, options, and pointers do not receive JavaScript truthiness.
5. **Expressions are Rust:** no expression string evaluation, JavaScript proxy,
   or implicit template scope object is introduced.
6. **DOM modifiers are native analogues:** `.stop` and `.prevent` call GPUI's
   propagation and window-default APIs; system modifiers inspect GPUI click
   events. They do not introduce DOM events. `.passive` is rejected explicitly
   because its listener-registration semantics have no GPUI equivalent. The
   mandatory mouse-button suffix is a native listener filter; modifiers on
   key-down/up, modifier changes, mouse/drag move, drop, gestures, hover, focus,
   and blur are rejected rather than assigned DOM meaning. GPUI's outside mouse-down
   callback is capture-phase and any-button; gpui-vue only adds the selected
   native-button guard, not DOM capture registration.
7. **Classes use a bounded typed cascade:** supported broad/axis/side shorthands
   resolve by property specificity, not source order, and suffix `!` participates
   in the compile-time winner selection. Equal-rank candidates remain
   later-wins, and there is no general CSS selector/cascade engine. Dynamic
   strings and raw CSS remain outside the zero-parser path. GPUI-native state
   relations also stay explicit: nested unqualified groups select the nearest
   same-name group, effective group-active self-matches and cross-state cascade
   mismatches are rejected, `in-focus:` includes the focused target itself, and
   the inverse
   `focus-within:` relation is rejected rather than approximated;
   `focus-visible:` uses exact focus plus the host's last-input modality. GPUI hover is
   native hitbox state, not Tailwind's browser hover-capability media query.
8. **Plain borders have no `currentColor`:** `border` emits widths only. Portable
   visible borders need an explicit `border-<color>` because GPUI has no CSS
   inherited-current-color border default.
9. **HTML semantics are explicit:** tag spelling does not create a DOM node.
   Accessibility roles, keyboard activation, text input, links, and forms need
   real GPUI-native components and tests rather than cosmetic aliases.
10. **Lifecycle is visual-host lifecycle:** `mounted` / `updated` use GPUI's
    post-draw deferred effect cycle, and `unmounted` follows keyed host teardown;
    these are not browser DOM-paint hooks. Naked entities and `v-show` do not
    participate, same-level teardown order is unspecified, and shutdown work
    held alive by an external owner is not a process-finalization guarantee.

## Performance and Rust design constraints

Feature alignment is acceptable only if it retains these properties:

- **One host tree and renderer.** Generated code uses GPUI elements directly;
  there is no mirror DOM, PocketJS tree, VNode tree, or FFI mutation queue.
- **No hot-path string interpretation.** Template structure, literal utility
  tokens, property conflicts, arbitrary-value validation, and every nested
  `:class` leaf are compiled ahead of time. Static classes are merged into each
  conditional leaf before cascade resolution; runtime class selection evaluates
  each typed Rust condition once along the selected precompiled builder path.
- **GPUI owns scheduling.** Entities, contexts, subscriptions, focus, input, and
  async tasks remain GPUI-owned. Reactive conveniences may notify them but must
  not create an independent scheduler without measured need.
- **Prefer inline local state.** `Local<T>` plus `Revision` and `Memo<T, D>` keep
  component-owned state and explicit dependency caches allocation-free when
  their contained Rust values are allocation-free. Shared state still uses the
  appropriate GPUI entity or the deliberately shared `Ref<T>` handle.
- **Stable identity is explicit.** Stateful interactions and loop descendants
  receive stable `ElementId` values; compile-time diagnostics reject ambiguous
  cases.
- **Native component retention has no layout wrapper.** A PascalCase compile
  slot and optional user key select GPUI per-window element state. The retained
  plain value owns the child entity and a fixed-size mount-scoped subscription
  array; the event-aware value instead owns the entity, one subscription, and
  one shared handler cell. `HostedEntity` delegates the native entity's element
  phases and carries only the compile-selected lifecycle token; without hooks it
  stays the same size as `Entity`.
- **Typed event reconciliation stays monomorphic.** All listeners on one child
  share one concrete closure, one native subscription, and one shared handler
  cell per mount. Later frames replace the handler value before input
  reconciliation; the no-listener path creates none of these event resources.
- **Typed public surfaces.** Props, emits/actions, slots, theme tokens, values,
  and event filters should use Rust types and traits. Unsupported syntax should
  fail at its source span.
- **Lazy slots erase once at the host boundary.** A non-empty `Slot<P>` owns one
  boxed `'static` provider and invokes it only when the receiving component asks
  for content; an empty slot stores no closure. Direct component markup captures
  only a parent `WeakEntity` and re-enters live state, while standalone `view!`
  uses owned-`'static` captures. The result is one `SlotContent` / `AnyElement`,
  with no collection or framework node. This is deliberately not an
  allocation-free-provider claim.
- **No hidden allocation promise.** Proc-macro lowering alone does not prove
  zero allocation. Optimizations must be evaluated in release builds with
  realistic element counts and state changes.
- **Measure before retaining subtrees.** Track render/update latency, frame/paint
  time, allocation count/bytes, peak memory, generated token/code size, and
  incremental compile time. A retained binding/effect layer is justified only
  by repeatable improvements over GPUI entity re-rendering.
- **Idiomatic quality gate.** Workspace code should keep `unsafe_code` forbidden,
  `missing_docs` denied (including private items where configured), formatting
  clean, and Clippy's configured lint set warning-free. Exceptions require a
  local rationale rather than a blanket relaxation.

Broad claims such as “Vue-compatible” or “Tailwind-compatible” require every
relevant row to move out of **Partial**, **Not implemented**, and
**Host-different**, plus
differential behavior tests. Until then the accurate description is
“Vue-inspired Rust syntax with a compile-time Tailwind-like GPUI subset.”
