# 應用程式與視窗

gpui-vue 元件最後必須掛到一個原生視窗。一般桌面程式可用 `DesktopApp` 完成平台啟動、一次性初始化與根元件掛載，不必在每個專案重寫 GPUI 的 bootstrap。

## 建立第一個應用程式

`desktop` 模組受同名 Cargo feature 控制：

```toml
[dependencies]
gpui-vue = { version = "0.1", features = ["desktop"] }
```

下面是一個完整的單視窗程式：

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#app_root{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#app_root -->

::: tip 執行結果
不帶參數執行時，程式開啟 1180 × 820 的原生視窗，最小尺寸是 900 × 680；透明標題列下方顯示可捲動的「gpui-vue Guide Gallery」。傳入 `local-counter`、`status-view` 等 fixture 名稱時，同一個 binary 只掛載對應的已編譯範例，供 Guide 結果圖與人工 smoke test 使用。
:::

`run_component` 建立根 entity，並替它加上與巢狀元件相同的視覺 host。因此根元件宣告的 `mounted`、`updated` 與 `unmounted` 也會按 gpui-vue 的生命週期規則執行。

## 啟動前設定

`DesktopApp` 採 builder 形式組合應用程式邊界。除了範例中的 `plugin`，也可鏈接 `.setup(|app| { /* 註冊 globals、actions、key bindings 或 menus */ })`。

多個 `setup` callback 會依註冊順序執行。`assets(...)`、`http_client(...)` 與 `quit_mode(...)` 分別安裝資源來源、HTTP transport 與退出政策。這些是應用層能力，不是元件區域狀態。

`plugin(...)` 接受實作 `AppPlugin` 的值；普通 `FnOnce(&mut App)` 已自動實作該 trait。它是可重用的 native app 安裝入口，不是 component registry。

`WindowConfig` 目前可設定初始與最小尺寸、透明標題列、macOS traffic-light 位置、可見/聚焦狀態、視窗種類、移動/縮放/最小化政策、背景材質、桌面 app id 與 macOS tab group。

## 原生啟動模型

gpui-vue 沒有瀏覽器 DOM，也沒有 Vue `createApp()` instance。`DesktopApp` 包裝的是單一 GPUI `Application` 及第一個視窗；排程、entity 與事件迴圈都由 GPUI 持有。

## 開啟其他視窗

`open_window(app, config, builder)` 開啟 raw `Render` entity；`open_component_window` 則替 generated component root 安裝與首個視窗相同的 visual lifecycle host。兩者都套用 `WindowConfig`，建立失敗時回傳 error，不會 panic。

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#desktop_window_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#desktop_window_demo -->

::: tip 執行結果
按下 Open secondary window 會建立另一個原生視窗；其中的 generated component 有自己的 retained root lifecycle，Close window 只關閉該視窗。
:::

`DesktopApp::on_open_urls` 接收平台送來的 URL batches；`on_reopen` 在平台要求重新開啟已執行的 app 時取得 live `App`，可用上述 helper 恢復視窗。`register_url_scheme("gpui-vue")` 請求 runtime scheme registration；支援與登錄政策由作業系統決定，應用仍要驗證收到的 URL。

::: warning 尚未實作
目前沒有 component-subtree `provide` / `inject` 或宣告式 `<Window>` 元件。Application globals 與 `AppPlugin` 已可使用，但它們沒有「最近祖先 provider」語義；多視窗由 typed function 明確建立。
:::

## 相關閱讀

- [元件基礎](./component-basics)
- [生命週期](./lifecycle)
- [快速開始](/guide/quick-start)
- [能力矩陣](/capability-matrix)
