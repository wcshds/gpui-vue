# Custom Renderer Boundary

GPUI 已是 gpui-vue 的 host renderer。`view!` 在編譯期直接產生 GPUI elements，`component!` 使用 GPUI entities、layout、prepaint、paint、focus與subscriptions；中間沒有可替換的 virtual-node renderer。

## 沒有 `createRenderer`

gpui-vue 不提供 Vue runtime-core 的 `createRenderer` / `createHydrationRenderer` host operations，也沒有 `nodeOps`、patch prop或VNode diff hook。新增另一個 renderer會改變本專案「單一 host tree」架構，而不是一般擴充點。

## 正式擴充層

| 需求 | API |
| --- | --- |
| typed view helper | `view!` 或回傳 `impl IntoElement` 的 Rust function |
| reusable retained unit | `component!` |
| native media | `media` |
| large collections | `virtual_list` |
| precision paint | `paint::drawing_surface` |
| time-based element style | `animation` |
| floating placement / paint order | `anchored_overlay` / `deferred_overlay` |
| owner-safe async work | `AsyncResource` 或 `spawn` / `spawn_in` |
| native network contract | `http` |
| platform/bootstrap與額外視窗 | `desktop`、`open_window` / `open_component_window` |

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#native_fade_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#native_fade_demo -->

## Low-level host interoperability

curated module已涵蓋能力時，應視其為 first-class gpui-vue API。只有尚未包裝的 host feature 才從 `gpui_vue::gpui` 使用完整 re-export；該程式碼直接依賴 pinned GPUI contract。

手寫 `Element` / `Render` 可透過 `{ expression }` 插入模板。若要自己管理 request-layout/prepaint/paint，需遵守 GPUI global element identity、window state與callback lifetime；gpui-vue不會替自訂 element重新協調。

`paint` 的 low-level curated surface 包含 `BorderStyle`、`BoxShadow`、`ContentMask` 與 `PathBuilder`；`media` 包含 cached `RenderImage` 及 raw `img` / `svg` constructors；`virtual_list` 包含 `ListAlignment`、`ListHorizontalSizingBehavior`、`ListMeasuringBehavior`、`ListOffset`、`ListScrollEvent` 與 `ListSizingBehavior`。完整重新匯出清單與各類型的角色見 [Utility Types](/api/utility-types)。

## `ComponentElement` internals

PascalCase lowering用 keyed per-window element state保留 child entity。`HostedEntity` 透明委派 native element phases；mount types保留 subscriptions/lifecycle並在 identity消失時drop。這是 framework host implementation，不是 renderer plugin API。

## 另見

- [Rendering Mechanism](/guide/extras/rendering-mechanism)
- [Render Function](/api/render-function)
- [Architecture](/architecture)
