# 模板語法

`view!` 讓原生 GPUI element tree 以接近標記語言的方式排列。它不是字串模板：巨集在編譯時解析結構，模板內的值仍是普通 Rust 表達式，錯誤也由 proc macro 或 rustc 指向來源位置。

## 最小模板

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#status_view{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#status_view -->

::: tip 執行結果
範例 gallery 傳入 `label = "Renderer connected"`，並沿用 `online = true` 的預設值；畫面因此顯示綠色狀態點與「Renderer connected」。把 `online` 改為 false 時，狀態點會改用玫紅色樣式。
:::

<NativeResult
  src="/screenshots/gallery-templates.png"
  alt="單獨執行的 StatusView 原生範例，顯示綠色狀態點與 Renderer connected"
  caption="程式碼區塊不是另抄的片段，而是這個原生畫面的編譯來源。"
/>

字串 literal 直接成為文字 child；動態內容只需一層 `{ Rust 表達式 }`。Web Vue 的雙大括號插值拼寫在 gpui-vue 中不成立。

需要讓一個 expression 獨佔 host 內容時，也可使用 `v-text`：

```rust
view! { <span v-text={format!("你好，{name}")} /> }
```

它會把 expression 每次 render 求值一次，再以 GPUI `.child(value)` 加入真實 intrinsic。它不是 HTML 字串 API，也不做 HTML escaping；值遵循 Rust / GPUI 的 child type contract。`v-text` 不能與同一 tag 的任何 child 並存，也不能用於 `img`、`template` 或 PascalCase component。

## 元素與 child

目前 intrinsic tag 是 `div`、`view`、`text`、`span`、`button` 與 `img`。前五者主要降低為原生 container；`button` 額外啟用 pointer、focus 與 tab-stop 慣例。`img` 必須有 `src` 或 `:src`，且不能帶 child：

```rust
use gpui_vue::{media::ObjectFit, prelude::*};

fn icon(source: String) -> impl IntoElement {
    view! {
        <img
            :src={source}
            :object-fit={ObjectFit::Contain}
            :loading={|| view! { <div class="size-full bg-slate-800" /> }.into_any_element()}
            :fallback={|| view! { <text>"圖片無法載入"</text> }.into_any_element()}
            class="w-8 h-8"
        />
    }
}
```

三個 image-only binding 都是 GPUI 原生 typed 值：`:object-fit` 接受 `ObjectFit`，`:loading` 與 `:fallback` 接受 `Fn() -> AnyElement + 'static`。它們不是 CSS/DOM 字串；各 expression 在 render 時依 attribute 來源順序求值一次，兩個 callback 只在 GPUI 需要對應 replacement 時執行。

任意 `IntoElement` 值也能以 `{ element }` 插入。簡單的 PascalCase tag 則走 gpui-vue 產生的元件 host，會保留 child entity 身份。

## 屬性與綁定

- `class="..."`：編譯期 class literal。
- `id="save"` / `:id={id}`：GPUI element identity。
- `key="row"` / `:key={item.id}`：保留 stateful element 或元件的身份。
- `:class={if selected { "..." } else { "..." }}`：分支必須由 literal 組成。
- `:style={|style| ...}`：typed `StyleRefinement` callback。
- `v-text={expression}`：唯一的 typed native child，expression 每次 render 求值一次。
- `@click={handler}` 或 `on:click={handler}`：原生事件 listener。

同名 dynamic shorthand 也可使用，例如已有 `id` 變數時寫 `:id`。PascalCase 元件的 kebab-case prop 會正規化成 snake_case builder method。

## Rust 表達式，不是 JavaScript

條件必須是 `bool`，迴圈來源必須實作 `IntoIterator`，closure capture 也遵循 Rust 所有權與 `'static` 要求。模板不會提供 JavaScript truthiness、proxy unwrap 或執行期 expression evaluator。

## 多個 root 與 `<template>`

多 root 和 `<>...</>` root 都能編譯；由於目前 GPUI render 邊界需要單一 element，它們會包進一個 synthetic `div`，這層 wrapper 可能參與 layout。巢狀 fragment 會攤平。

`<template v-if={...}>` 可在不新增 child element 的前提下條件化多個同層 node。`<template v-for>` 尚不支援，因為目前無法在不改變 layout 的情況下為 repeated fragment 提供 GPUI identity。

::: tip 編譯期限制是設計的一部分
任意執行期 class 字串、未知 intrinsic、無 key 的 `v-for` 與不完整的條件鏈會直接編譯失敗，避免把 identity 或樣式錯誤留到使用者操作時才出現。
:::

## 相關閱讀

- [條件渲染](./conditional)
- [列表渲染](./list)
- [Class 與 Style](./class-and-style)
- [元件基礎](./component-basics)
