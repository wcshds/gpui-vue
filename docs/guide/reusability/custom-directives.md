# 自訂指令

有些介面行為看起來適合寫成 `v-autofocus` 或 `v-tooltip`。在原生 GPUI 中，這些行為往往涉及 focus handle、element identity、prepaint 或視窗事件，而不是對 DOM 節點做一次變更。

## 目前狀態

`gpui-vue` 尚未提供自訂指令註冊 API。`view!` 只接受編譯器已知的結構指令與綁定，例如 `v-if`、`v-for`、`v-show`、`:style`、`:track-focus` 和事件監聽器。未知的 `v-*` 名稱會在編譯期失敗，不會被保留到執行期。

## 用元件封裝有狀態的行為

如果行為需要 focus、生命週期或訂閱，建立小型元件最清楚。以下 gallery component 把 `FocusHandle`、key context 與 key-down listener 放在同一個 owner：

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#focus_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#focus_demo{rust}

::: tip 執行結果
FocusDemo 取得鍵盤焦點後會顯示最後收到的按鍵；focus handle 與文字 state 都隨 component entity 保留，移除 keyed visual host 時相關 mount state 一併釋放。
:::

## 用 builder 函式封裝無狀態樣式

若只是重複組合原生元素，可回傳 `impl IntoElement`，或接受並回傳 `StyleRefinement` 的 `:style` callback。這條路徑沒有執行期指令表，也不會繞過 `view!` 對互動元素穩定 `id` 的檢查。

## 為何不模擬 DOM hook

GPUI element 每次 render 都可能重建，而 retained state 依 global element id 保存。`mounted(el)`／`updated(el)` 形式無法準確表達 layout、prepaint、paint 三階段，也很容易持有失效的 element 值。因此在正式的自訂行為 API 出現前，請使用元件、typed element builder，或 `paint::drawing_surface`。

下一步閱讀[外掛](./plugins.md)，了解應用級初始化與元素級行為的分界。
