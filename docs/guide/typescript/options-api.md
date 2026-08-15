# `component!` 區段與型別

如果一個視圖需要完整 props、state、events、slots 與生命週期，把它們放進 `component!` 能讓公開面和實作在同一處。這與動態 options object 不同：每個區段都在編譯期產生具名 Rust 型別或實作。

## Typed event payload

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_stepper -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#typed_stepper{rust}

::: tip 執行結果
每次 activation 將數值增加一，畫面立即顯示新值，並透過原生 GPUI event channel 發送 `TypedStepperEvent::Changed { value }`。父元件 handler 必須接受完整 enum 型別。
:::

## 區段對應的 Rust 產物

- `props` 產生 comparable props struct、constructor 與 typestate builder。
- `state` 變成 component entity 的欄位，不進入隱藏 map。
- `emits` 產生 event enum、`EventEmitter` impl 與 `emit_*` helper。
- `slots` 產生 typed lazy provider；scoped props 也是 Rust 型別。
- `setup` 只在 entity construction 執行一次。
- lifecycle sections 由 visual host 靜態 dispatch。

## 所有權注意事項

event、slot provider 與 deferred callback 通常要求 `'static` capture。需要回到 parent 時，generated direct slot provider 使用 weak entity 避免 ownership cycle；自行寫長壽 closure 時也應明確選擇 weak handle。`unmounted` 收到 `App` 而非 window-bound context，不能假設視窗仍存在。

詳細 component authoring 可接著讀[元件 API 常見問題](../extras/component-api-faq.md)。
