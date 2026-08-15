# Fallthrough Attributes

gpui-vue component host 是透明的 retained entity adapter，不是一個 DOM root element。因此 parent 寫在 PascalCase tag 上的內容只會被當作 declared props、events、slots 或 host `key`；不會把未知 attrs 默默轉交到 child template 的第一個 intrinsic。

## 用 typed props 明確轉交

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#explicit_attrs{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#explicit_attrs -->

::: tip 執行結果
`emphasized` 經 generated bool prop 傳入，child root 顯示藍底白字。若改寫成未宣告的 `class="bg-red-500"`，會在 generated props builder 找不到 setter 而編譯失敗。
:::

這種明確 contract 對 reusable native component 很重要：component 可以決定色彩 token、focus handle、element id 與事件要掛在哪個實際 host，而不必猜測「第一個 root」。

## 需要 wrapper 時由 parent 建立

layout 與 visibility 屬 parent concern 時，像同一 fixture 的 `wrapped_panel` 一樣，把 component 放進 intrinsic。

wrapper 是真正的 GPUI layout node；其寬度、`v-show`、id 與 pointer hitbox 都有清楚歸屬。component host 本身刻意不新增 layout wrapper。

## 目前限制

::: warning 尚未實作
沒有 `$attrs`、`inheritAttrs`、listener fallthrough、任意 attribute bag 或 multi-root attrs forwarding。HTML `aria-*` 與 DOM property/attribute distinction 也不適用；原生 accessibility 需要獨立 typed API，目前尚未補齊。
:::

如果許多元件共享同一種視覺設定，優先建立有語意的 typed prop（如 `tone: Tone`、`density: Density`），而不是把任意 class string當成外部協定。

## 相關閱讀

- [Props](./props)
- [Class 與 Style](../essentials/class-and-style)
- [Slots](./slots)
