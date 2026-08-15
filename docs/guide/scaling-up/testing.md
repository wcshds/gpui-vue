# 測試

UI 測試若只比較截圖，容易漏掉事件型別、生命週期與 identity 錯誤；若只測 model，又可能讓 template 範例早已無法編譯。有效的測試組合應從純狀態一路覆蓋到桌面 smoke test。

## 純狀態測試

Entity model 的 mutation 與 notification 應留在同一個 typed helper，便於在 app context 測試觀察者：

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#notification_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#notification_demo{rust}

::: tip 執行結果
`rename_document` 把同一個 `DocumentModel` entity 的 title 改為「Edited glyph」，然後呼叫 entity context 的 `notify`；透過 `watch_entity` 訂閱的 owner 會在原生 effect cycle 收到一次 model notification。
:::

## 編譯期契約

對 `view!` 和 `component!` 使用 integration tests 驗證合法程式，並以 compile-fail golden cases 驗證錯誤：缺少 required prop、不穩定 `v-for`、未知 utility 或事件 signature 都應在編譯期被拒絕。

## Native 行為

IME、focus、window、pointer gesture 和 paint surface 需要 desktop/backend 測試。測試輸入法時以 UTF-16 range 與 marked-text composition 為契約，不要只發送 ASCII key-down。平台 smoke test 應少而明確，將大部分規則留給 headless tests。

## 文件也是測試目標

本文件的核心片段標向 `examples/docs_gallery.rs`；gallery 必須由 Cargo 編譯並可開窗查看，VitePress 則以 dead-link build 驗證資訊架構。發布 gate 見[正式部署](../best-practices/production-deployment.md)。

需要逐項核對 source 與畫面時，直接啟動 gallery 的單 fixture 模式，例如：

```bash
cargo run --locked -p gpui-vue --example docs_gallery --features desktop -- local-counter
cargo run --locked -p gpui-vue --example docs_gallery --features desktop -- search-panel
```

可用名稱為 `local-counter`、`status-view`、`component-card`、`layer-list`、`search-panel` 與 `registration`。這些模式掛載的就是 Guide include 的元件，不維護第二份 screenshot-only UI；因此截圖和人工 smoke test 都應從這些入口取得。
