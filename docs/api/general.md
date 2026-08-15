# General API

gpui-vue 的日常入口是兩個 procedural macro。它們在編譯期產生普通 Rust 與 GPUI builders，不建立第二棵 runtime tree。

## `view!`

```rust
pub use gpui_vue_macros::view;
```

接受 Vue-shaped Rust markup，回傳實作 `IntoElement` 的 native element。Rust expression 放在單層 `{ ... }`；沒有 Vue 的雙大括號 JavaScript interpolation。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#inline_panel_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#inline_panel_demo -->

支援的結構包括：

- intrinsic `div`、`view`、`text`、`span`、`button`、`img`；
- PascalCase generated components；
- `v-if` / `v-else-if` / `v-else`、intrinsic `v-show`；
- 必須帶 item-derived `:key` 的 `v-for`；
- fragments 與 structural `<template>`；
- literal `class`、compile-time `:class` branches、typed `:style`；
- curated native events，以及 typed drag source/drop lanes，見 [Built-in Directives](/api/built-in-directives)。

未知 tag、attribute、directive、class utility 或不穩定的 interactive identity 會在編譯期報錯。

## `component!`

```rust
pub use gpui_vue_macros::component;
```

宣告 documented component，以及可選 `props`、`state`、`setup`、`emits`、`slots`、lifecycle 與 `template` sections。產物包含 component struct、props/input/builder、需要時的 event enum 與 slots struct，以及 native trait implementations。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_stepper{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_stepper -->

## `prelude`

```rust
use gpui_vue::prelude::*;
```

prelude 匯入 macros、component traits、`Local` / `Memo` / `Ref`、`AsyncResource` / `AsyncState`、`spawn` / `spawn_in`、`EffectScope`、global traits、slots、`TextInput` / config / style / binding、overlay constructors與 `OverlayCorner` / `OverlayInsets`，以及 GPUI prelude。`AsyncContext`、`WeakOwner`、其餘 effect helpers、完整 overlay policy types，以及 `desktop`、`ui`、`paint`、`media`、`virtual_list`、`http`、`animation` 的完整名稱都不由 prelude 展開；請從 crate root 或相應模組明確匯入。

## `gpui_vue::gpui`

完整 host re-export 是進階 escape hatch。已有 `ui`、`paint`、`http`、`animation`、`async_state`、`overlay` 或 `desktop` bridge 時，應依賴 curated module；需要尚未包裝的 GPUI 能力時才使用 re-export，並接受其版本與 host contract。

## 另見

- [Component Setup](/api/component-setup)
- [Built-in Special Elements](/api/built-in-special-elements)
- [Render Function](/api/render-function)
