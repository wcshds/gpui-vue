# Options API: Miscellaneous

gpui-vue 的元件選項由 Rust item visibility、文件與型別系統控制，沒有 Vue runtime options merging。

## Visibility

```rust
component! {
    /// Exported component.
    pub component Inspector { /* ... */ }
}
```

component visibility 也套用到 generated props、builder、event、slots、constructor、accessor 與 emit helpers。Rust module path / `use` 決定可見範圍；不需要也不存在 `components: { ... }` registry。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#registration_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#registration_demo -->

## Documentation contract

component，以及每個 props/state/emits/slots declaration 都必須有 doc comment。這讓公開與私有 generated API 都能通過 crate 的 `missing_docs` quality gate，也讓錯誤發生在 declaration span。

## Names

- PascalCase tag 只接受 simple generated component identifier；先用 `use module::Type as Alias` 引入可保持 hygienic associated types。
- kebab-case prop/event/slot name 在 markup 正規化為 snake_case。
- duplicate canonical names會在 macro expansion 報錯。
- `build` 保留給 props builder terminal method。

## 不提供的 options

沒有 `name`、`inheritAttrs`、`mixins`、`extends`、runtime `components` / `directives` registry、`expose` 或 render-cache policy。Rust trait、function、module 與 composition 是相應的語言級工具。

## 另見

- [Component Setup](/api/component-setup)
- [Component Instance](/api/component-instance)
- [Registration 指南](/guide/components/registration)
