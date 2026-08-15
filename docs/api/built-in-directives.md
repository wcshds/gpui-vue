# Built-in Directives

gpui-vue directives 是 `view!` 在編譯期辨識的 structural bindings。值必須是 Rust expression；沒有 runtime directive registry。

## `v-if` / `v-else-if` / `v-else`

```rust
<text v-if={ready}>"Ready"</text>
<text v-else-if={loading}>"Loading"</text>
<text v-else>"Idle"</text>
```

相鄰 sibling chain 會 lower 為 native conditional builders。expression 必須 type-check 為 `bool`，不使用 JavaScript truthiness。支援 intrinsic、PascalCase component 與 structural `<template>`。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#result_panel{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#result_panel -->

## `v-show`

```rust
<div v-show={visible}>"Inspector"</div>
```

只支援有真實 host 的 intrinsic。值為 false 時套用 native `hidden()`，element identity 仍存在，因此不觸發 component unmount。PascalCase component host 與 `<template>` 沒有 layout wrapper，故拒絕 `v-show`。

## `v-text`

```rust
<span v-text={format!("{name} · {count}")} />
```

`v-text={expression}` 只支援可接收 child 的真實 intrinsic：`div`、`view`、`text`、`span` 與 `button`。expression 每次 render **只求值一次**，保存到區域變數後以原生 `.child(value)` 加入 host；值因此必須符合 GPUI `ParentElement::child` 的 Rust / `IntoElement` 契約。它不是 HTML 字串，也不做 HTML escaping 或 parser insertion。

與 Vue 一樣，`v-text` 取代 host 的 child-content lane，所以同一 tag 不能再包含 literal、`{expression}`、fragment 或 element children。`<img>`、structural `<template>`、`<slot>` 與 PascalCase component 都會得到 targeted compile error；component 的內容應使用 typed props 或 slots。

`v-text` 可與 intrinsic 的 `v-if`、`v-else-if`、`v-else`、`v-show` 與 keyed `v-for` 組合。macro 先遵循固定 native builder order（style、identity、focus/context、retained state），再依 attribute source order 安裝 listener、drag/drop 與 `v-text` expression；因此不宣稱 style / identity expression 和 `v-text` 之間存在 HTML attribute evaluation order。

## `v-for`

```rust
<div v-for={(id, label) in rows} :key={("row", id)}>{label}</div>
```

右側接受任何 Rust `IntoIterator`；左側是 Rust pattern。每個 loop root 都必須有**非 literal**、由 item 推導的 dynamic `:key`，避免 stateful descendant identity 混淆。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#layer_list{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#layer_list -->

`<template v-for>` 目前是編譯錯誤：GPUI host 無法在不插入 layout wrapper 的情況替 repeated fragment 配置 identity。請把 loop 放在一個 real child。

## Event bindings

`@event={handler}` 與 `on:event={handler}` 是 aliases。intrinsic 目前支援：

- `click`；
- `key-down`、`key-up`、`modifiers-changed`；
- `mouse-down`、`mouse-down-out`、`mouse-move`、`mouse-up`、`mouse-up-out`；
- `pinch`、`scroll-wheel`、`hover`；
- typed `drag-move`、`drop`；
- exact-handle `focus`、`blur`。

mouse down/up variants 必須指定 `.left`、`.right` 或 `.middle`。focus/blur 必須同時提供 `:track-focus={&handle}`。每個 interactive host 都需要 `id`；loop 內由 root key namespace identity。

`click` 支援 `.stop`、`.prevent`、`.ctrl`、`.alt`、`.shift`、`.meta` 與 `.exact`。其他 intrinsic event（包含 `drag-move` / `drop`）不接受 modifiers；component events 也不接受 modifiers。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#focus_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#focus_demo -->

## Drag/drop bindings

drag source 必須在同一個 identified intrinsic 上成對提供：

```rust
<div
    id="drag-source"
    :drag-payload={payload_expression}
    :drag-preview={preview_constructor}
/>

fn preview_constructor(
    payload: &T,
    offset: ScreenPoint,
    window: &mut Window,
    app: &mut App,
) -> Entity<Preview>;
```

`payload_expression` 的型別 `T` 必須是 `'static`；`Preview` 必須實作 `Render + 'static`。payload 會由 native drag 保存；preview constructor 在 drag initiation 建立跟隨 pointer 的 retained entity。兩個 binding 各只能出現一次，缺少配對者或重複宣告都會在 macro expansion 報錯。它們的 source order 可互換；macro 先依原始 attribute 順序評估 expression，取得完整配對後只呼叫一次 native `on_drag`。

drop / move surface 的 contracts 是：

```rust
fn can_drop(payload: &dyn Any, window: &mut Window, app: &mut App) -> bool;
fn drag_over<T>(
    style: StyleRefinement,
    payload: &T,
    window: &mut Window,
    app: &mut App,
) -> StyleRefinement;
fn drag_move<T>(event: &DragMoveEvent<T>, window: &mut Window, app: &mut App);
fn drop<T>(payload: &T, window: &mut Window, app: &mut App);
```

`:can-drop` 每個 host 只有一份，對所有 payload type 先做 type-erased gate；false 同時阻止 matching `:drag-over` style 與 `@drop` dispatch。`:drag-over`、`@drag-move`、`@drop` 是 exact-`T` lanes，可針對不同 payload type 重複。`@drag-move` / `@drop` 與 `on:drag-move` / `on:drop` 是 aliases；同一 type 不應重複註冊相同 lane。

所有 drag/drop bindings 都會進入 stateful native host，因此要求 stable `id` 或 loop `:key`。`ui` 正式匯出 `DragMoveEvent`、`ExternalPaths`、`ScreenPoint`、`StyleRefinement` 及 callback 所需的 window/entity types；`Any` 來自 `std::any::Any`。這不是 DOM `DragEvent` / `DataTransfer` API，也不提供 MIME string negotiation。

## 尚未實作

沒有 `v-model` template attribute、custom directive API 或 `v-html`。`v-html` 是 DOM/HTML parser 語意，不適用原生 GPUI；其他 native-capable items 的狀態見[能力矩陣](/capability-matrix)。

## 另見

- [Event Handling](/guide/essentials/event-handling)
- [Special Attributes](/api/built-in-special-attributes)
