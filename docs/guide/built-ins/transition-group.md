# 列表轉場

插入、刪除或排序一列資料時，穩定身份比動畫本身更重要。身份錯誤會讓 focus、hover 與 element state 跟著索引移到另一筆資料。

## 先建立正確的 keyed 列表

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#keyed_queue_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#keyed_queue_demo{rust}

::: tip 執行結果
每筆 queue item 顯示為獨立的深色列。重新排序輸入時，`row.0` 而非迴圈位置決定 GPUI identity；同一筆資料保有自己的 retained element state。
:::

## 目前沒有 FLIP 協調器

`gpui-vue` 尚未實作 `<TransitionGroup>`。框架不會自動記錄每個 child 的舊 bounds、計算位移，再把新 layout 動畫回原位；刪除項目也會立即離開 `v-for` subtree。

底層安全做法是讓列表 owner 保存「舊位置、目標位置、開始時間」，以資料 ID 為鍵計算插值，並使用 `gpui_vue::animation` 或 native next-frame scheduling 驅動重繪。若列表可捲動或虛擬化，bounds 必須在同一個座標空間處理。

## 互動準則

- `:key` 取自資料的穩定 ID，不要取 enumerate index。
- 先保證 selection、focus 與 event subscription 在無動畫時正確。
- 移除動畫期間保留資料的 tombstone，而不是把已移除 row 借回主集合。
- 大量列表應優先使用 `gpui_vue::virtual_list`（等高 row 選 `uniform_list`，可變高度選 `list` + `ListState`）；不要為看不見的行建立 animation state。

現在可將個別 row 包在 `animation::AnimationExt` 中做出現效果，但它不等同 layout-aware 的列表轉場。單一 subtree 的轉場邊界見[進入與離開轉場](./transition.md)。
