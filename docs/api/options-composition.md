# Options API: Composition

`emits` 與 `slots` 讓 component 間的輸出與內容保持 Rust typed。兩者都在編譯期產生 concrete types，不使用字串 event bus 或 dynamic slot map。

## `emits`

```rust
emits {
    /// Reports a save with its name.
    saved(name: SharedString);
    /// Requests cancellation.
    cancel;
}
```

macro 產生 `<Component>Event` enum、`EventEmitter` implementation 與 `Component::emit_saved(...)` / `emit_cancel(...)` helpers。parent handler 收到完整 enum reference：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_events{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_events -->

PascalCase listener 使用 `@saved={handler}` 或 `on:saved={handler}`；kebab-case 正規化為 snake_case。event 只來自 direct child，不 bubble，不接受 `.stop` / `.once` 等 modifiers。setup 中過早 emit 可能先於 parent subscription。

## `slots`

```rust
slots {
    /// Main content, with no scoped props.
    default: ();
    /// Action area receiving typed props.
    actions: ActionSlotProps;
}
```

macro 產生 `<Component>Slots`，其中每個 field 是 `Slot<Props>`；`new()` 建立全部空 slot，`with_<name>(provider)` 設定 provider。component 會多出 `new_with_slots`、`slots()`，並實作 `NativeComponentSlots`。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_slots{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_slots -->

parent ordinary children提供 default slot；`<template #actions={pattern}>` 提供 named/scoped slot。provider lazy 執行並可讀 parent 當下 state。explicit `:slots={typed_slots}` 不能與 declarative children 混用。

一個 declared slot 目前在 child template 只能有一個 outlet；non-unit slot 必須帶 `:props`。slot outlet 自身不接受 `v-if` / `v-for` / `v-show`，可在外層放 structural template。

## 另見

- [Events 指南](/guide/components/events)
- [Slots 指南](/guide/components/slots)
- [Composition Helpers](/api/composition-api-helpers)
