# 進入與離開轉場

狀態切換若完全瞬移，使用者不容易辨認哪個面板剛出現。桌面 UI 的轉場也應短而克制；它需要配合 GPUI 的 frame 排程，而不是套用瀏覽器 CSS class。

## 目前可用的原生動畫

`gpui-vue` 尚未實作宣告式 `<Transition>`，但 `animation` module 已把 GPUI 的 keyed `Animation`、`AnimationElement`、`AnimationExt` 與 easing functions 收進正式 curated surface：

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#native_fade_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#native_fade_demo{rust}

::: tip 執行結果
畫面出現一個 176×44 的藍色區塊，首次掛載時在 180ms 內依 `ease_in_out` 由透明變為不透明。動畫 ID 穩定，因此 GPUI 能保留該動畫的時間狀態。
:::

## 離場為何更難

`v-if` 為 `false` 時，element 當幀就不存在；要播放離場動畫，框架必須暫時保留舊 subtree，等動畫完成才卸載。現在沒有這層 transition host。`v-show` 保留的是語意上的顯示切換，但也沒有自動插入過渡時間軸。

目前可由擁有狀態的元件建立 `Visible → Leaving → Hidden` 狀態機，在 `Leaving` 期間繼續 render，再由 timer/task 轉為 `Hidden`。這是可行的原生模式，但必須自行處理取消、快速反向與 reduced-motion 設定。

## 能力缺口

尚缺 `<Transition>`、enter/leave hooks、模式協調與共用的 reduced-motion policy。`animation` module 解決單一 retained element 的時間軸，不會假裝已處理 subtree 暫存或離場完成後卸載。

多個項目的移動與移除請接著閱讀[列表轉場](./transition-group.md)。
