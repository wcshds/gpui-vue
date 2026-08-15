# Component Setup DSL

`component!` 是完整的 generated component declaration。語法為：

```text
component! {
    DOCS VISIBILITY component Name {
        props { ... }? state { ... }? emits { ... }? slots { ... }?
        setup(this, props, cx) { ... }?
        mounted(this, window, cx) { ... }?
        updated(this, window, cx) { ... }?
        unmounted(this, cx) { ... }?
        template(this, window, cx) { ... }   // required
    }
}
```

sections 順序不限，每種至多一個。

## Declaration fields

```rust
props { /// Docs. label: SharedString, /// Docs. enabled: bool = true, }
state { /// Docs. count: Local<i32> = Local::new(0), }
emits { /// Docs. changed(value: i32); /// Docs. closed; }
slots { /// Docs. default: (); /// Docs. actions: ActionProps; }
```

每項均須 doc comment。props default 會成為每次完整 props construction 的 expression；state initializer 只在 component construction 執行。

## Generated surface

以 `EditorPane` 為例，視 sections 產生：

- `EditorPane`，實作 `Render` 與 `NativeComponent`；
- `EditorPaneProps`、`EditorPanePropsBuilder<...>`、private input；
- 有 emits 時的 `EditorPaneEvent` 與 emit helpers；
- 有 slots 時的 `EditorPaneSlots`、`new_with_slots` 與 `NativeComponentSlots`；
- 有 hooks 時的 `ComponentLifecycleHooks` implementation。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_stepper{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_stepper -->

## Constructor

`Name::new(props, cx)` 可在任何 `AppContext` 建立 entity；有 slots 時 `Name::new_with_slots(props, slots, cx)` 可直接傳 typed slots。這只建 entity，visual lifecycle 需要 PascalCase host 或 `run_component`。

## Compile-time errors

macro 會拒絕缺 template、重複/未知 section、缺 docs、duplicate fields、保留名稱與非法 hook binder shape。section body 仍是普通 Rust，型別/借用/lifetime 由 rustc 診斷。

## 另見

- [Setup Hook](/api/composition-api-setup)
- [Options State](/api/options-state)
- [Options Composition](/api/options-composition)
