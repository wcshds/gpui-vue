# API Reference

這裡記錄 gpui-vue **目前可編譯的公開契約**。若你第一次使用，先讀[快速開始](/guide/quick-start)；若已經知道要找的名稱，則從本頁進入各模組。

gpui-vue 採 Vue 風格的宣告方式，但輸出是 GPUI element 與 entity，沒有 JavaScript、DOM、虛擬 DOM 或 CSS runtime。API 頁中的「不提供」是明確的邊界，不是相容性承諾。

本 reference 的主要可見範例都從 `crates/gpui-vue/examples/docs_gallery.rs` 擷取；文件顯示的 source 與實際畫面共用同一份編譯單元。可用下列命令檢查並開啟：

```sh
cargo check -p gpui-vue --example docs_gallery --features desktop --locked
cargo run -p gpui-vue --example docs_gallery --features desktop --locked
```

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#app_root{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#app_root -->

## 一般 API

| 分類 | 主要入口 | 參考 |
| --- | --- | --- |
| 應用程式 | `DesktopApp`、`WindowConfig`、`open_window`、`open_component_window` | [Application](/api/application) |
| 模板 | `view!`、`component!` | [General](/api/general)、[Component Setup](/api/component-setup) |
| 本地狀態 | `Local`、`Revision`、`Memo` | [Reactivity: Core](/api/reactivity-core)、[Advanced](/api/reactivity-advanced) |
| 共享 handle | `Ref`、`ref_`、`reactive_ref` | [Reactivity: Core](/api/reactivity-core) |
| Effects | `EffectScope`、watchers、`spawn` / `spawn_in`、`next_frame` | [Lifecycle](/api/composition-api-lifecycle) |
| App globals | `provide_global`、`global`、`watch_global` | [Dependency Injection](/api/composition-api-dependency-injection) |
| 元件契約 | typed props、events、slots、lifecycle | [Options API](/api/options-state) |
| 原生輸入 | `TextInput`、`TextInputConfig`、`TextInputStyle`、`TextModelBinding` | [Composition Helpers](/api/composition-api-helpers) |
| 非同步資源 | `AsyncResource`、`AsyncState`、`Task` ownership | [Composition Helpers](/api/composition-api-helpers) |
| 浮層 | `anchored_overlay`、`deferred_overlay`、fitting policy | [Native Components](/api/built-in-components) |

## 完整索引

### Composition-style APIs

- [Setup](/api/composition-api-setup)：一次性 component 建構 hook。
- [Reactivity: Core](/api/reactivity-core)：`Local`、`Ref` 與 notifier。
- [Reactivity: Utilities](/api/reactivity-utilities)：revision 與讀取方式。
- [Reactivity: Advanced](/api/reactivity-advanced)：`Memo`、effect scope 與 watchers。
- [Lifecycle](/api/composition-api-lifecycle)：visual hooks、owner-safe async spawn、next-frame、defer、release。
- [App-wide State](/api/composition-api-dependency-injection)：typed application globals 與 subtree 邊界。
- [Helpers](/api/composition-api-helpers)：文字輸入、slots、`AsyncResource` state/cancellation 與 task ownership。

### Component declaration APIs

- [State](/api/options-state)：props、state initializer 與 reconciliation。
- [Rendering](/api/options-rendering)：template signature、roots 與 re-render。
- [Lifecycle](/api/options-lifecycle)：mounted、updated、unmounted。
- [Composition](/api/options-composition)：typed events 與 slots。
- [Miscellaneous](/api/options-misc)：visibility、名稱與文件契約。
- [Component Instance](/api/component-instance)：generated accessors、entity 與 host internals。

### Template built-ins

- [Directives](/api/built-in-directives)
- [Native Components](/api/built-in-components)
- [Special Elements](/api/built-in-special-elements)
- [Special Attributes](/api/built-in-special-attributes)

### Source、native 與 host boundaries

- [Rust Component File Format](/api/sfc-spec)
- [`component!` Setup DSL](/api/component-setup)
- [Native Style Features](/api/native-style-features)
- [Custom Elements Boundary](/api/custom-elements)
- [Render Function API](/api/render-function)
- [SSR Boundary](/api/ssr)
- [Utility Types](/api/utility-types)
- [Custom Renderer Boundary](/api/custom-renderer)
- [Compile-time Flags](/api/compile-time-flags)

## Curated native bridges

應用程式要碰到 host 能力時，優先使用窄而穩定的 gpui-vue 模組。

| 模組 | 內容 |
| --- | --- |
| `ui` | 常用 element、event、focus、clipboard、色彩與 pixels |
| `paint` | typed path、bounds 與 `drawing_surface` |
| `animation` | `Animation`、`AnimationExt`、easing functions |
| `media` | raster / animated image、SVG、object-fit 與 transformation |
| `virtual_list` | 等高與可變高度的大型 native list |
| `async_state` | `AsyncResource`、`AsyncState` 與 GPUI cancellable `Task` |
| `overlay` | anchored positioning、window fitting 與 deferred paint order |
| `http` | `HttpClient`、request/response/body/URL contract |
| `assets` | `EmbeddedAssets`，供 `include_bytes!` 安裝靜態資產 |
| `desktop` | app bootstrap、額外視窗、open-URL/reopen callbacks 與 URL scheme registration |

這些橋接不複製 host 實作；它們提供 gpui-vue 應用可依賴的命名空間。詳見 [Native Style Features](/api/native-style-features) 與 [Custom Renderer](/api/custom-renderer)。

## Macro 產生與 framework internals

`NativeComponent`、`NativeComponentSlots`、`NativeComponentEvents`、`ComponentElement`、mount 與 typestate marker 是公開的，因為下游 crate 的 macro expansion 必須能命名它們。日常程式不應手動建構 host internals；使用 PascalCase markup 與產生的 props builder 即可。

完整邊界見 [Component Instance](/api/component-instance) 與 [Utility Types](/api/utility-types)。若 curated bridge 尚未包裝需要的能力，`gpui_vue::gpui` 是低層互通出口；它不表示該 API 已成為 gpui-vue 的穩定高階語意。

## Feature 與平台

`desktop` 模組需要 Cargo 的 `desktop` feature。原生視窗、輸入法、clipboard、pinch 與背景材質的實際可用性取決於 GPUI host 與作業系統。編譯期 feature 列表見 [Compile-time Flags](/api/compile-time-flags)。

## 另見

- [能力矩陣](/capability-matrix)
- [架構決策](/architecture)
- [可執行範例](/examples/counter)
