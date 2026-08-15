# Built-in Special Attributes

attributes 由 macro 按 tag 類型靜態驗證。Intrinsic attributes 不會 fall through 到 PascalCase component。

## Identity

| Attribute | Contract |
| --- | --- |
| `id="save"` / `:id={value}` | native `ElementId`；interactive state 的穩定 identity |
| `key="primary"` / `:key={value}` | component/element state namespace；loop 只接受 dynamic item-derived key |

同名 shorthand `:id` / `:key` 讀取 Rust local。`key` 不傳給 generated component props。

## Styling

| Attribute | Contract |
| --- | --- |
| `class="..."` | compile-time literal Tailwind-like subset |
| `:class={if ... { "..." } else { "..." }}` | compile-time known literal branch tree |
| `:style={|style| style.w(px(80.0))}` | typed `StyleRefinement -> StyleRefinement` callback |

`class` 不是任意 runtime string，`:style` 也不是 CSS string/map。詳見 [Native Style Features](/api/native-style-features)。

## Focus 與 keyboard context

```rust
<div
    id="editor"
    focusable
    tab-index={0}
    :track-focus={&focus}
    key-context="Editor"
    @key-down={handler}
    @focus={focused}
/>
```

- `focusable` 是 bare boolean attribute；
- `tab-index` / `tab_index` 接受整數 expression；
- `:track-focus` 接受 exact `&FocusHandle`，供 focus routing 與 focus/blur observers；
- `key-context="literal"` 或 `:key-context={value}` 設定 native key dispatch context。

這些與 listeners 都需要 stable `id` 或 loop identity。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#focus_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#focus_demo -->

## Hit testing

bare `occlude` 安裝 native blocking pointer hitbox；也要求 stable identity。它不是 modal focus trap，不會自動處理 Escape、背景 keyboard input 或 accessibility modality。

## Image source

`img` 接受一個 static `src="..."` 或 dynamic `:src={image_source}`（含 shorthand）。不能同時提供兩個 source，也不能有 children。

| Image-only binding | Exact native contract |
| --- | --- |
| `:object-fit={fit}` | `gpui_vue::media::ObjectFit`；lower 至 `StyledImage::object_fit` |
| `:loading={loading}` | `Fn() -> AnyElement + 'static`；lower 至 `StyledImage::with_loading` |
| `:fallback={fallback}` | `Fn() -> AnyElement + 'static`；lower 至 `StyledImage::with_fallback` |

```rust
use gpui_vue::{media::ObjectFit, prelude::*};

fn image_with_states(source: String) -> impl IntoElement {
    view! {
        <img
            :src={source}
            :object-fit={ObjectFit::Cover}
            :loading={|| view! { <text>"載入中…"</text> }.into_any_element()}
            :fallback={|| view! { <text>"載入失敗"</text> }.into_any_element()}
            class="h-32 w-full rounded-lg"
        />
    }
}
```

所有 expression 依 attribute source order 各求值一次；loading/fallback function body 保持 lazy，由 GPUI 的 image state 調用。這些 binding 不需要 stable `id`，因為 pinned GPUI 的 `StyledImage` API 本身不要求 identity。`object-fit="cover"` 或 `:object-fit="cover"` 會被拒絕，不提供 DOM/CSS string compatibility。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#image_bindings_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#image_bindings_demo -->

<NativeResult
  src="/screenshots/gallery-image.png"
  alt="原生 Guide Gallery 的 typed image states panel，ObjectFit::Contain 完整顯示 KAGE Editor 圖標"
  caption="由上方同一個 image_bindings_demo 實際編譯、啟動、滾動至可見區後擷取；這是 GPUI 原生 image pipeline 的輸出。"
/>

## Component host attributes

PascalCase tag 保留 `key` / `:key`、`:props`、`:slots` 與 typed `@event`。其餘 attrs 都是 exact generated prop setters；沒有 `class`/`style`/listener fallthrough 或 general HTML attribute bag。

## 另見

- [Built-in Directives](/api/built-in-directives)
- [Fallthrough Attributes](/guide/components/attrs)
