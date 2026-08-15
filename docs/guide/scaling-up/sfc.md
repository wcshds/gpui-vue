# Rust 單檔元件

當 template、props、事件與 state 分散在多個 builder 中，元件的公開契約不容易一次看清。`component!` 把這些部分寫在同一個 Rust item，並在編譯期展開成普通型別與原生 GPUI `Render`。

## 一個完整的小元件

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#status_badge -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#status_badge{rust}

::: tip 執行結果
`StatusBadge` 把 required `SharedString` label 顯示為綠色圓角 badge。tag、props 與 class 都在 macro expansion 時降為 typed GPUI calls，執行期不解析 SFC 或 CSS 字串。
:::

## 它不是 `.vue` 檔解析器

這裡的「單檔」指一個 Rust source item 封裝元件，而非支援 `<script setup>`、`<style scoped>` 或 JavaScript expression。macro 可包含 `props`、`state`、`emits`、`slots`、一次性 `setup`、三個 visual lifecycle 區段與 `template`。產生的 props builder、event enum 和 component type 都能被 rust-analyzer 看見。

template 可直接寫 markup，也可寫 Rust block 回傳 `IntoElement`。外部 helper 與 model 仍應放在普通 module，避免把所有領域邏輯塞進 macro。

## 限制

目前沒有外部 SFC compiler、scoped CSS、HMR state preservation 或 language-server plugin。靜態 class 由內建 Tailwind-like 編譯器檢查；動態數值樣式用 typed `:style` callback。下一步看[工具鏈](./tooling.md)建立開發迴圈。
