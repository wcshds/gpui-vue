# 使用 gpui-vue 的幾種方式

`gpui-vue` 可以只負責一段 typed template，也可以負責完整桌面應用啟動。選擇最小可用層級，避免為一個靜態 helper 建立不必要的 component entity。

## 1. `view!` 作為 element helper

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#inline_panel_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#inline_panel_demo{rust}

::: tip 執行結果
`inline_panel` 每次被呼叫都建立一個顯示「Incremental view! adoption」的 native panel element；它沒有獨立 component lifecycle，適合純展示或由 caller 持有狀態的區塊。
:::

## 2. `component!` 建立 retained 元件

當 UI 擁有 local state、props、typed events、slots 或 visual lifecycle，使用 `component!`。父 template 以 PascalCase tag 掛載後，GPUI keyed element state 會跨連續 render 保留 child entity。

## 3. Curated native bridges

`ui` 提供常用 element、事件、style value 與 clipboard；`animation` 提供 keyed 時間軸與 easing；`paint` 提供 precision drawing surface；`http` 提供 transport；`desktop` 負責 application/window bootstrap。這些仍是 GPUI 原生型別，不是第二套 widget tree。

## 4. 與既有 GPUI 程式漸進整合

`DesktopApp::run` 可掛載普通 GPUI `Render` entity，`view!` 也能插入任何 `IntoElement` expression。尚未進 curated modules 的底層能力可經 `gpui_vue::gpui` 相容 re-export 使用；請把它限制在 adapter module，之後更容易移到正式 bridge。

預設 feature 適合 headless macro/state tests；需要開視窗時啟用 `desktop`。架構細節見[渲染機制](./rendering-mechanism.md)。
