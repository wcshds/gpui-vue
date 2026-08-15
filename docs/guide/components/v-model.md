# 元件上的雙向值流

gpui-vue 目前不解析 `v-model` template attribute；對內建 `TextInput`，`TextModelBinding` 已提供同一個核心資料流：parent 擁有 canonical value，input 保留 selection 與 IME composition，兩者以 typed `Change` 和 silent controlled update 同步。

## 可執行的受控文字欄位

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#search_panel{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#search_panel -->

::: tip 執行結果
欄位接受原生中文 IME；每次 `Change` 都更新 `Local<String>`，下方預覽與可見輸入保持一致。其他操作修改 Local 並通知 parent 時，binding 會自動靜默回寫 input。
:::

`TextModelBinding::bind` 接受 initial value、`read` closure 與 `write` closure。它擁有兩個 native subscriptions，必須像範例一樣存進 parent state。drop 後兩個方向一起取消；需要共享 model 時，`bind_ref` 接受 `Ref<String>`。

這個流程把 ownership 寫得很清楚：input entity 擁有 selection/composition，parent 擁有 domain value，binding 的存活時間由 parent state 控制。parent notification 若回傳和 composing 中間值相同的字串，`set_text` 會忽略它，不會把 marked range 重設。

## 尚未可用的語法

以下只是未實作 API 的概念形狀，不能放進 Rust 程式：

```text
<EditorField v-model={this.title} />
<EditorField v-model:query={this.query} />
```

gpui-vue 尚未定義 template 語法如何產生 prop/event 名稱、處理多個 model 與 modifier，所以巨集會拒絕它們。這不代表 `TextInput` 缺少 controlled binding；缺少的是 declarative lowering 與自訂 component 的通用 model convention。

對一般自訂元件，目前可組合 [Props](./props) 與 [typed emits](./events)：parent 傳入 `value={...}`，child 發出 `change(value: T)`，listener 再更新 parent owner。

## 相關閱讀

- [表單與文字輸入](../essentials/forms)
- [Props](./props)
- [元件事件](./events)
