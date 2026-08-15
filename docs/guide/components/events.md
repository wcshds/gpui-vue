# 元件事件

Child 不應取得 parent 的可變狀態借用。`emits` section 會產生 typed event enum 與 `emit_*` helper；PascalCase listener 透過 GPUI entity subscription 把 event 送給直接 parent。

## 宣告與接收事件

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#typed_events{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_events -->

::: tip 執行結果
每點一次「+2」，child 發出 `StepButtonEvent::Increment { amount: 2 }`；parent 透過 weak entity 回到自己的 context，總計依序顯示 2、4、6……。
:::

一個 `emits` section 可宣告多個事件，也可有零個或多個 named payload。generated helper 直接呼叫 `Context::emit`，payload type 不做字串序列化。

## Delivery contract

listener callback 收到 `&<Component>Event`、`&mut Window` 與 `&mut App`。若 enum 有多個 variant，generated dispatcher 只把對應名稱的 variant 交給該 listener。listener 只訂閱直接 child，不 bubble，也不會 fall through 到 wrapper。

同一 child 的多個 listener 共用一個 native subscription 與 handler cell；parent rerender 會替換 handlers，不會為同 identity 重複 subscribe。沒有 listener 時不建立這些 event resources。

Construction / `setup` 早於 parent subscription 安裝，所以在 setup 中 emit 可能無人接收；使用者操作或 `mounted` 後的事件較符合預期。

## 目前限制

PascalCase event 不接受 `.stop`、`.once` 或其他 modifiers。也沒有 event validation、string-based dynamic event name、multi-level bubbling，callback 取得的是完整 enum，而不是解構後的單一 payload參數。

## 相關閱讀

- [Props](./props)
- [元件上的 `v-model`](./v-model)
- [事件處理](../essentials/event-handling)
