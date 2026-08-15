# Component Instance

generated component 是 GPUI `Entity<Component>` 中的普通 Rust struct。通常在 template 內以 `this` 存取，或由 owner 保存 `Entity` handle；沒有 JavaScript proxy instance、`$el` 或字串索引屬性。

## Generated accessors

每個 component 產生：

```rust
impl Component {
    pub fn new<C: AppContext>(
        props: ComponentProps,
        cx: &mut C,
    ) -> C::Result<Entity<Self>>;

    pub const fn props(&self) -> &ComponentProps;
}
```

有 slots 時另有：

```rust
pub fn new_with_slots<C: AppContext>(
    props: ComponentProps,
    slots: ComponentSlots,
    cx: &mut C,
) -> C::Result<Entity<Self>>;
pub const fn slots(&self) -> &ComponentSlots;
```

event helpers依 declaration 產生，如 `emit_saved(name, cx)`；它們呼叫 native `Context::emit` 並回傳 `()`。

## `Entity<Component>`

在 owner context 中用 native `read` / `update` 取得 component：

```rust
fn build_component(
    props: ComponentProps,
    cx: &mut gpui_vue::ui::App,
) -> gpui_vue::ui::Entity<Component> {
    Component::new(props, cx)
}
```

直接 constructor 建立的是 naked entity。要讓 PascalCase child identity、props reconciliation、typed declarative listeners與 visual lifecycle 生效，請在 `view!` / component template 中掛載；window root 使用 `DesktopApp::run_component`。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#component_card{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#component_card -->

## Public framework traits

| Trait | 目的 |
| --- | --- |
| `NativeComponent` | associated `Props` / `Input` / mount state，以及 construct/reconcile contract |
| `NativeComponentSlots` | associated `Slots` 與 typed input adapter |
| `NativeComponentEvents` | associated typed event enum |
| `ComponentLifecycleHooks` | macro 產生的 statically dispatched hook bodies |

這些 traits 公開是為了 downstream macro expansion 與進階 host integration。手寫實作必須維持 reconciliation、identity 與 lifecycle invariants；一般應用不要用它們模擬 Vue instance APIs。

## `ComponentElement` 與 mounts

`ComponentElement` / `ComponentEventElement` 是透明 native host；`ComponentMount` / `ComponentEventMount` 保留 entity 與 subscriptions，並提供 `entity()` clone。`HostedEntity` 委派 layout/prepaint/paint，不增加 layout node。大部分 call site 應由 PascalCase lowering 自動建構。

低層 constructors `component_element(...)` 與 `component_element_with_events(...)` 供 macro lowering / host integration 建立這兩種 element。`ComponentLifecycleMount` 與 `LifecycleRenderToken` 是 lifecycle-enabled host 的 retained state/token。它們雖因下游 macro hygiene 而公開，仍屬 framework internals；signature 會跟隨 component host invariants，應用程式不要直接耦合。

`RequiredProp<T, PropMissing>::missing()`、`.set(value)` 與 `RequiredProp<T, PropSet>::into_value()` 是 generated typestate builder 的 sealed storage lane。只有 macro 產生的 builder 應使用它。

## Missing Vue instance members

沒有 `$data`、`$props` dynamic map、`$attrs`、`$refs`、`$parent`、`$root`、`$slots` dynamic map、`$watch`、`$forceUpdate` 或 `$nextTick`。對應能力是 typed fields/accessors、entity ownership、`EffectScope`、`cx.notify()` 與 `next_frame`。

## 另見

- [Options State](/api/options-state)
- [Options Composition](/api/options-composition)
- [Custom Renderer](/api/custom-renderer)
