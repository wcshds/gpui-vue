# 表單與文字輸入

原生桌面沒有 HTML `<form>`。對需要鍵盤、選取、剪貼簿與中文輸入法的單行欄位，gpui-vue 提供 retained `TextInput`；它直接實作 GPUI 的平台文字輸入 contract，並用 typed event 或 `TextModelBinding` 把值交回 parent component。

## 建立 IME-aware 輸入框

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#search_panel{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#search_panel -->

::: tip 執行結果
原生欄位可以輸入英文、繁體中文與輸入法 composing 文字；下方「目前內容：」會跟著 `Change` 即時更新。按 Return 會另送出 `Submit`，但這個最小範例只把 change flow 顯示在畫面上。
:::

<NativeResult
  src="/screenshots/gallery-input.png"
  alt="單獨執行的 SearchPanel，顯示 IME-aware 原生文字輸入與目前內容"
  caption="輸入框是實際 GPUI platform input handler，不是文檔中的 HTML 仿製品。"
/>

`text_input(placeholder, cx)` 建立 `TextInputHandle`，也就是 retained `Entity<TextInput>`。範例把 `TextModelBinding` 存在 component state：drop binding 時，input → model 與 model → input 兩個 subscription 都會解除。

## 外觀與初始策略

不必在 `TextInput` 外再套一層假輸入框。`TextInputConfig` 與 `TextInputStyle` 直接設定原生 control：

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#configured_text_input{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#configured_text_input -->

這個可見範例設定高度、padding、背景、文字、placeholder、border、focus border、selection、caret、圓角與字級；`read_only(true)` 保留 focus、選取與複製，但拒絕鍵入、刪除、剪下、貼上和 IME replacement。`disabled(true)` 則會退出 tab order、拒絕 pointer / keyboard interaction，也不註冊 platform input handler。

`max_length(n)` 限制後續 user edit，並以 Unicode extended grapheme cluster 計數，不會從 emoji family 或結合字的中間切開。輸入法的 marked composition 可暫時超過上限，等平台 commit 後才套用限制，避免拼音還沒組成漢字便被截斷。`set_text` 與初始 controlled value 受 parent 信任；降低 limit 不會暗中改寫既有 model value。

## 輸入事件

`TextInputEvent` 有三種：

- `Change(String)`：可見內容變動，包含輸入法 composing 中間值；
- `Submit(String)`：完成平台 composition 後按 Return / Enter；
- `Escape`：取消 marked text、釋放 focus 並通知 parent。

`TextInput` 支援 grapheme-aware 左右移動與刪除、selection、滑鼠拖選、Home/End、複製/貼上/剪下/全選、水平 caret scrolling，以及平台 marked-text underline。

## Controlled 更新與 focus

parent 可透過 entity update 設值：

```rust
use gpui_vue::prelude::*;

fn update_input<Owner: 'static>(
    input: &TextInputHandle,
    cx: &mut Context<'_, Owner>,
) {
    input.update(cx, |input, cx| input.set_text("永", cx));
}
```

相同文字會被忽略，避免每次 parent render 都重設 caret 或 composition。另有 `clear`、`set_placeholder`、`set_style`、`set_disabled`、`set_read_only`、`set_max_length`、`text`、`selected_range`、`is_composing`、`focus_handle`、`focus(window)` 與 `blur(window)`。

`TextModelBinding::bind` 把這個相等抑制用在兩個方向。input 的 `Change` 交給 `write` closure；parent 每次 `notify` 後，`read` closure 取得 canonical value 並靜默回寫。若回寫值與 composing 中間值相同，marked range 不會被清掉。共享 `Ref<String>` 可改用 `TextModelBinding::bind_ref`。

## 目前表單邊界

::: warning 尚未實作
`v-model` template 語法、checkbox、radio、select、多行 textarea、validation 與 declarative form submission 尚未提供。不要把 `<button>` 的外觀當作完整 semantic form control；目前也沒有瀏覽器 constraint-validation 或 autofill 語義。
:::

安全的 password / secure-entry 模式也尚未提供。只把 glyph 畫成圓點仍會讓 clipboard、IME 與 platform accessibility 看見原文，因此 gpui-vue 不把視覺遮罩偽裝成安全輸入。

## 相關閱讀

- [元件上的 `v-model`](../components/v-model)
- [事件處理](./event-handling)
- [Template Refs](./template-refs)
