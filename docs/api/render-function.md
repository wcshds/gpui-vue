# Render Function API

`view!` 是首選 render authoring API；helper 也可直接組合 curated native builders。兩條路徑都回傳 GPUI `IntoElement`，沒有 VNode 型別。

## `view!`

```rust
fn empty_state(label: impl Into<SharedString>) -> impl IntoElement {
    let label = label.into();
    view! { <div class="p-6 text-slate-500">{label}</div> }
}
```

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#render_helper_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#render_helper_demo -->

macro 產出具體 native element type；在 function boundary 使用 `impl IntoElement` 可保留 static dispatch。

## Curated builders

```rust
use gpui_vue::ui::{IntoElement, div};

fn native_div() -> impl IntoElement {
    div()
}
```

`ui::div`、`media::{image, svg_asset}`、`paint::drawing_surface`、`virtual_list::{list, uniform_list}`，以及 `anchored_overlay` / `deferred_overlay` 都是 first-class render seams。可用 `{ expression }` 插入 `view!` tree；overlay constructors 回傳自己的 concrete `IntoElement` builder，不需要先 type erase 或穿透到 `gpui_vue::gpui`。

## Type erasure

branch 必須回傳相容 concrete type時，可在適當 host boundary 使用 `IntoElement::into_any_element`。typed `SlotContent` 也只在 lazy slot 邊界抹除一次。不要為每個 child 先建立 `Vec<AnyElement>`，除非真正需要 heterogeneous dynamic collection。

## Custom painting

`paint::drawing_surface(prepaint, paint)` 保證同 frame 把 typed prepaint state 傳入 paint callback；適合 bounds-dependent transform 與 hit-test geometry。它仍是一個 native element，不是第二套 renderer。

## Virtualized collections

普通 `v-for` 會為每個 item 描述 element。大量資料可直接回傳 `uniform_list`（等高）或 `list` + retained `ListState`（可變高度）。兩者的 row renderer signatures由 GPUI typed contract 決定，gpui-vue不做隱式轉換。

## 不提供的 Vue render helpers

沒有 `h`、`createVNode`、`cloneVNode`、`isVNode`、`resolveComponent`、`resolveDirective`、`withDirectives` 或 `mergeProps`。component/type resolution由 Rust，props merging由具體 builder，conditional/list rendering由 macro 或 Rust control flow完成。

## 另見

- [General API](/api/general)
- [Options Rendering](/api/options-rendering)
- [Custom Renderer](/api/custom-renderer)
