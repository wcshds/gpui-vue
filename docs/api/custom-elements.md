# Custom Elements Boundary

Vue Custom Elements 把 component 註冊為瀏覽器 `HTMLElement`，依賴 custom-elements registry、attributes/properties、DOM events、shadow root 與 CSS。gpui-vue 是 native GPUI renderer，因此不提供該 API。

## 明確不提供

沒有：

- `defineCustomElement` / `configureApp`；
- `HTMLElement` subclass、`customElements.define`；
- attribute reflection、DOM `CustomEvent`；
- shadow DOM / shadow CSS / custom-element lifecycle callbacks。

這些不是待補的 native wrapper，而是 Web-only host contract。

## Native reusable boundary

在 gpui-vue 中，公開 reusable component：

1. 將 `component` 宣告為 `pub`；
2. 以 public typed props/events/slot-props 表達 contract；
3. 從 Rust crate/module 匯出；
4. 如需 app-wide installation，另提供 `AppPlugin`。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#status_badge{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#status_badge -->

若要在 Web page 中嵌入 native app，需要另一層 platform/wasm/render integration；這不會讓 native entity 變成 DOM custom element。

## 另見

- [Web Components 指南](/guide/extras/web-components)
- [Options Misc](/api/options-misc)
