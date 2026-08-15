# 效能

`gpui-vue` 已經省去 JavaScript runtime、VDOM diff 與執行期 class parser，但原生並不等於自動快速。大型 clone、過寬通知範圍與每幀重建 I/O 仍會造成卡頓。

## 先縮小狀態與失效範圍

component-local 狀態用 inline `Local<T>`；跨視圖 model 用獨立 entity，讓真正的讀者透過 `watch_entity` 失效。不要把整個 workspace model 放入一個 `Ref`，再把同一個 `cx` 通知當作全域 re-render bus。

## 以 revision 快取昂貴衍生值

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#memo_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#memo_demo{rust}

::: tip 執行結果
第一次呼叫依 `count` 產生例如 `Total: 3`；dependency revision 未變時再次取得同一個 `String` cache，不會重跑 formatting closure。count 有效更新後 revision 改變，下一次才重算。
:::

`Memo` 不收集依賴，dependency key 必須完整；多個 `Local` 可用 revision tuple。`Ref::update` 會 clone 舊值來比較，對大型集合應考慮小粒度 refs、entity model 或能建構 replacement 的 `Local::update`。

## Render hot path

- template 中不要啟動網路或檔案 I/O；render 可能重跑。
- 大量 list 使用穩定資料 ID，並採用正式 `gpui_vue::virtual_list` bridge：等高 row 用 `uniform_list`／`UniformListScrollHandle`，不同高度用 `list` 與 retained `ListState`。
- 靜態 `class` 和可列舉 `:class` 分支由 macro 預編譯；不要自行建立 runtime class parser。
- precision canvas 走 `paint::drawing_surface`，把 layout/prepaint 結果直接交給 paint。

## 量測而非猜測

目前沒有 gpui-vue 專用 profiler。使用平台 sampling profiler與 GPUI frame instrumentation，分辨 domain 計算、layout、text shaping、image/network 與 paint。優化後保留能重現資料量的 benchmark 或 example。

目前沒有把普通 `v-for` 自動改寫成 virtual list 的 directive；virtualization 會改變 mount 範圍、測量與 scroll API，應由呼叫端明確選擇 `list` 或 `uniform_list`，而不是根據資料筆數暗中切換。
