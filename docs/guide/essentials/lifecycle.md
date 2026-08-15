# 元件生命週期

有些工作要等 component 首次畫出才執行，例如設定焦點；subscription 則應在 visual host 掛載時建立並隨 host 拆除。`component!` 提供 `mounted`、`updated` 與 `unmounted` 三個原生視覺生命週期 section。

## 宣告 hooks

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#lifecycle_probe{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#lifecycle_probe -->

::: tip 執行結果
第一次 delegated draw 後，`mounted` 將焦點移到 probe 並把計數改為 1。這次通知帶來後續 dirty render；`updated` 在該 render 的 effect cycle 結尾執行。probe 從 keyed visual tree 消失時，`unmounted` 最多執行一次。
:::

## 精確時序

`mounted` 與 `updated` 會在相關 delegated draw 完成後透過 `Window::defer` 排入目前 GPUI effect cycle 結尾。它們不是 DOM insertion / browser paint hook。nested 元件的 mount/update 順序是 child 先於 parent；已排程期間的多次 dirty draw 會合併。

`unmounted` 在一個已畫出的 keyed visual host 消失後執行，並先使尚未執行的 mount/update callback 失效。`v-show` 只隱藏 intrinsic，不會卸載。改變 component `:key` 會選出新 mount。

## Entity lifetime 不等於 visual lifetime

保存 `Entity<Component>` 會延長 entity lifetime，但 component 可以已不在 element tree。gpui-vue 的 visual mount state 獨立追蹤 host 消失，所以有 external owner 時仍能安排 `unmounted`；裸呼叫 `Component::new` / `new_with_slots` 而不掛到 visual host，則不會觸發這三個 hooks。

在應用程式 shutdown 時，刻意由外部持有到最後的 entity 可能來不及 poll deferred unmount task。process-critical 清理應使用明確的應用退出流程；entity resource cleanup 可用 `on_release`。

## Setup 與其他 effects

`setup(this, props, cx)` 在 construction 中執行一次，早於 parent 對 child 安裝 event subscription，因此 setup 中 emit 的 event 可能被錯過。需要 window 的首次工作放在 `mounted`。

`next_frame`、`defer`、`watch_entity(_in)`、`watch_event(_in)` 與 `on_release` 提供更細的 native effect control。

::: warning 目前範圍
沒有 before-mount/before-update、activated/deactivated、error-captured 或 server-prefetch hook。Web DOM timing 與 SSR hooks 不適用於 native backend；其餘可原生表達的 hooks 尚未實作。
:::

## 相關閱讀

- [Watchers 與 Effects](./watchers)
- [條件渲染](./conditional)
- [Template Refs](./template-refs)
- [元件基礎](./component-basics)
