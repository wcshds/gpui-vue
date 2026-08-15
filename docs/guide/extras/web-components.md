# Web Components 邊界

Custom Elements 依賴 browser custom-elements registry、`HTMLElement` lifecycle、attributes/properties、Shadow DOM 與 DOM events。`gpui-vue` 建立的是 native GPUI elements，不存在可註冊的 browser element class。

## 不提供的相容假象

目前沒有且不計畫以同名空殼模擬：

- `defineCustomElement` 與 `customElements.define`；
- Shadow Root、slot distribution 或 CSS encapsulation；
- DOM attribute reflection、`CustomEvent` 與 bubbling/composed flags；
- 將 generated component 輸出成 JavaScript package。

這些是 Web-only renderer 能力，不能靠補一個 macro 就保持語意。

## Native 的可重用邊界

在 Rust 應用間共享元件，將 component、props 與 events 放入 library crate。gallery 的 typed component 展示了同一個可匯出的 Rust 邊界：

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_props -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#typed_props{rust}

consumer 以正常 PascalCase tag 使用，所有事件仍是 typed GPUI stream。若需要跨語言或跨程序，應在 domain/service 層設計 JSON、protobuf 或 IPC protocol，而不是把 native element 偽裝成 HTMLElement。

::: tip 執行結果
library component 與 consumer 一起由 rustc monomorphize，無需 JavaScript registry；required/default props 都保留型別。版本與 ABI 邊界遵循 Rust crate，而非瀏覽器 tag name。
:::

同一產品若需要真正的 Web Component，保留共享 domain model，再以 Vue/Web 工具建立獨立 frontend。原生程式的多種整合方式見[使用 gpui-vue 的幾種方式](./ways-of-using-gpui-vue.md)。
