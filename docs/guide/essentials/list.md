# 列表渲染

`v-for` 依序消費任何 Rust `IntoIterator`。與允許無 key patch 的 Web renderer 不同，gpui-vue 要求每個 loop root 都有資料衍生的動態 `:key`，避免焦點與 retained element state 跟錯項目。

## 渲染一組資料

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#layer_list{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#layer_list -->

::: tip 執行結果
fixture 依序迭代 `(id, name)` 陣列，得到「背景 #01」、「輪廓 #02」與「節點 #03」三列由上到下排列的深色 row。每列的 retained identity 來自 tuple 中的 `id`，重新排序不會讓 identity 跟著位置走。
:::

<NativeResult
  src="/screenshots/gallery-lists.png"
  alt="單獨執行的 LayerList，顯示背景、輪廓與節點三列 keyed list"
  caption="截圖中的列順序與上方 fixture 的資料順序一致。"
/>

`v-for={pattern in expression}` 的兩側都是 Rust 語義：左側可使用 pattern，右側必須產生可迭代值。沒有 JavaScript object enumeration，也不接受 `of` 拼寫。

## 索引與解構

需要 index 時，在 Rust 端使用 `enumerate()`：

```rust
use gpui_vue::prelude::*;

fn numbered(names: Vec<String>) -> impl IntoElement {
    view! {
        <div>
            <text
                v-for={(index, name) in names.into_iter().enumerate()}
                :key={("name", index)}
            >
                {format!("{} · {name}", index + 1)}
            </text>
        </div>
    }
}
```

tuple key 是合法的 GPUI identity，key 不限於 Web Vue 常見的字串或數字。真正會重排的資料應優先使用資料庫 id 等穩定 identity，不要使用會跟著位置改變的 index。

## 在元件狀態中迭代

render 只有對 component 的可變借用。若 collection 仍需留在 state，可先 clone，或建立擁有值的 iterator：

```rust
use gpui_vue::Local;

fn prepare_rows() {
    let layers = Local::new(vec![String::from("背景"), String::from("筆畫")]);
    let render_items = layers.get();
    for name in render_items {
        let _ = name;
    }
}
```

大型 collection 不適合每幀整體 clone。此時可改用 `gpui_vue::virtual_list`：可變高度資料使用 `list` / `ListState`，等高資料使用 `uniform_list` / `UniformListScrollHandle`，讓 host 只建立可見範圍。一般 `v-for` 仍適合數量小、每列都需要直接參與模板結構的清單。

## 條件與重複 fragment

在 loop 前篩選通常最清楚：

例如可先寫 `let visible = layers.into_iter().filter(|layer| !layer.name.is_empty());`，再把 `visible` 傳給 `v-for`。

`<template v-for>` 尚未實作。當前 GPUI host 無法替 wrapper-free repeated fragment 指派可靠 identity；請把 `v-for` 與 `:key` 放到一個真實 child element 或 PascalCase component 上。

::: warning Key 是必要條件
literal `key="row"` 與 `:key={1}` 都不能區分不同項目，巨集會拒絕。key 必須是 dynamic、且實際由 loop item 衍生；型別能否轉成 GPUI identity 由 rustc 最終檢查。
:::

## 相關閱讀

- [條件渲染](./conditional)
- [模板語法](./template-syntax)
- [元件基礎](./component-basics)
