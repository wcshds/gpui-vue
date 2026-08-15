# Built-in Special Elements

special elements 只存在於 macro grammar；它們不會建立 DOM node。

## `<component />`

任何 simple PascalCase identifier 會進入 generated component lane：

```rust
<ComponentCard label={"Native".into()} />
```

macro 經 `NativeComponent::Props` / `Input` associated types lower，並以 compile-site identity 及可選 `key` 保留 child entity。支援 typed props、events、slots、conditions 與 keyed loops。module path tag / generic tag 不支援；可先用 Rust `use ... as ...` 取 alias。

## `<slot />`

只可用於 `component!` direct template，且必須對應 `slots` section 中的 static declaration：

```rust
<slot><text>"Fallback"</text></slot>
<slot name="actions" :props={ActionSlotProps { count: 2 }} />
```

unit slot 可省略 `:props`；non-unit slot 必須傳 exact typed value。fallback 只有 provider 缺少時才 lazy 建立。一個 slot declaration 目前只能出現一個 outlet。

## `<template>`

structural template flatten children，不建立 element：

```rust
<template v-if={ready}>
    <text>"Title"</text>
    <text>"Body"</text>
</template>
```

它只接受 conditional directive；`v-show` 無 host 可隱藏，`v-for` 無 wrapper 可保存 repeated identity，因此兩者拒絕。作為 component direct child 時，`<template #name={pattern}>` 是 named/scoped slot provider。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_slots{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_slots -->

## Fragments

`<> ... </>` nested fragment 直接 flatten。render root fragment / 多 roots 會加入 synthetic `div`，因目前 GPUI `Render` 邊界需要一個 `IntoElement`；所以 root fragment 並非 wrapper-free layout。

## Intrinsic tags

`div`、`view`、`text`、`span`、`button` lower 至 curated native elements；`img` 走 media image pipeline並要求恰好一個 `src` / `:src`。名稱是 authoring affordance，不代表 HTML semantics：`button` 仍需要測試 keyboard/accessibility 行為，`text` 也不是 DOM Text node。

## 另見

- [Template Syntax](/guide/essentials/template-syntax)
- [Options Composition](/api/options-composition)
