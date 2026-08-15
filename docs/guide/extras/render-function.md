# 渲染函式

當一段 UI 由演算法產生，硬塞成巢狀 markup 反而不易閱讀。`view!` 可以和普通 Rust 函式、iterator 與 native builder 混用；沒有另一套 `h()` vnode API 要學。

## 回傳一個 typed element

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#render_helper_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#render_helper_demo{rust}

::: tip 執行結果
`empty_state` 接受任何能轉成 `SharedString` 的 label，並產生置中的空狀態 panel；字串作為普通 Rust value 顯示，沒有 vnode 或 runtime template。
:::

若 helper 還要依布林值切換 class，應直接寫 `:class={if enabled { "text-emerald-300" } else { "text-slate-500" }}`；兩個 literal 都會在編譯期驗證。把分支先存成任意 runtime `&str` 則會被 macro 拒絕。

## 何時使用 `ui::div()`

需要由 iterator builder、泛型 adapter 或 GPUI extension trait 組合 element 時，使用 `gpui_vue::ui::div()`；相關 `Styled`／`ParentElement` traits 目前可由 prelude 或 `gpui_vue::gpui` 引入。這是同一個 native element，不是逃離 gpui-vue renderer。

## Type erasure

優先回傳 `impl IntoElement`。只有 collection 或 API boundary 確實要容納不同 concrete element types 時才使用 `AnyElement`；過早 erase 會讓 compiler 無法內聯並降低型別診斷品質。

render 中不應啟動 task、註冊 subscription 或改 global；那些副作用應放在 setup、event、lifecycle 或明確 effect helper。內部流程可參考[渲染機制](./rendering-mechanism.md)。
