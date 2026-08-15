# 反應性深入

`gpui-vue` 的反應性是一條明確的 native invalidation 路徑：修改資料、通知 owner、GPUI 安排 render。它刻意不在讀取時建立全域 dependency graph。

## `Local<T>`：單一 entity 內的狀態

值與 `Revision` inline 儲存。`set` 比較新舊值；有效變更才推進 revision 並呼叫 notifier。`update` 從 `&T` 產生 replacement，所以不需 clone 舊的大型值。

## `Ref<T>`：共享 cell，不是共享 subscriber list

clone `Ref` 只 clone handle，所有 clone 讀到同一個 `Rc<RefCell<T>>`。`Ref::update` 會 clone 舊值比較；通知對象仍只有呼叫端傳入的 notifier。若 A、B 兩個 component 都讀同一 Ref，A 的 context mutation不會自動讓 B 重繪。

## Entity observer 與 effect scope

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#effect_scope_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#effect_scope_demo{rust}

::: tip 執行結果
`clear_screen_effects` 對 owner 傳入的 scope 執行 `clear`，立即取消其中全部 subscription。真實 component 在 setup/mounted 中把 `watch_entity` 或 `watch_event` 返回值交給 `track`；scope drop 也有相同取消效果。
:::

`watch_entity(_in)` 監聽 notification，`watch_event(_in)` 監聽 typed event；`next_frame` 與 `defer` 提供明確排程，`on_release` 註冊 entity cleanup。它們包裝 GPUI stream，不會在 closure 讀值時新增依賴。

## `Memo<T, D>`：由 key 決定快取

通常以一個 `Revision` 或 tuple 作 `D`。key 相同就回傳 cache，key 不同才執行 closure；如果漏掉一個 dependency，框架無法代為發現 stale value。

## 目前沒有的語意

沒有任意 `watch(source closure)`、deep traversal、flush `pre/post/sync`、automatic computed graph 或 cleanup-on-rerun。Entity watchers 與 `EffectScope` 已覆蓋可明確命名的 native source；Ref dependency tracking 仍是能力缺口。狀態層級選擇見[狀態管理](../scaling-up/state-management.md)。
