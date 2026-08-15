# 無障礙

鍵盤、focus、對比與輔助技術語意是專業桌面工具的基本功能，不是最後加上的視覺選項。原生 renderer 也不能靠 HTML element 名稱自動獲得完整 accessibility tree。

## 先讓核心操作可用鍵盤完成

內建 `button` intrinsic 會建立 pointer interaction、tab stop、focus 與鍵盤 activation；互動 host 必須有穩定 `id`。原生文字輸入則應使用 `TextInput`，以便 IME、selection 和平台 input handler 正常工作。

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#keyboard_action_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#keyboard_action_demo{rust}

::: tip 執行結果
按 Tab 可讓 Save button 取得 focus，Enter／Space 可走原生 button activation；focus 狀態出現可見邊框，而不只依靠 hover。
:::

## Focus 是應用狀態的一部分

對自訂互動區使用 `:track-focus={&focus_handle}` 與 `key-context`，開啟 modal 時主動移入 focus，關閉後恢復先前目標。`occlude` 只能阻止 pointer 穿透，並不會自動建立 focus trap。

## 目前缺口

`gpui-vue` 尚未提供完整 role/state/property DSL、標籤關聯、live region、accessibility tree 檢查器或 reduced-motion/high-contrast policy。當前 `button` 也明確不是完整平台 accessibility-role abstraction。這些不是 Web-only；它們是仍需補齊的 native 能力。

在能力完成前，重要產品應直接核對 GPUI 底層 accessibility 支援並以 VoiceOver、Narrator 或 Orca 做實機測試。不要用 icon glyph 作為唯一標籤，也不要只靠顏色區分狀態。動畫政策見[動畫](../extras/animation.md)。
