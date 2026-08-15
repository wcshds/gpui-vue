# Options API: State

`component!` 的 `props` 與 `state` sections 是 gpui-vue 的 typed state declaration。它們不是 runtime options object；macro 會產生普通 Rust fields 與 constructors。

## `props`

```rust
props {
    /// Required title.
    title: SharedString,
    /// Optional starting count.
    initial: i32 = 0,
}
```

每個 field 必須有 doc comment。無 default 的 prop 是 required；有 default expression 的 prop 產生 `with_<name>` override。generated props derive `PartialEq`，所以每個 field type 也必須實作 `PartialEq`。

對 `CardProps`，macro 產生：

- `CardProps::new(required...)`，並可鏈接 `.with_initial(...)`；
- consuming typestate `CardProps::builder()`，required fields 全部設定後才有 `.build()`；
- 全部 props 都有 default 時才實作 `Default`；
- PascalCase markup 的 exact-typed setters。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_props_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_props_demo -->

`label="literal"` 傳 `&'static str`，不隱式配置或轉成 `String`。kebab-case prop 名在 markup 正規化為 snake_case。`:props={complete_value}` 與 individual attrs 互斥。

## `state`

```rust
state {
    /// Component-local count.
    count: Local<i32> = Local::new(props.initial),
    /// Retained native subscription.
    subscription: Option<Subscription> = None,
}
```

initializer 只在 entity construction 執行一次，可讀初始 `props` 與 `cx`。field 可是任何具體 Rust type；`Local<T>` 提供 revision 與 equality-suppressed notification，但不是強制 wrapper。

parent 後續傳新 props 時，generated host 比較 props 並 reconcile；state initializer 與 setup 不重跑。component 內以 `this.props()` 取得目前 props 的共享借用。

## Errors

缺 required prop、型別不符與 builder 尚未完整時由 rustc 報錯；duplicate/unknown markup prop、保留名稱及缺文件由 macro 報錯。props 沒有 runtime validator、coercion 或 mutation proxy。

## 另見

- [Props 指南](/guide/components/props)
- [Reactivity Core](/api/reactivity-core)
- [Setup](/api/composition-api-setup)
