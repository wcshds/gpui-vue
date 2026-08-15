# 組合式寫法與型別

在普通 Rust helper 中組合 state 時，函式 signature 就是契約；不需要泛型 Hook runtime，也不需要為 callback 另寫型別註解語言。

## 封裝一個可重用狀態操作

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_toggle_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#typed_toggle_demo{rust}

::: tip 執行結果
`toggle_local` 反轉傳入的 `Local<bool>`，並回傳這次是否真的變更（反轉時為 `true`）。測試可傳入 `()` 作為 no-op notifier；元件事件中傳入 `cx` 則會安排重繪，同一份邏輯不用條件編譯。
:::

## Observer 的 callback 型別

`watch_entity`、`watch_event` 與其 `_in` 版本保留 GPUI 的具體 owner/emitter/event 型別，回傳 `Subscription`。多個 observer 可放進 `EffectScope`，drop 或 `clear()` 就取消全部 callback；這比失去 owner 的 detached closure 更容易推理。

## 泛型與 trait

跨元件共用行為時，先考慮普通 trait 或泛型函式。若回傳不同 element concrete types，可在分支外使用 `view!` 讓 macro 統一輸出，或在明確邊界轉為 `AnyElement`；不要在所有 helper 中預先 type erase。

目前沒有自動 dependency inference 或 `watchEffect` 對任意 `Ref` 讀取的追蹤。Entity/global watchers 是 typed stream，而非 closure 執行期間的隱式收集。component macro 的區段式寫法見下一頁。
