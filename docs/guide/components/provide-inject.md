# 共享依賴與 Application Globals

設定、theme 或服務 client 的壽命若屬整個應用程式，可使用 `gpui_vue::state` 對 GPUI globals 的 typed 包裝。它解決 app-wide dependency，不等同 component subtree 的 provide/inject。

## 安裝與讀取 global

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#app_global_state{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#app_global_state -->

::: tip 執行結果
第一個視窗開啟前，`Theme` 先被安裝；card render 時以 type 取回同一份值，顯示藍色背景的「Application theme」。若未安裝就呼叫 `global`，程式會 panic。
:::

`has_global` / `try_global` 可避免缺值 panic，`default_global` 在不存在時安裝 `Default`。`global_mut` 取得可變值並通知 observers；`watch_global` 回傳必須保留的 `Subscription`。`remove_global` 會移除並回傳該值。

## 與 provide/inject 的差異

Application global 以 Rust type 作 key，整個 `App` 只有一份；它不會沿 component ancestry 查找「最近 provider」，也不能讓兩個 subtree 各自覆寫 Theme。它適合 app preferences、service handles 與全域 registries，不適合需要巢狀 override 的 context。

::: warning 尚未實作
component-scoped `provide` / `inject`、symbol/string keys、optional/default injection、reactive subtree overrides 與 app/component lookup precedence 尚未實作。GPUI global 不應被描述成這些功能的完整替代品。
:::

短距離依賴請使用 props；parent 客製 child 內容使用 slots；獨立長壽命 model 可用 GPUI entity 加 `watch_entity`。這三種 ownership 都比把所有資料放進 global 更容易推理。

## 相關閱讀

- [Props](./props)
- [Slots](./slots)
- [Watchers 與 Effects](../essentials/watchers)
- [應用程式與視窗](../essentials/application)
