# 外掛與應用初始化

鍵盤動作、全域主題、資產與 HTTP transport 通常只應安裝一次。把這些設定散落在根元件的 render 中，會造成重複註冊，且難以控制啟動順序。

## 使用 `AppPlugin` 與 `DesktopApp::plugin`

啟用 `desktop` feature 後，可用 setup callback 建立原生應用擴充點：

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#theme_global_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#theme_global_demo{rust}

::: tip 執行結果
plugin 在首個視窗建立前安裝 typed theme global；gallery root 隨後讀到同一份設定並顯示目前主題。安裝函式不會因 component render 重複執行。
:::

`AppPlugin::install` 會在平台啟動後、首個視窗建立前，依註冊順序各執行一次。普通 `FnOnce(&mut App)` 已 blanket-implement `AppPlugin`；大型套件也可用具名型別實作 trait。`.assets(...)`、`.http_client(...)` 與 `.quit_mode(...)` 則分別處理資產、傳輸和退出政策。

## 封裝一組安裝步驟

目前最穩定的「外掛」形式是普通函式：

具名函式 `fn install_editor_services(app: &mut App)` 會透過 blanket implementation 成為 `AppPlugin`；自訂 struct 也可直接實作 `AppPlugin::install`，把建構參數與安裝行為包在一起。

函式輸入與副作用都受 Rust 型別檢查，也能由應用自行保證只安裝一次。

## 作用域限制

`AppPlugin` 不提供注入 key 或重複安裝防護；需要冪等性的 plugin 應透過 `state::has_global` 自行檢查。GPUI globals 是正式可用的底層應用狀態機制，但它不是 Vue component-tree 的 provide/inject：global 的作用域是整個 `App`，而非某個子樹。需要 per-window 或 per-document 狀態時，應由 root entity 擁有並透過 props、slots 或 entity handles 傳遞。

外掛不應在每次 render 中註冊 listener；把返回的 `Subscription` 交給具明確生命週期的 owner。下一步可看[正式部署](../best-practices/production-deployment.md)，建立啟動與發布檢查。
