# SSR 與 hydration 邊界

SSR 解決的是伺服器先輸出 HTML，再由瀏覽器把 client runtime 接回既有 DOM。`gpui-vue` 的輸出是原生 GPUI elements 與 GPU draw commands，沒有 HTML、DOM 或瀏覽器 hydration 目標。

## 明確的 Web-only 部分

下列能力不適用於目前的 native renderer：

- HTML string rendering 與 streaming response；
- DOM hydration、mismatch recovery 與 client-only boundary；
- `onServerPrefetch`、request context 與瀏覽器 event replay；
- CSS／script preload tag 與伺服器產生的 head metadata。

因此 `gpui-vue` 不會提供看似相容、實際卻沒有相同語意的 `renderToString`。

## 原生應用相近但不同的需求

桌面程式仍需要快速首屏。可在 build 時嵌入圖示與靜態檔案：

```rust
use gpui_vue::EmbeddedAssets;

static APP_ICON: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg'/>";
let assets = EmbeddedAssets::new()
    .with_file("icons/app.svg", APP_ICON);

assert_eq!(assets.get("icons/app.svg"), Some(APP_ICON));
```

這會建立 native asset source，並不產生 HTML。大型文件可在背景解析、先顯示 shell 與 loading state；序列化 model 可用應用選擇的 serde format，但它也不是 hydration。

::: tip 執行結果
這段會在 `icons/app.svg` 取得同一份 static bytes；安裝 `EmbeddedAssets` 的桌面應用可用該 logical path 載入真正圖像，不需要啟動 HTTP server。程式仍由 GPUI 建立完整原生視圖。
:::

需要首屏資料時，component 可擁有正式的 `AsyncResource`，讓 request、取消與 UI state 共用同一個 owner lifetime：

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

這個 resource 在 native event loop 中非同步完成並通知 entity；它不是 server prefetch、HTML serialization 或 hydration payload。沒有 state machine 需求的工作可用 `spawn` / `spawn_in`，但仍要保存回傳的 `Task`。

若產品同時需要 Web 與桌面前端，應共享 domain model/protocol，分別建立 Vue Web renderer 與 gpui-vue native renderer。下一步看[Web Components 邊界](../extras/web-components.md)了解另一個瀏覽器專屬介面。
