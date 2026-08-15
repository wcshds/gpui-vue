# 非同步邊界

資料載入不能阻塞平台事件迴圈，也不能讓舊 request 在新畫面完成後回寫。gpui-vue 已提供 owner-scoped `AsyncResource` 處理單一資源；尚未提供 Vue `<Suspense>` 那種 descendant aggregation。

## 一個可取消的資源邊界

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

component 直接對 `resource.state()` 做 exhaustive match，因此 `Idle`、`Loading`、`Ready` 與 `Error` 不會形成互相矛盾的布林組合。`reload` 擁有 GPUI `Task`，drop 或取代 task 會取消工作；generation guard 再阻擋已排隊的 stale completion。

對只需一次 owner-safe future 的情況，`spawn` 會把 `WeakOwner<Owner>` 與 `AsyncContext` 傳入 async closure。需要更新 originating window 時使用 `spawn_in`；呼叫端仍應保存回傳的 task，讓 cancellation lifetime 可見。

## 為何這還不是 `<Suspense>`

Vue Suspense 會等待 descendant async dependencies，協調 fallback/default subtree 與 nested boundary。`AsyncResource` 只管理它所在 owner 的一個 request state machine；它不掃描 child component，也沒有隱式 template dependency registration。

目前仍沒有：

- `<Suspense>` fallback slot 與 nested boundary；
- descendant dependency collection；
- async component factory 與 code splitting；
- boundary timeout、error propagation 或 reveal coordination。

這個邊界是刻意的。render 只讀 state，副作用由 mounted/event/effect 明確啟動，避免每次 render 重送 I/O。需要切換畫面仍保留結果時，讓更長壽的 model owner 保存 resource；這與[保留非顯示中的元件](./keep-alive.md)是同一項 ownership 決策。

## 相關閱讀

- [非同步資料與元件](../components/async)
- [Composition Helpers API](/api/composition-api-helpers)
- [Lifecycle / Effects API](/api/composition-api-lifecycle)
