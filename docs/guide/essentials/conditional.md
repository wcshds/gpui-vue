# 條件渲染

桌面工具常需要在 loading、empty 與 ready 畫面間切換。gpui-vue 提供 `v-if` 條件鏈與 intrinsic-only 的 `v-show`；兩者都接受 Rust `bool`，但對 visual identity 的影響不同。

## `v-if` 條件鏈

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#result_panel{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#result_panel -->

::: tip 執行結果
gallery 依序展示三個狀態：Loading 只顯示「正在載入…」，Empty 只顯示「沒有結果」，Ready 只顯示綠色「可以開始編輯」。每個呼叫只會產生一個 branch。
:::

`v-else-if` 與 `v-else` 必須緊接前一個 branch。插入其他 sibling、單獨使用 `v-else`、在同一 element 重複條件或替 `v-else` 給值，都會產生編譯錯誤。

condition 不套用 JavaScript truthiness。`Option<T>`、整數與字串不能直接當條件，請明確寫成 `value.is_some()`、`count > 0` 等 `bool` 表達式。

## 一次控制多個 node

structural `<template>` 可以把條件放在一組 sibling 外：

```rust
use gpui_vue::prelude::*;

fn toolbar(can_edit: bool) -> impl IntoElement {
    view! {
        <div class="flex gap-2">
            <template v-if={can_edit}>
                <button id="save">"儲存"</button>
                <button id="export">"匯出"</button>
            </template>
            <text v-else>"唯讀模式"</text>
        </div>
    }
}
```

`<template>` 本身不產生 child element，因此不能帶 `class`、`v-show` 或互動事件。

## `v-show`

```rust
use gpui_vue::prelude::*;

fn inspector(visible: bool) -> impl IntoElement {
    view! {
        <div v-show={visible} class="w-72 bg-slate-900">
            "Inspector"
        </div>
    }
}
```

`v-show` 永遠建立這個 intrinsic，再於條件為 false 時套用 GPUI `hidden()`。它適合頻繁切換、又希望保留同一 host identity 的畫面。它不會觸發 child component 的 `unmounted`。

PascalCase component 與 `<template>` 不接受 `v-show`：component host 刻意不增加 layout wrapper，沒有可隱藏的中介 element。需要時請以 intrinsic wrapper 包住 component。

## `v-if` 與元件身份

`v-if` 為 false 時，該 branch 不進入 element tree。重新出現的 keyed component 會按 GPUI element state 規則重新掛載；changing `:key` 也會選擇新的 mount。這正是需要停止 subscription 或重建區域狀態時應使用的語義。

同一 node 同時有 `v-if` 和 `v-for` 時，條件包在迴圈外，因而不能引用 loop pattern。更清楚的做法通常是先在 Rust 中篩選 iterator，或把條件放到迴圈內的 child。

## 相關閱讀

- [列表渲染](./list)
- [生命週期](./lifecycle)
- [模板語法](./template-syntax)
