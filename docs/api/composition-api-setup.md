# Composition API: Setup

gpui-vue 沒有 Vue runtime 的 `setup()` function。對應入口是 `component!` 內可選的 `setup(this, props, cx)` section；它在 entity construction 期間執行一次。

下列宣告與 repository 的 component macro fixtures 使用同一種語法；整個 docs gallery 會由 `cargo check --example docs_gallery` 驗證，setup 的 compile-time contract 則由 `crates/gpui-vue/tests/component.rs` 覆蓋。

相同的 component 宣告、state initializer 與 template 在可執行 gallery 中如下；此 component 沒有 setup section，因為 setup 是按需加入、且不改變其他 sections 的形狀：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_stepper{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_stepper -->

## Signature

```rust
component! {
    /// A component with one-time setup.
    component Example {
        props {
            /// Initial label.
            label: SharedString,
        }
        state {
            /// Derived once from the initial props.
            initial_len: usize = 0,
        }
        setup(this, props, _cx) {
            this.initial_len = props.label.len();
        }
        template(this, _window, _cx) {
            <text>{format!("{} ({})", this.props().label, this.initial_len)}</text>
        }
    }
}
```

參數均由 macro 靜態綁定：

| 參數 | 型別/用途 |
| --- | --- |
| `this` | 可變的 generated state draft |
| `props` | 初始 generated props 的共享借用 |
| `cx` | `&mut Context<Component>`，可建立 focus handle/entity/subscription |

section 的名稱可自行選，但位置與數量固定。setup 回傳 `()`；無法像 Vue setup function 回傳一個 template binding object，因為 template 直接存取 typed fields 與 Rust lexical items。

## 執行時機

順序為：建立 props/input → 執行所有 `state` initializer → 執行 `setup` → 建立 entity。相同 keyed host 後續收到新 props 時只 reconcile input，不重跑 initializer 或 setup。

setup 發出的 component event 可能早於 parent declarative subscription 安裝，因此 parent 可能收不到。需要第一幀後通知時，改用 `mounted` 或 `next_frame`。

## Errors

重複 section、未知 section、錯誤參數數量，以及未記錄的 component/field 會在 macro expansion 報錯。setup 內的借用、型別與 lifetime 問題則是一般 rustc diagnostics。

## 另見

- [Component Setup DSL](/api/component-setup)
- [Lifecycle Hooks](/api/composition-api-lifecycle)
- [Options: State](/api/options-state)
