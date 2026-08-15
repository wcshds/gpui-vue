# 動畫

動畫應解釋狀態改變，而不是長時間占用注意力。`gpui_vue::animation` 正式匯出逐 frame 的 keyed `Animation`、element extension trait 與一組 native easing functions；它仍是 typed element API，不是 declarative directive。

## 原生時間軸

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#animation_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#animation_demo{rust}

::: tip 執行結果
一個 32×32 藍色方塊在 35% 到 100% opacity 之間循環；GPUI 在動畫期間要求新 frame，easing 將線性進度映射成平滑節奏。
:::

## Identity 與取消

animation ID 必須在同一視覺角色上穩定。更換 key 或移除 element 就捨棄該 retained animation state；快速切換時不要讓兩個不同操作重用同一 ID。

## 目前缺口

尚缺 template directive、state-to-state tween、離場保留、列表 FLIP、finished callback，以及 app-wide reduced-motion policy。正式 `animation` module 代表可直接使用的底層能力，但不承諾已有高階 transition coordination。

涉及 conditional mount/unmount 的動畫請讀[進入與離開轉場](../built-ins/transition.md)；無障礙產品應讓使用者關閉非必要循環與大幅移動。
