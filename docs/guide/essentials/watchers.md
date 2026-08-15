# Watchers 與 Effects

當副作用要回應另一個 entity 的通知或 typed event 時，使用 `watch_entity` / `watch_event`，並保留回傳的 `Subscription`。這些 API 建立在 GPUI 原生 observation 上，不會另外建立自動 dependency graph。

## 觀察 typed event

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#manual_observation{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#manual_observation -->

::: tip 執行結果
在欄位輸入「永」時，`TextInput` 發出 `Change("永")`，watch callback 更新 parent；下一幀下方文字顯示「觀察到：永」。離開畫面、scope 被 drop 後，subscription 自動取消。
:::

`watch_event` 是不需要 `Window` 的版本；`watch_event_in` 讓 callback 能操作視窗。對不是 event emitter、只用 `cx.notify()` 宣告變動的 entity，使用 `watch_entity` 或 `watch_entity_in`。

## 管理 effect 壽命

`EffectScope::track` 收集 subscription，`clear()` 立即取消全部 observer，drop 也有相同清理效果。`detach()` 會刻意讓 callback 脫離 scope，直到相關 entity 釋放才停止；一般元件不應隨意 detach。

其他明確排程工具包括：

- `next_frame(cx, window, callback)`：下一個 native frame 執行一次；
- `defer(cx, window, callback)`：目前 GPUI effect cycle 結尾執行一次；
- `on_release(cx, cleanup)`：owner entity 被 GPUI 釋放時清理。

每個 callback 都重新進入 typed owner context，不需要捕獲長期 `&mut Context`。

## 與自動 watcher 的差異

::: warning 尚未實作
目前沒有 `watch(source_closure, ...)`、deep watch、immediate option、cleanup callback protocol 或 `watchEffect` 自動收集依賴。讀取 `Local` / `Ref` 不會登記 watcher；要觀察的 entity 或 event stream 必須明確傳入。
:::

這也表示 `Ref<T>` clone 雖共享資料，卻不會自行廣播給每個 reader。若跨 entity 需要通知，優先讓資料成為 GPUI entity 或 application global，並使用原生 observer。

## 相關閱讀

- [反應式狀態基礎](./reactivity-fundamentals)
- [生命週期](./lifecycle)
- [元件事件](../components/events)
- [Provide / Inject](../components/provide-inject)
