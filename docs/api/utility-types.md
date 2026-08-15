# Utility Types

gpui-vue 的 public types 主要把 ownership、identity與 macro-generated contract留給 Rust type checker。

## State types

| Type | Contract |
| --- | --- |
| `Local<T>` | inline entity-local value，`&mut self` mutation |
| `Ref<T>` | cloneable single-threaded shared handle |
| `Revision` | wrapping `u64` dependency token |
| `Memo<T, D = Revision>` | explicit dependency-key cache |
| `AsyncState<V, E = String>` | idle/loading/ready/error UI state |
| `AsyncResource<V, E = String>` | 擁有 state、cancellable task 與 request generation 的 owner-local resource |
| `EffectScope` | owned native subscriptions |
| `TextInputConfig` / `TextInputStyle` | 可 clone、比較與保存的原生 input 設定 |
| `TextModelBinding` | 擁有 input ↔ parent model 的兩個 subscriptions |

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

## Component types

`component!` 對 `Widget` 產生 concrete `WidgetProps`、typestate builder、private input；依 sections 另產生 `WidgetEvent` 與 `WidgetSlots`。下列 public traits 是 downstream macro expansion 與 persistent host 之間的 ABI：

```rust
pub trait NativeComponent: Render + Sized {
    type Props: 'static;
    type Input: 'static;
    type MountState: NativeComponentMount<Self>;

    fn attach_mount(entity: &Entity<Self>, cx: &mut App) -> Self::MountState;
    fn construct(input: Self::Input, cx: &mut Context<Self>) -> Self;
    fn reconcile_input(
        &mut self,
        input: Self::Input,
        cx: &mut Context<Self>,
    ) -> bool;
}

pub trait NativeComponentMount<ComponentType>: 'static
where
    ComponentType: NativeComponent,
{
    type RenderToken: 'static;

    fn attach(entity: &Entity<ComponentType>, cx: &mut App) -> Self;
    fn input_changed(&self);
    fn render_token(&self) -> Self::RenderToken;
    fn after_render(
        token: &Self::RenderToken,
        entity: &Entity<ComponentType>,
        window: &mut Window,
        cx: &mut App,
    );
}

pub trait ComponentLifecycleHooks: NativeComponent {
    const HAS_MOUNTED: bool = false;
    const TRACK_UPDATES: bool = false;
    const HAS_UNMOUNTED: bool = false;

    fn mounted(component: &mut Self, window: &mut Window, cx: &mut Context<Self>);
    fn updated(component: &mut Self, window: &mut Window, cx: &mut Context<Self>);
    fn unmounted(component: &mut Self, cx: &mut App);
}

pub trait NativeComponentSlots: NativeComponent {
    type Slots: Default + 'static;

    fn slots(&self) -> &Self::Slots;
    fn input_with_slots(props: Self::Props, slots: Self::Slots) -> Self::Input;
}

pub trait NativeComponentEvents: NativeComponent { type Event: 'static; }
```

`NativeComponent::attach_mount` 由 default implementation委派給 `MountState::attach`；`reconcile_input` 的 `bool` 表示當幀 input 是否可能改變 render output。`NativeComponentMount::RenderToken` 是每幀複製進 transparent host element 的 token，`input_changed` 在 comparable input 改變時標記 lifecycle state，`after_render` 在 delegated layout 後執行。

`ComponentLifecycleHooks::{HAS_MOUNTED, TRACK_UPDATES, HAS_UNMOUNTED}` 是 generated compile-time flags；`mounted` / `updated` / `unmounted` 的 default body皆為空。`NativeComponentSlots::input_with_slots` 把 comparable props 與 typed slots 組成完整 input。這些 traits 雖為 public，且數個標記為 `#[doc(hidden)]`，一般應用不應手寫 implementation；它們的 public 可見性是為了讓下游 crate 的 `component!` 產物可以命名 ABI。

Persistent host recipes 也是 public concrete types，其 constructors 供 generated code 使用：

```rust
impl<ComponentType, const SUBSCRIPTIONS: usize>
    ComponentMount<ComponentType, SUBSCRIPTIONS>
{
    pub const fn new(
        entity: Entity<ComponentType>,
        subscriptions: [Subscription; SUBSCRIPTIONS],
    ) -> Self;
}

impl<ComponentType, Subscribe, const SUBSCRIPTIONS: usize>
    ComponentElement<ComponentType, Subscribe, SUBSCRIPTIONS>
where
    ComponentType: NativeComponent,
    Subscribe: FnOnce(
        &Entity<ComponentType>,
        &mut Window,
        &mut App,
    ) -> [Subscription; SUBSCRIPTIONS] + 'static,
{
    pub const fn new(
        slot: ElementId,
        key: Option<ElementId>,
        input: ComponentType::Input,
        subscribe: Subscribe,
    ) -> Self;
}

impl<ComponentType, Event, Handler>
    ComponentEventElement<ComponentType, Event, Handler>
where
    ComponentType: NativeComponentEvents<Event = Event> + EventEmitter<Event>,
    Event: 'static,
    Handler: FnMut(&Event, &mut Window, &mut App) + 'static,
{
    pub const fn new(
        slot: ElementId,
        key: Option<ElementId>,
        input: ComponentType::Input,
        handler: Handler,
    ) -> Self;
}
```

`ComponentMount::new` 只構造 default `Lifecycle = ()` 的 mount，而 `ComponentElement::new` / `ComponentEventElement::new` 是每幀 recipe；實體與 subscriptions 由 keyed window element state 保留。`ComponentMount::entity` 與 `ComponentEventMount::entity` 會回傳另一個 strong `Entity` handle；`ComponentEventMount` 的 constructor 不是 public。

`PropMissing` / `PropSet` 是 zero-sized builder typestate marker；`RequiredProp<T, State>` 是 sealed inline storage。它們公開供 downstream expansion命名，不是 application data model。

## Curated `ui` re-exports

`gpui_vue::ui` 是 template 事件與低階 native helper 的穩定匯入點。除 `App`、`Context`、`Entity`、`Window`、`IntoElement` 等 host core types 外，它明確重新匯出：

- interaction：`ClickEvent`、`KeyDownEvent`、`KeyUpEvent`、`ModifiersChangedEvent`、`MouseButton`、`MouseDownEvent`、`MouseMoveEvent`、`MouseUpEvent`、`PinchEvent`、`ScrollWheelEvent` 與 `TouchPhase`；
- drag/drop：`DragMoveEvent` 與 external-file payload `ExternalPaths`；
- clipboard / typography：`ClipboardItem`、`Font`、`FontWeight`；
- visual/value types：`FocusHandle`、`Hsla`、`Rgba`、`Pixels`、`SharedString`、`StyleRefinement` 與 `Subscription`。

`KeyDownEvent` / `KeyUpEvent` 是 key payload，`ModifiersChangedEvent` 是 modifier-state payload，mouse、pinch、scroll 與 touch types均保留 GPUI 的 typed native contract；沒有 DOM `Event` union 或字串 downcast。

```rust
#[doc(hidden)]
pub fn type_drag_preview<Payload, Preview, Constructor>(
    constructor: Constructor,
) -> Constructor
where
    Payload: 'static,
    Preview: Render + 'static,
    Constructor: Fn(
        &Payload,
        ScreenPoint,
        &mut Window,
        &mut App,
    ) -> Entity<Preview> + 'static;
```

`ui::type_drag_preview` 是 `view!` 為 `:drag-preview` closure 提供 contextual typing 的 hidden macro ABI；它原樣回傳 constructor，不 boxing、不分配。application code 應寫 `:drag-payload` / `:drag-preview`，不直接呼叫此 helper。

## Paint、media 與 virtual-list re-exports

`gpui_vue::paint` 明確重新匯出 `BorderStyle`、`BoxShadow`、`ContentMask` 與 `PathBuilder`，另有 `Bounds`、`Pixels`、`Rgba`、`ScreenPoint`、`bounds`、`fill`、`point`、`px`、`quad`、`rgba` 與 `size`。這些值供 `drawing_surface` 的 prepaint / paint callbacks使用：`ContentMask` 描述 paint clip，`PathBuilder` 建立 native path，`BorderStyle` 與 `BoxShadow` 則是 quad paint 的 typed style data。

`gpui_vue::media` 重新匯出 `Image`、`ImageSource`、`Img`、`ObjectFit`、`RenderImage`、`StyledImage`、`Svg`、`Transformation` 及 raw constructors `img` / `svg`。`RenderImage` 可作為 cached native render image source；`img(source)` 是底層 raster constructor，`svg()` 建立可再設定 asset path 或 external path 的 `Svg`。語意化 wrappers 是 `media::image`、`media::svg_asset` 與 `media::external_svg`。

`gpui_vue::virtual_list` 重新匯出 `List`、`ListState`、`UniformList`、`UniformListScrollHandle`、`ScrollStrategy` 與 constructors `list` / `uniform_list`；也明確匯出 policy/payload types `ListAlignment`、`ListHorizontalSizingBehavior`、`ListMeasuringBehavior`、`ListOffset`、`ListScrollEvent` 與 `ListSizingBehavior`。後六者分別描述 alignment、水平 sizing、measurement policy、scroll offset、scroll notification payload 與 list sizing policy；它們保留 pinned GPUI 的 enum/struct contract，gpui-vue 不再以字串封裝。

## Native aliases/re-exports

- `TextInputHandle = Entity<TextInput>`；`TextInputStyle` / `TextInputConfig` / `TextModelBinding` 從 crate root 與 prelude 匯出；
- `ui::ScreenPoint = Point<Pixels>`；
- `http::HttpResult` 是 host HTTP result；
- `http::BodyInner` 是 async body inner type；
- `async_state::Task<Output>` 是 GPUI cancellable task；保留 task 讓工作存活，drop 會取消；
- `virtual_list` 的完整重新匯出集合見上節；
- `media` 的完整重新匯出集合見上節。

async effects 的公開 alias 保持 native ownership，而不要求應用程式命名 GPUI crate path：

```rust
pub type AsyncContext = gpui::AsyncApp;
pub type AsyncWindowContext = gpui::AsyncWindowContext;
pub type WeakOwner<Owner> = gpui::WeakEntity<Owner>;
```

三者從 crate root 匯出；prelude 只匯入搭配它們的 `spawn` / `spawn_in`，需要在 signature 中命名 alias 時應明確匯入。`WeakOwner` 不會保持 entity 存活，更新已釋放 owner 會以 host error 表示，而不是復活 component。

## Overlay types

```rust
pub enum OverlayCorner { TopLeft, TopRight, BottomLeft, BottomRight }
pub enum OverlayPositionMode { Window, Local }
pub enum OverlayFit {
    SwitchAnchor,
    SnapToWindow,
    SnapToWindowWithMargin(OverlayInsets),
}

pub struct OverlayInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl OverlayInsets {
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self;
    pub const fn all(margin: f32) -> Self;
}

impl From<f32> for OverlayInsets;
```

`OverlayCorner::TopLeft`、`OverlayPositionMode::Window` 與 `OverlayFit::SwitchAnchor` 是 defaults。`AnchoredOverlay` / `DeferredOverlay` 是 opaque `IntoElement` builders，由 `anchored_overlay` / `deferred_overlay` 建立；完整方法見 [Native Components](/api/built-in-components)。

啟用 `desktop` feature 時，`desktop` 另重新匯出 `WindowHandle<View>`、type-erased `AnyWindowHandle` 與 native error alias `DesktopResult<T>`。前者是 `open_window` 的 raw-view handle；generated component 視窗使用後者，因 native root 是 lifecycle host。

## Trait bounds 與 errors

Props 需要 `PartialEq`；`Local::set/update` 需要 `T: PartialEq`；`Ref::update` 另需要 `Clone`；slot providers與effect callbacks需要 `'static`。不滿足 bounds 是正常 rustc diagnostics，沒有 runtime type coercion。

## 不提供的 Web utility types

沒有 `CSSProperties`、HTML attribute bag、DOM event map、VNode props或 browser ARIA type map。對 native events/colors/style使用 `ui` typed exports；accessibility高階 contract仍需在 gpui-vue補齊。

## 另見

- [Reactivity Core](/api/reactivity-core)
- [Component Instance](/api/component-instance)
- [Native Style Features](/api/native-style-features)
