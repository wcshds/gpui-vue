# Slots

Slot 讓 child 決定 layout，parent 決定部分內容。gpui-vue slot 是 lazy、typed Rust provider：child 呼叫時才執行 closure，scoped props 由 rustc 檢查，不存在 VNode collection。

## Default、named 與 scoped slot

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#typed_slots{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_slots -->

::: tip 執行結果
panel body 顯示「永字筆畫」，actions 區顯示「2 個操作」。因兩個 provider 都存在，兩段 fallback 不會求值；移除 provider 後才各自顯示 fallback。
:::

普通 component children 對應 `default`。direct child `<template #actions={pattern}>` 宣告 named/scoped provider；pattern 接收 child outlet 傳入的 exact `ActionProps`。

## Explicit Slot API

generated `<Component>Slots` 也能由 Rust 建立；同一 fixture 的 `explicit_slot_value` 使用 `Slots::new()` 和 `with_*` providers 組出完整值。

之後可呼叫 `SlotPanel::new_with_slots(props, slots, cx)`，或在模板使用完整 `:slots={slots}`。`:slots` 與 declarative children 互斥。

`Slot<Props>::new` 的 closure 另接收 `&mut Window` 與 `&mut App`；`from_fn` 適合不需要 render context 的 provider。`render` 回傳 `Option<SlotContent>`，`render_or_else` 只在 slot absent 時建立 fallback。

## Identity、ownership 與限制

component-aware direct markup 的 provider 只捕獲 parent `WeakEntity`，被 child 呼叫時重新進入仍存活的 parent context；因此 slot 內容可讀寫最新 parent state，也避免 strong cycle。standalone `view!` provider 則必須是一般 owned `'static` capture。

每次非空 slot invocation 產生一個 type-erased `AnyElement`。多 root provider 會使用 synthetic root wrapper；這不是 wrapper-free fragment。動態 slot 名、同名多個 provider、同一 declaration 的第二個 outlet、outlet directives，以及 non-unit slot 未給 `:props` 都會被拒絕。

## 相關閱讀

- [元件基礎](../essentials/component-basics)
- [Props](./props)
- [Fallthrough Attributes](./attrs)
