# 非同步資料與元件

原生工具常在網路或磁碟資料尚未完成時先顯示等待狀態。gpui-vue 的正式做法是讓 component 擁有 `AsyncResource<Value, Error>`：資源同時保存 UI state、native `Task`、取消語意與 request generation，不需要穿透到 GPUI 才能安全啟動 future。

## 讓 owner 保存資源

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

::: tip 執行結果
Gallery 掛載後啟動 initial load，區塊依序從 `Loading…` 進入 `Initial resource loaded`。按下 Reload 會取代上一個 request；畫面同時顯示 generation，完成後顯示該次 reload 的結果。
:::

`load` 只在 `Idle` 時啟動，適合 mounted 中的第一次請求；`reload` 不論目前狀態都開始新 request。傳入的 `locate` closure 必須從 component 找回**同一個** resource，讓 async completion 能透過 weak owner 安全更新它。

資源進入 `Loading` 時會同步通知 owner。新 request 會 drop 舊 `Task`，generation 也會前進；即使舊 completion 與取消競速，過期結果仍不能覆寫最新 state。drop resource 會取消它還持有的工作，`cancel(cx)` 則主動回到 `Idle`。

## 需要 window 的工作

`load_in` / `reload_in` 把 `AsyncWindowContext` 傳入 future，供工作在 `.await` 後更新原視窗。若工作不需要維護 loading/error state，可直接用 `spawn(cx, operation)` 或 `spawn_in(cx, window, operation)`；兩者都提供 weak owner，回傳的 `Task` 被 drop 時就取消。

不要從 `template` 啟動工作。render 可以重跑多次，這會重送 I/O；應在 mounted、event 或其他明確 effect 邊界啟動，並由 owner 保存 `AsyncResource` 或 `Task`。

## 非同步元件仍是不同問題

Rust component type 通常已靜態編進 binary。`AsyncResource` 解決「固定 component 等待資料」；它不會依 future 載入另一個 component type，也不會自動收集 descendant dependencies。

::: warning 尚未實作
目前沒有 async component factory、loading/error component options、delay/timeout/retry policy、code splitting、動態 library loading，或 descendant `<Suspense>` boundary。這些能力不能用 `AsyncResource` 的名稱代替。
:::

## 相關閱讀

- [非同步邊界](../built-ins/suspense)
- [Effects API](/api/composition-api-lifecycle)
- [條件渲染](../essentials/conditional)
