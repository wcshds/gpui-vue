# Native Style Features

此頁整理 gpui-vue 的 curated native bridge。這些是 first-class API；只有未被它們覆蓋的 host seam 才需要 `gpui_vue::gpui`。

## `ui`

```rust
pub fn div() -> Div;
pub fn image(source: impl Into<ImageSource>) -> Img;
pub const fn px(value: f32) -> Pixels;
pub fn rgb(value: u32) -> Rgba;
pub fn rgba(value: u32) -> Rgba;
pub fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla;
pub fn write_clipboard_text(app: &App, text: impl Into<String>);
pub fn read_clipboard_text(app: &App) -> Option<String>;
```

模組也匯出常用 `App`、`Window`、`Context`、`Entity`、`IntoElement`、focus、subscription、`Font` / `Hsla` / `Rgba`，以及所有 template event payload types。drag/drop surface 可直接使用 `DragMoveEvent<T>`、`ExternalPaths` 與 `StyleRefinement`；`ScreenPoint` 是 `Point<Pixels>` alias。精確 binding contract 見 [Built-in Directives](/api/built-in-directives#drag-drop-bindings)。

`apply_style_refinement`、`image_object_fit`、`image_loading`、`image_fallback`、`type_click_handler`、`type_mouse_down_handler`、`type_drag_preview`、`FocusEventHandler`、`boxed_focus_handler`、`focus_events` 與 `FocusEventElement` 帶有 `#[doc(hidden)]`；它們讓 macro expansion 在 downstream crate 保持 contextual typing、typed `StyledImage` calls 與 exact focus subscription。這些名稱是 public implementation ABI，不是手寫 UI 的一般入口。

## Compile-time classes

`class="literal"` 與 literal-leaf `:class` 在 macro expansion 解析為 typed GPUI style calls。支援範圍含 layout/flex/grid、spacing、size、typography、borders/radius、colors/opacity/shadows、overflow、cursor與部分 interaction variants；精確 token 以[能力矩陣](/capability-matrix)與 compiler tests 為準。

runtime class string、CSS selector/cascade、stylesheet inheritance與 arbitrary object/array binding不支援。未知 utility 直接是 compile error。

### Overflow

`overflow-clip` / `overflow-visible` 與 `overflow-x-*` / `overflow-y-*` 對應版本會直接寫入 GPUI 的 per-axis `Overflow::Clip` / `Overflow::Visible`。它們不是 retained scroll container、不要求 element ID，也可出現在已支援的 interaction variant 中。`clip` 不讓溢出內容貢獻 parent scroll region；`visible` 保留內容對 parent overflow 的貢獻。

`overflow-scroll` 及 axis versions 仍是 static-only retained-state utilities，要求 stable `id` 或 loop key，而且不能放進 interaction variant。`overflow-auto` 仍無可靠 native 對應。所有 broad/axis overflow 候選共用 typed per-axis cascade，依 Tailwind canonical order和 trailing `!` 選出結果，不依 class token 書寫順序。

### Line height

`leading-[Npx]` / `leading-[Nrem]` 產生 absolute native length；新的 nonnegative unitless `leading-[1.5]` 與 percentage `leading-[150%]` 都產生 `relative(1.5)`，`leading-[20%]` 則是 `relative(0.2)`。`leading-[0]` 可用；負號、明確 `+`、non-finite、`auto` 及其他單位會在編譯期拒絕。這些 arbitrary forms 與 named、bare numeric spacing forms 寫入同一個 line-height property slot，仍套用 canonical candidate order與 `!`。

`rounded-[Npx]` / `rounded-[Nrem]` 以及 `rounded-t/r/b/l-*`、`rounded-tl/tr/br/bl-*` 的同類形式會直接產生 typed `Pixels` / `Rems` corner calls。shorthand 先拆為四個 physical corner slots，再套用 canonical、state 與 `!` cascade；負值、non-finite、百分比、`auto` 和未知單位都會在編譯期拒絕。

普通態 `content-normal` 直接呼叫 GPUI `Styled::content_normal()` 清除 `align_content`，並保留 canonical / important 行為。`hover:content-normal` 等 state 形式會精確拒絕，因為 state refinement 裏的 `None` 無法覆蓋已解析的 base field；`justify-normal`、`self-auto` 也沒有相同的 faithful native reset。

### `focus-visible:`

一層未堆疊的 `focus-visible:utility` 會降低為 GPUI 的 keyboard-modality-aware `focus_visible` refinement：只在 exact target focus 且視窗最近輸入來自鍵盤時生效。它要求 stable `id` / loop key，並讓 host 進入 focusable path。這是 native input-modality predicate，不宣稱與每個瀏覽器的 `:focus-visible` heuristic 完全相同。

若 `focus-visible:` 與其他同時成立的 state 對同一 property 產生 Tailwind/GPUI winner 不一致，macro 會像其他 cross-state 組合一樣回報 targeted compile error；不同 property 可正常共存。stacked、named group、`focus-within:` 與其他未列 variant 仍不支援。

## Typed `:style`

```rust
<div :style={move |style| style.w(px(width)).text_color(rgb(0xE2_E8_F0))} />
```

callback 接收 fresh `StyleRefinement` 並回傳 refinement；它在 static/conditional classes 之後 merge。這不是 CSS map，因而不接受 property string、`!important` 或 browser cascade。

## `media`

`image`、`svg_asset`、`external_svg` 以及 `ImageSource`、`ObjectFit`、`Transformation` 等 typed native media surface。`ui::image` forwards 至同一 pipeline。

`<img>` 可直接使用 `:object-fit={ObjectFit}`、`:loading={Fn() -> AnyElement}` 與 `:fallback={Fn() -> AnyElement}`；它們精確對應 `StyledImage`，不是 CSS string properties。

## `animation`

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#native_fade_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#native_fade_demo -->

`AnimationExt::with_animation(id, animation, mapper)` 以 stable ID 保留 timeline；mapper 依每幀 delta 回傳 styled element。沒有 CSS transition classes 或 enter/leave subtree coordinator。

## `paint`

```rust
pub fn drawing_surface<T: 'static>(
    prepaint: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) -> T + 'static,
    paint: impl FnOnce(Bounds<Pixels>, T, &mut Window, &mut App) + 'static,
) -> Canvas<T>;
```

prepaint 的 typed 結果在同 frame 傳入 paint，適合精密 canvas、chart 與 editor。模組匯出 bounds/point/path/fill/quad/shadow 等 geometry/paint vocabulary。

## `virtual_list`

提供正式 `uniform_list` / `list` bridge 及 scroll/state/sizing types；大量資料應明確選用，不透過 class 或 directive 隱式啟用。

## `http` 與 `assets`

`http` 匯出 `HttpClient`、`Request`、`Response`、`AsyncBody`、`BodyInner`、`Url`、`HttpResult`，以及 host 的 `http` / `anyhow` namespaces。

```rust
pub const fn EmbeddedAssets::new() -> EmbeddedAssets;
pub fn with_file(self, path: impl Into<String>, bytes: &'static [u8]) -> EmbeddedAssets;
pub fn insert(&mut self, path: impl Into<String>, bytes: &'static [u8]) -> Option<&'static [u8]>;
pub fn get(&self, path: &str) -> Option<&'static [u8]>;
pub fn is_empty(&self) -> bool;
pub fn len(&self) -> usize;
pub fn list(&self, prefix: &str) -> Vec<&str>;
```

`EmbeddedAssets` 以 exact logical path 保存 `&'static [u8]`，並可用 `DesktopApp::assets` 安裝。同 path 再 insert 會回傳並替換舊 bytes；path 不做 normalization。`list` 回傳以 literal prefix 開頭的完整 paths，順序為 lexicographic。

```rust
use gpui_vue::EmbeddedAssets;

static ICON: &[u8] = b"native-icon";
let assets = EmbeddedAssets::new().with_file("icons/app.bin", ICON);
assert_eq!(assets.get("icons/app.bin"), Some(ICON));
```

## Platform

visual、input、clipboard、image codec、HTTP transport、pinch與材質最終由 GPUI host/OS 提供。相同 typed API 不保證每個平台有完全相同 native affordance。

## 另見

- [Built-in Components](/api/built-in-components)
- [Custom Renderer](/api/custom-renderer)
