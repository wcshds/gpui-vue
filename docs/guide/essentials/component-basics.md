# 元件基礎

元件把 typed props、entity-local state、事件、slot 與 render template 放在一個 Rust item 裡。`component!` 會產生 ordinary Rust structs 與 GPUI trait implementations，不會建立 VNode runtime。

## 定義並使用元件

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#component_card{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#component_card -->

::: tip 執行結果
畫面一開始依序顯示卡片文字「Typed component」與預設 tone「Native」、一枚「儲存」按鈕，以及 parent 保存的「尚未儲存」。點擊按鈕後，`SaveButton` 送出帶有 `glyph.kage` 的 typed `saved` event，最下方文字更新為「已儲存 glyph.kage」。
:::

<NativeResult
  src="/screenshots/gallery-components.png"
  alt="單獨執行的 ComponentCardDemo，顯示 typed component 卡片、儲存按鈕與尚未儲存狀態"
  caption="卡片、按鈕與 parent 接收的 typed event 狀態都由本頁引用的 Rust 元件實際渲染。"
/>

## Component sections

一個 declaration 可依需要包含：

- `props`：parent 輸入；每個 field type 必須實作 `PartialEq`；
- `state`：entity 建立時執行一次的 typed initializer；
- `emits`：typed child-to-parent event；
- `slots`：lazy typed content provider；
- `setup`：construction 中執行一次；
- `mounted` / `updated` / `unmounted`：visual host lifecycle；
- `template`：必要的 native render body。

component 本身以及 props、state、events、slots declarations 都必須有 Rust doc comment；workspace 啟用了 missing-docs lint。

## Props 與 state 的更新方式

PascalCase host 在 parent 每次 render 時 reconciliation input。普通 props 值不同才替換並通知 child；child entity identity 不變，所以 `state` 不會重跑 initializer，`setup` 也不會重跑。

帶 slots 的 input 因 closure 無法比較，parent reconciliation 會保守地視為可能影響 render。詳細 construction API 請見 [Props](../components/props) 與 [Slots](../components/slots)。

## Template 可回傳什麼？

`template(this, window, cx)` 可以直接以 `<...>` 開始、呼叫 `view!`，或寫普通 Rust block 回傳任意 `IntoElement`。`this.props()` 讀取目前 props，`this.slots()` 只在有 slots section 時生成。

PascalCase tag 目前只接受簡單 identifier（alias 可以），不接受 path/generic tag。component host 不繼承 intrinsic `class`、focus 或任意 attrs，也不為 child 增加 layout wrapper。

## 相關閱讀

- [Props](../components/props)
- [元件事件](../components/events)
- [Slots](../components/slots)
- [生命週期](./lifecycle)
