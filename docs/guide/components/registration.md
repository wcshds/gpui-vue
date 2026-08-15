# 元件註冊與模組

gpui-vue component 是 Rust type，因此可見範圍由 Rust module、`pub` 與 `use` 決定。沒有執行期 component registry，也不需要在 render 前呼叫 `app.component(...)`。

## 在同一模組使用

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#registration_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#registration_demo -->

::: tip 執行結果
`RegistrationDemo` 以 PascalCase tag 建立 `ComponentCard`，畫面顯示 label「由 Rust 型別解析」與 tone「No registry」。這個 call site 擁有自己的 persistent child host。
:::

<NativeResult
  src="/screenshots/gallery-composition.png"
  alt="單獨執行的 RegistrationDemo，顯示由 Rust 型別解析與 No registry"
  caption="畫面中的 ComponentCard 由 Rust module scope 直接解析，不經過執行期 registry。"
/>

`ComponentCard` 是簡單 PascalCase identifier，所以模板會找到同名 type，以及它透過 `NativeComponent` 關聯的 props/input types。

## 跨模組匯入

library component 通常宣告成 `pub component`，props field 若要由普通 Rust code 讀取也可標 `pub`：

```rust
mod widgets {
    use gpui_vue::prelude::*;

    component! {
        /// 對外公開的工具列。
        pub component Toolbar {
            template(_this, _window, _cx) {
                <div class="h-10">"Toolbar"</div>
            }
        }
    }
}

use widgets::Toolbar;
```

目前 tag 本身不接受 `<widgets::Toolbar>` 或 generic path；先用 `use` 引入，必要時可 `use widgets::Toolbar as EditorToolbar`，再寫 `<EditorToolbar />`。generated associated types 會透過 traits 正確解析 alias。

## App plugin 不是元件註冊

`DesktopApp::plugin` / `setup` 用來在第一個視窗前安裝 globals、actions、key bindings 或 menu。它們不會讓一個未匯入的 Rust type突然出現在模板 scope。

::: warning 尚未實作
沒有動態依名稱尋找元件、全域 component registry、hot-loaded shared library component 或 runtime template compiler。這些機制需要不同的 type-erasure 與版本邊界，不能假裝是目前 PascalCase lane。
:::

## 相關閱讀

- [元件基礎](../essentials/component-basics)
- [Props](./props)
- [非同步元件](./async)
