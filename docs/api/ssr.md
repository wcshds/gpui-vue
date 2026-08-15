# Server-Side Rendering Boundary

gpui-vue 不產生 HTML，也沒有 DOM hydration。GPUI application在本機 event loop 中建立 native window、layout 與 paint，因此 Vue SSR APIs 對此 backend 不成立。

## 不提供的 API

沒有 `renderToString`、`renderToNodeStream`、`renderToWebStream`、SSR app、hydration、teleport HTML collection 或 `serverPrefetch`。也沒有 browser/server 雙 bundle與 hydration mismatch diagnostics。

## 原生程式的相應問題

| SSR 需求 | Native 應用做法 |
| --- | --- |
| 初始資料 | 在 `DesktopApp::setup` 安裝 state/service，或以 owner-held `AsyncResource` 載入並顯示 state |
| 首屏資產 | `EmbeddedAssets` 打包，HTTP client處理 remote resource |
| 延後工作 | `spawn` / `spawn_in`、owned `Task`、`next_frame`、entity notification |
| headless output | 建立專用 renderer/export pipeline，而非假裝輸出 HTML |

資料 loading 本身是可用能力；只有 HTML serialization/hydration 是 Web-only。不要把「沒有 SSR」誤讀成 native app 不能預載資料。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

## 另見

- [SSR 指南](/guide/scaling-up/ssr)
- [Async Helpers](/api/composition-api-helpers)
- [Application API](/api/application)
