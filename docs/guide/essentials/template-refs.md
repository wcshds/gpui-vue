# Template Refs 與原生 Handle

桌面 UI 常要讓某個控制項取得焦點，或從 retained entity 讀取狀態。gpui-vue 不產生 DOM node，因此沒有 `HTMLElement` template ref；相對應的原生工具是 typed `FocusHandle`、`Entity<T>` 與專用 handle alias。

## 保存焦點 identity

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#focus_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#focus_demo -->

::: tip 執行結果
點擊「聚焦編輯區」後，`editor_focus` 成為目前 window focus；編輯區套用藍色 focus border，並以 `Editor` key context 接收後續鍵盤路由。
:::

`:track-focus` 把既有 `FocusHandle` 綁到 intrinsic host。`focusable` 讓它進入原生 focus 系統，`key-context` 則為 actions/keyboard dispatch 提供 context。這些互動 attribute 都需要穩定 `id`。

## Retained component handle

由 `component!` 產生的 `Component::new(props, cx)` 回傳 `Entity<Component>`。parent 可把 entity 放在 state，之後用 `read` 讀取或 `update` 修改。以下以具體的 `TextInputHandle` 展示同一套 entity API：

```rust
use gpui_vue::prelude::*;

fn replace_and_read<Owner: 'static>(
    input: &TextInputHandle,
    cx: &mut Context<'_, Owner>,
) -> String {
    input.update(cx, |input, cx| input.set_text("永", cx));
    input.read(cx).text().to_owned()
}
```

這是 typed native handle，不是從 template 查詢 element。直接保存 entity 也會延長 entity 壽命；visual host 消失與 entity release 可能因此發生在不同時間。

`TextInputHandle` 是 `Entity<TextInput>` 的 alias，另提供 `focus()`、`set_text()` 與 selection API，適合需要 IME 的欄位。

## 目前限制

::: warning 尚未實作
模板尚無 `ref="name"`、自動 ref array、component expose 或渲染後 element geometry query。GPUI 的低層 element state 與 bounds 能力尚未整理成 gpui-vue template-ref abstraction。
:::

瀏覽器 CSS selector、DOM traversal 與 `HTMLElement` API 屬 Web-only，不會在 native backend 模擬。需要焦點、文字輸入、繪圖或模型存取時，應保存對應的 typed native handle。

## 相關閱讀

- [表單與文字輸入](./forms)
- [生命週期](./lifecycle)
- [元件基礎](./component-basics)
