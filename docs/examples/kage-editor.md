# KAGE Editor

KAGE Editor 是 gpui-vue 的完整原生應用範例。它不是 WebView 包裝：工作區、工具列、側欄、對話層、輸入控制項與畫布事件全部進入同一棵 GPUI host tree。

## 執行

在 repository 根目錄執行：

```bash
cargo run --locked -p kage-editor
```

此 package 有獨立的 `Cargo.toml`，因此從應用目錄也可以直接執行：

```bash
cd crates/gpui-vue/examples/kage_editor
cargo run --locked
```

<NativeResult
  src="/screenshots/kage-editor.png"
  alt="實際執行 KAGE Editor example 的原生 macOS 工作區"
  caption="這不是設計稿：截圖由 repository 內的 kage-editor package 編譯、啟動後擷取。"
/>

應用的 bootstrap 同樣直接取自可執行來源：

<<< ../../crates/gpui-vue/examples/kage_editor/src/main.rs#kage_editor_main{rust}

<!-- verified: crates/gpui-vue/examples/kage_editor/src/main.rs#kage_editor_main -->

## 它展示了什麼？

- 200 × 200 KAGE 設計座標與精確向量畫布；
- 直接、多選、框選、控制點拖曳與八方向 resize；
- undo / redo、剪貼簿、KAGE 原文檢視與一次複製全部；
- Mincho / Gothic 呈現、中心線、網格、knockout 與圓滑筆畫；
- macOS 觸控板 pinch zoom 與 modifier-wheel fallback；
- 原生單行文字輸入與中文、日文、韓文 IME composition；
- GlyphWiki 搜尋、50 個即時隨機部件與遞迴相依載入；
- typed props、component events、slots、visual lifecycle 與多語介面。

## 應用邊界

一般 UI 由 `component!` 與 `view!` 描述。需要高精度兩階段繪製的畫布使用 `gpui_vue::paint::drawing_surface`，網路 contract 來自 `gpui_vue::http`，視窗與 assets 則由 `gpui_vue::desktop` 管理。

這種分層讓應用保持原生能力，而不必讓 ordinary panel code 到處依賴低層 GPUI 路徑：

```text
component! / view!
        │
        ├── ui       常用原生型別與剪貼簿
        ├── paint    精確畫布
        ├── http     GlyphWiki transport contract
        └── desktop  視窗、assets 與 root mount
```

## GlyphWiki 資料

部件面板啟動時向 GlyphWiki 取得 50 個隨機結果，也可以手動刷新或搜尋。點選部件後才會下載其 KAGE 原文與完整遞迴相依；應用程式不內建一份近似、容易過期的預設部件庫。

網路失敗不會破壞目前文件。搜尋與部件載入都有明確狀態，較舊的非同步結果也會由 generation token 丟棄。

## 匯出

Export KAGE 會開啟可捲動、帶行號的原文視圖。緊湊來源中的 `$` 會轉成換行並移除，畫面內容與 **Copy all** 放入剪貼簿的內容保持一致。範例刻意不實作 GlyphWiki 登入或提交。

## 打包 macOS 應用

安裝 `cargo-bundle` 後，在 repository 根目錄執行：

```bash
cargo bundle --release --manifest-path crates/gpui-vue/examples/kage_editor/Cargo.toml
```

產生的 `.app` 位於 `target/release/bundle/osx/`。package 內的 1× / 2× 圖示會用於 application bundle。

::: tip 閱讀順序
先從 `src/main.rs` 的 `KageEditor` component 看整體狀態，再依序閱讀 `canvas.rs`、`model.rs`、`glyphwiki.rs`。畫布、資料模型與網路邊界彼此分離，比從單一 render 函式追蹤整個應用更容易。
:::
