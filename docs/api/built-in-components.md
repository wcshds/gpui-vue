# Built-in Components

gpui-vue 不複製 Vue 的 DOM component set；它提供適合原生 host 的 first-class typed primitives。它們可直接放進 `view!` expression 或 component state。

## `TextInput`

`text_input(placeholder, cx)` 建立 retained `TextInputHandle`，支援原生 IME composition、selection、clipboard editing 與 typed events。`text_input_with_config` 搭配 `TextInputConfig` / `TextInputStyle` 設定初始值、外觀、disabled/read-only policy 與 Unicode grapheme limit；`TextModelBinding` 提供 owned two-way model subscriptions。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#search_panel{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#search_panel -->

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#configured_text_input{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#configured_text_input -->

完整方法見 [Composition Helpers](/api/composition-api-helpers)。

## Async resources

`AsyncResource<V, E = String>` 是 owner-held resource：它保存 `AsyncState`、cancellable native `Task` 與 request generation。`load` 只從 idle 啟動，`reload` 取消並取代目前 request，`cancel` 回到 idle；window-aware 工作使用 `load_in` / `reload_in`。舊 request 即使在取消後完成，也會因 generation 不再 current 而被忽略。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

`AsyncState<V, E = String>` 仍可獨立表示 `Idle`、`Loading`、`Ready(V)` 與 `Error(E)`。這些 API 使用 GPUI executor，不建立另一個 runtime；它們不是 descendant `<Suspense>` coordination 或 async component factory。完整 signatures 與 cancellation 規則見 [Composition Helpers](/api/composition-api-helpers)；owner-safe `spawn` / `spawn_in` 見 [Lifecycle](/api/composition-api-lifecycle)。

## Native overlays

```rust
pub fn deferred_overlay(child: impl IntoElement) -> DeferredOverlay;
pub fn anchored_overlay(child: impl IntoElement) -> AnchoredOverlay;

impl DeferredOverlay {
    pub const fn priority(self, priority: usize) -> Self;
}

impl AnchoredOverlay {
    pub const fn anchor(self, corner: OverlayCorner) -> Self;
    pub const fn at(self, position: ScreenPoint) -> Self;
    pub fn at_xy(self, x: f32, y: f32) -> Self;
    pub const fn offset(self, offset: ScreenPoint) -> Self;
    pub fn offset_xy(self, x: f32, y: f32) -> Self;
    pub const fn position_mode(self, mode: OverlayPositionMode) -> Self;
    pub const fn fit(self, fit: OverlayFit) -> Self;
    pub const fn snap_to_window(self) -> Self;
    pub fn snap_to_window_with_margin(
        self,
        margin: impl Into<OverlayInsets>,
    ) -> Self;
}
```

`anchored_overlay` 選擇 child corner、明確 window/local coordinate、offset 及 window overflow fitting。`OverlayCorner` 有 `TopLeft`、`TopRight`、`BottomLeft`、`BottomRight`；`OverlayFit` 有 `SwitchAnchor`、`SnapToWindow` 與 `SnapToWindowWithMargin(OverlayInsets)`。`OverlayInsets::new(top, right, bottom, left)` / `all(margin)` 使用 logical-pixel `f32`。

`deferred_overlay` 保留原本 layout position，只延後 paint；較高 `priority` 在較低 priority 之後繪製。anchoring 本身不改 paint order，一般 popup 應以一個 deferred boundary 包住 anchored content。anchored child 不應帶 margin，因為 host 會先量測合併後的 child bounds。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#overlay_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#overlay_demo -->

這些 helper 不移動 component owner 或 element subtree；event routing、focus 與 lifecycle 仍屬原 tree。它們沒有 global/root overlay registry、跨視窗 target 或 Vue `<Teleport to=...>` 語意。`occlude` 只攔截 pointer hit testing，也不會自動建立 keyboard focus trap、dismissal 或 modality。

## Virtual lists

`gpui_vue::virtual_list` 重新匯出 GPUI 的：

- 等高 row：`uniform_list`、`UniformList`、`UniformListScrollHandle`；
- 可變 row：`list`、`List`、retained `ListState`；
- policy / payload：`ListAlignment`、`ListHorizontalSizingBehavior`、`ListMeasuringBehavior`、`ListOffset`、`ListScrollEvent`、`ListSizingBehavior` 與 `ScrollStrategy`。

virtualization 會改變 mount 與測量範圍，因此不會把普通 `v-for` 暗中轉換。呼叫端要依 row geometry 明確選擇 `uniform_list` 或 `list`。

## Media elements

```rust
use gpui_vue::media::{image, svg_asset, external_svg};
```

`image(source)` 建立 raster/animated `Img`；`svg_asset(path)` 從已安裝 assets 解析，`external_svg(path)` 從 filesystem path 載入。模組也匯出 `Image`、`ImageSource`、`Img`、`ObjectFit`、`RenderImage`、`StyledImage`、`Svg`、`Transformation` 與底層 constructors `img` / `svg`。

`view!` 的 `<img>` 直接提供 typed `:object-fit={ObjectFit}`、`:loading={Fn() -> AnyElement}` 與 `:fallback={Fn() -> AnyElement}`，對應同一個 `StyledImage` surface；需要 image cache 或其他低層 builder 時才使用 `media::image(source)`。

遠端 image 需要 `DesktopApp::http_client` 安裝 transport；embedded path 需要 `DesktopApp::assets`。

## Native animation

`animation::{Animation, AnimationElement, AnimationExt}` 對 retained element 提供 keyed timeline；`easing` 含 `linear`、`quadratic`、`ease_in_out`、`ease_out_quint`。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#animation_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#animation_demo -->

## Vue built-ins boundary

尚無高階 `<Transition>`、`<TransitionGroup>`、`<KeepAlive>`、`<Teleport>` 或 `<Suspense>` coordination。native animation、anchored/deferred overlay、owner-held entity/state 與 `AsyncResource` 是正式 API，但不可把它們宣稱為相同 lifecycle contract。

## 另見

- [Transitions](/guide/built-ins/transition)
- [Overlay 與 Teleport 邊界](/guide/built-ins/teleport)
- [Async Components 邊界](/guide/components/async)
- [Virtual list 效能](/guide/best-practices/performance)
- [Native Style Features](/api/native-style-features)
