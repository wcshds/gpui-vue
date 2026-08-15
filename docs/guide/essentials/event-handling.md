# 事件處理

gpui-vue 的事件直接連到 GPUI listener，不經過 DOM `Event` 或合成事件層。intrinsic listener 的 callback 取得 typed native event、目前 `Window` 與 `App`；在 component 內則通常用 `cx.listener` 把 callback 綁回 component entity。

## 處理點擊

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#click_counter{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#click_counter -->

::: tip 執行結果
按鈕初始顯示「點擊 0 次」。每個 primary click 都透過 `cx.listener` 回到 `ClickCounter`，數字隨下一幀變為 1、2、3……。
:::

互動 element 需要穩定 `id`；在 `v-for` 中由 loop root 的 `:key` 提供 identity。缺少 identity 會在巨集展開時報錯。

`@click` 與 `on:click` 是同一事件的兩種拼寫，不能在同一 element 重複宣告。

## 目前支援的 intrinsic 事件

| 綁定 | 原生輸入 | 注意事項 |
| --- | --- | --- |
| `@click` | `ClickEvent` | 支援下列 click modifiers |
| `@key-down` | `KeyDownEvent` | 通常搭配 focus 與 key context |
| `@key-up` | `KeyUpEvent` | 鍵盤 release，無 modifiers |
| `@modifiers-changed` | `ModifiersChangedEvent` | focused dispatch path 上的 modifier 變化 |
| `@mouse-down.left` | `MouseDownEvent` | 必須選 `.left`、`.right` 或 `.middle` |
| `@mouse-down-out.left` | `MouseDownEvent` | host 外部按下時依 button 過濾 |
| `@mouse-move` | `MouseMoveEvent` | 無 modifiers |
| `@drag-move` | `DragMoveEvent<T>` | 依 exact payload type 建立 lane；可重複 |
| `@mouse-up.left` | `MouseUpEvent` | 同樣必須選 button |
| `@mouse-up-out.left` | `MouseUpEvent` | pointer 在 host 外放開時也能收尾 |
| `@scroll-wheel` | `ScrollWheelEvent` | 原生滾輪/觸控板捲動 |
| `@pinch` | `PinchEvent` | macOS 與 Wayland native pinch |
| `@hover` | `bool` | 進入為 true，離開為 false |
| `@drop` | `T` | 只接收 exact payload type；可重複 |
| `@focus` / `@blur` | 無 event payload | 必須同時傳 `:track-focus={&handle}` |

Windows precision-trackpad 的 pinch 走平台 Ctrl-wheel 路徑，應在 `@scroll-wheel` 中處理，而不是期待獨立 `PinchEvent`。

## Click modifiers

```rust
use gpui_vue::prelude::*;

fn save_button() -> impl IntoElement {
    view! {
        <button
            id="save"
            @click.stop.prevent.meta.exact={|_event, _window, _app| {
                // 只在精確按住平台 command/super 時執行。
            }}
        >
            "儲存"
        </button>
    }
}
```

click 支援 `.stop`、`.prevent`、`.ctrl`、`.alt`、`.shift`、`.meta` 與 `.exact`。它們是 GPUI propagation/default-action 與 modifier-state 的原生對應，不是 DOM listener option。`.passive`、`.once` 及其他未列出的 modifiers 會被拒絕。

## Typed native drag/drop

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#drag_drop_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#drag_drop_demo -->

::: tip 執行結果
拖曳「永 · u6c38」source 到右側 target 時，typed preview 跟隨指標；合格 target 進入 drag-over 樣式，放開後 component state 顯示最後接收的 glyph。
:::

drag source 由一對 binding 組成：

以下省略 `DraggedRow` 與實作 `Render` 的 `RowDragPreview` 定義，重點是 callback 的實際型別：

```rust
<div
    id="row-source"
    :drag-payload={DraggedRow { row: 7 }}
    :drag-preview={|
        payload: &DraggedRow,
        offset: ScreenPoint,
        _window: &mut Window,
        app: &mut App,
    | -> Entity<RowDragPreview> {
        let row = payload.row;
        app.new(move |_| RowDragPreview(row))
    }}
    @drag-move={|
        event: &DragMoveEvent<DraggedRow>,
        _window: &mut Window,
        app: &mut App,
    | {
        let payload = event.drag(app);
        let pointer = event.event.position;
        let source_bounds = event.bounds;
        let _ = (payload, pointer, source_bounds);
    }}
/>
```

`DraggedRow: 'static` 是整次 native drag 保存的 typed payload。`:drag-preview` 必須回傳 `Entity<Preview>`，其中 `Preview: Render + 'static`；`offset` 是觸發 drag 的 click 相對 host origin 的 logical-pixel offset。`:drag-payload` 與 `:drag-preview` 必須出現在同一 host，缺少任何一個都會是 macro error，而且每個 source 各只能宣告一份。

drop target 可依 payload type 同時提供接受政策、hover style 與完成 callback：

```rust
<div
    id="row-target"
    :can-drop={|payload: &dyn Any, _window, _app| payload.is::<DraggedRow>()}
    :drag-over={|
        style: StyleRefinement,
        _payload: &DraggedRow,
        _window,
        _app,
    | style.bg(rgb(0x1D_4E_D8))}
    @drop={|payload: &DraggedRow, _window, _app| {
        eprintln!("dropped row {}", payload.row);
    }}
/>
```

`:can-drop` 是每個 host 唯一的 type-erased predicate；回傳 false 時，不套用 drag-over style，也不 dispatch drop。`:drag-over` 與 `@drop` 則以 Rust `TypeId` 精確匹配 payload，可針對不同 payload type 重複宣告；例如另一組可直接接收 `ui::ExternalPaths`。`@drag-move` 也可依不同 exact type 重複。這三種 typed lanes 沒有基底型別或字串 name matching。

所有六種 drag/drop binding 都需要 stable `id`，在 `v-for` 內由 `:key` 提供。`@drag-move` / `@drop` 也有 `on:` alias，但不接受 event modifiers。macro 依 attribute source order、每次 render 各評估 expression 一次；即使 preview 寫在 payload 前面，仍只在兩者都取得後安裝一個 native drag source。

這是 GPUI 的 in-process typed drag contract，不是 HTML Drag and Drop API：沒有 `DataTransfer`、MIME string registry、DOM drag phases 或 browser default action。`DragMoveEvent<T>` 提供原生 mouse event、host bounds、`drag(app) -> &T` 與 type-erased `dragged_item()`。

## Focus 與文字輸入

`@focus` / `@blur` 使用 exact `FocusHandle` observer，callback 形狀是 `|window, app|`，而不是帶 event payload 的三參數 listener。兩者需要 `:track-focus`，以免把 descendant focus 誤當成 host 自己取得焦點。

文字輸入與 IME 請使用 [`TextInput`](./forms)，不要用 key-down 拼接可見字元。

PascalCase component 的 typed emitted events 有不同 contract：listener 收到完整的 generated event enum，且只觀察直接 child、不 bubble。詳見[元件事件](../components/events)。

## 相關閱讀

- [表單與文字輸入](./forms)
- [元件事件](../components/events)
- [模板語法](./template-syntax)
