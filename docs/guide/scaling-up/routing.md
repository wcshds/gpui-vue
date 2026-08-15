# 桌面路由

桌面應用的「頁面」通常是 workspace mode、文件 tab 或設定 pane；它不一定有 URL，也不應假設存在瀏覽器 history。

## 用 enum 建模可到達的畫面

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#route_view_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#route_view_demo{rust}

::: tip 執行結果
`Route::Dashboard` 與 `Route::Settings` 分別得到固定 label。root template 可用同一個 exhaustive `match` 選擇 pane；新增 enum variant 時，rustc 會要求更新匹配，避免字串路徑拼錯。
:::

## 把 navigation 當作狀態轉移

root component 可用 `Local<Route>` 保存當前路由；toolbar click、command palette 與快捷鍵最後都呼叫同一個 `navigate(next, cx)`。若需要 Back/Forward，保存有上限的 route stack，並把不可序列化的 entity handle 放在 route 之外。

## 接上平台 URL 與 reopen

`DesktopApp` 已有 native lifecycle hooks：

```rust
let app = DesktopApp::new(window)
    .register_url_scheme("gpui-vue")
    .on_open_urls(|urls| {
        for url in urls {
            eprintln!("open request: {url}");
        }
    })
    .on_reopen(|app| {
        // 若沒有合適視窗，可在此呼叫 open_window / open_component_window。
        let _ = app;
    });
```

`on_open_urls` 可能在 app 已執行時被多次呼叫；callback 應先 parse/validate scheme、host 與 payload，再送入自己的 navigation model。`register_url_scheme` 是平台請求，不保證每個 OS 都支援 runtime registration。`on_reopen` 的觸發條件也由平台決定，例如 macOS Dock reopen。

多視窗 navigation 可用正式 helper 建立：

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#desktop_window_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#desktop_window_demo -->

## 仍然沒有高階 Router

目前沒有 route table、nested outlet、navigation guard、document restoration 或 URL-to-route mapper。瀏覽器 `history.pushState`、hash routing 與 scroll restoration 屬於 Web-only 語意，不會照搬。

enum + root state 仍是型別安全且可測試的核心；平台 callbacks 只是 native entry points，不會替應用定義 route 語意。共享 route 與 document models 的安排見[狀態管理](./state-management.md)。
