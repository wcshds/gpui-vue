# Class 與 Style 綁定

gpui-vue 的 class 不是交給瀏覽器解析的 CSS。literal 會在編譯期轉成 typed GPUI style calls；無法可靠映射的 utility 會在編譯時被拒絕。這讓 release binary 不需要 class parser，也讓樣式錯誤靠近來源。

## 靜態 class

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#style_gallery{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#style_gallery -->

::: tip 執行結果
gallery 先顯示一枚深綠底、白字的圓角「Connected」狀態標籤；同頁的 selection row 會依 `selected` 改為藍底白字或深灰底灰字，meter 則使用 runtime pixel width。
:::

支援範圍包含常用 flex/grid、尺寸與間距、位置、文字、色彩、border、radius、overflow，以及一組 GPUI 能精確表達的 interaction variants。完整清單與差異以[能力矩陣](/capability-matrix)為準。

## 條件 class

`:class` 接受 literal 或 Rust `if` tree：

```rust
use gpui_vue::prelude::*;

fn row(selected: bool) -> impl IntoElement {
    view! {
        <div
            id="selection-row"
            class="px-3 py-2 rounded"
            :class={if selected {
                "bg-blue-600 text-white"
            } else {
                "bg-slate-800 text-slate-300"
            }}
        >
            "圖層 1"
        </div>
    }
}
```

static `class` 會合併進每個動態 branch，再由編譯期 cascade 決定每個 typed property 的結果。condition 在 render 時只沿被選中的 branch 求值。

任意 `String`、`Vec<String>`、object map 或由網路下載的 class name 不受支援；這些形態需要執行期 CSS-like parser，會破壞目前的零解析路徑。

## Typed inline style

需要真正的 runtime 數值時，使用 `:style` refinement：

```rust
use gpui_vue::prelude::*;
use gpui_vue::ui::{px, rgb};

fn meter(width: f32) -> impl IntoElement {
    view! {
        <div
            class="h-2 rounded"
            :style={move |style| style.w(px(width)).bg(rgb(0x3b82f6))}
        />
    }
}
```

callback 接收新的 `StyleRefinement`，並在 regular class 之後合併。這是 GPUI typed style，不接受 CSS 字串、array/object normalization 或瀏覽器 cascade priority。

## Interaction state 與 identity

`hover:`、`active:`、`focus:` 等 stateful class 需要 GPUI 保留互動 state，因此 element 必須有穩定 `id`；在 `v-for` 內由動態 `:key` 提供 identity。巨集會拒絕缺少 identity 的寫法。

GPUI 的 hover、focus 與 group relation 是原生 hitbox/focus 狀態，不是 browser pseudo-class。plain `border` 也沒有 CSS `currentColor` 預設；要顯示可攜的 border，請明確給 `border-<color>`。

## 目前界線

- 沒有 scoped CSS、CSS Modules、custom properties 或 `<style>` block。
- 沒有通用 CSS cascade、selector engine、transition class 或 keyframes。
- PascalCase component 不會自動承接 parent 的 `class` / `style`；請設計明確 typed props 或 wrapper。
- 不支援的 Tailwind family 不會僅因拼寫合法就被當作已相容。

## 相關閱讀

- [模板語法](./template-syntax)
- [條件渲染](./conditional)
- [Fallthrough Attributes](../components/attrs)
- [能力矩陣](/capability-matrix)
