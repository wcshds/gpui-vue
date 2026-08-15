# Options API: Rendering

每個 `component!` 必須恰有一個 `template(this, window, cx)` section。它產生該 component 的 native `Render` implementation。

## Signature

```rust
template(this, window, cx) {
    /* direct markup, view!, or a Rust expression implementing IntoElement */
}
```

| 參數 | 內容 |
| --- | --- |
| `this` | `&mut Component`，可讀 props/state/memo |
| `window` | `&mut Window`，原生視窗與 frame context |
| `cx` | `&mut Context<Component>`，listener、notify、entity APIs |

template 可直接寫 markup：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#status_view{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#status_view -->

需要先執行 Rust statements 時，可在 section 內最後回傳 `view! { ... }`：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#square_counter{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#square_counter -->

## Re-render

`cx.notify()` 或被觀察的 native model 使 entity dirty；下一次 GPUI render 會重建 element description。keyed `ComponentElement` 會保留 child entity，並沒有 Vue VNode diff 或 DOM patch。

## Roots 與 identity

單一 root 直接回傳其 element。多 roots / root fragment 會因 GPUI 需要一個 `IntoElement` 而加入 synthetic `div`；nested fragment 則直接 flatten。interactive element 必須有穩定 `id`，loop root 必須有 item-derived `:key`。

## Errors

缺少 template、重複 template、回傳值不實作 `IntoElement` 都是編譯錯誤。render 內 panic 會照普通 Rust/host panic 策略傳播；gpui-vue 不設 error boundary。

## 另見

- [Render Function](/api/render-function)
- [Special Elements](/api/built-in-special-elements)
- [Rendering Mechanism](/guide/extras/rendering-mechanism)
