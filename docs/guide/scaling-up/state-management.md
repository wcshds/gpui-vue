# 狀態管理

狀態放得太低，兄弟元件必須互相繞路；放得太高，又會讓每次小修改都重繪整個應用。`gpui-vue` 提供三種不同所有權模型，不用一個通用 store 包辦所有情況。

## 依生命週期選容器

| 需求 | 建議 | 通知模型 |
| --- | --- | --- |
| 單一 component 擁有 | `Local<T>` | 修改時傳入 `cx` |
| 少數 owner 共用 cell | `Ref<T>` | 只通知當次傳入者 |
| 多視圖觀察 model | GPUI `Entity<T>` + `watch_entity` | entity notification stream |
| 整個應用唯一設定 | `state::Global` helpers | global observers |

## 應用級設定

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#theme_global_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#theme_global_demo{rust}

::: tip 執行結果
plugin/setup 執行 `install_gallery_theme` 後，`gallery_is_dark(app)` 讀取同一個 typed global 並回傳 `true`；型別 `GalleryTheme` 本身就是 key，不需要字串 registry。
:::

更新 global 可用 `global_mut`，觀察替換或 mutable access 可用 `watch_global`；返回的 `Subscription` 必須由 owner 保存。文件或大型資料模型通常更適合 entity，因為它有獨立身份與通知流。

## 避免隱藏依賴

Global 是 app-wide，不是 component-tree provide/inject。可重用元件優先從 props 接收資料或 entity handle；只有真正全域的 theme、locale、服務 registry 才放 global。`Ref<T>` 也不是自動 store：clone 的讀者不會自動訂閱。

需要衍生資料時，使用 `Memo<T, D>` 配明確 revision key；深入機制見[反應性深入](../extras/reactivity-in-depth.md)。
