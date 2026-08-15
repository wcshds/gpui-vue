# Composition API: Helpers

此頁收錄可在 component state/setup 中組合的 typed helpers：原生文字輸入、slots 與 owner-scoped async resources。

## Input constructors

```rust
pub type TextInputHandle = Entity<TextInput>;

pub fn text_input<Parent: 'static>(
    placeholder: impl Into<String>,
    cx: &mut Context<'_, Parent>,
) -> TextInputHandle;

pub fn text_input_with_config<Parent: 'static>(
    config: TextInputConfig,
    cx: &mut Context<'_, Parent>,
) -> TextInputHandle;
```

`text_input` 以 default style、空 value 與可編輯 policy 建立 retained 單行 input；`text_input_with_config` 在 mount 時一次提供 value、style 與 editing policy。兩者都直接實作 GPUI platform input handler，處理 marked text、UTF-16 ranges、selection 與 clipboard editing。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#search_panel{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#search_panel -->

設定化 input 也由同一個可執行 gallery 顯示；執行後可看到較高的深色欄位、藍色 focus border 與「可選取與複製；不能修改」說明：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#configured_text_input{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#configured_text_input -->

## `TextInputConfig`

```rust
impl TextInputConfig {
    pub fn new(placeholder: impl Into<String>) -> Self;
    pub fn value(self, value: impl Into<String>) -> Self;
    pub fn style(self, style: TextInputStyle) -> Self;
    pub fn disabled(self, disabled: bool) -> Self;
    pub fn read_only(self, read_only: bool) -> Self;
    pub fn max_length(self, max_length: usize) -> Self;
    pub fn unlimited(self) -> Self;
}
```

`new` 建立空值、default style、enabled 且 editable 的 config。`max_length` 限制後續 user edit，並以 Unicode extended grapheme clusters 計數，不以 UTF-8 bytes 或 scalar values 計數；marked IME composition 可暫時超過上限，等平台 commit 時才裁切。初始或程式化 controlled value 由 parent 擁有，不受這個 user-edit limit 改寫。`unlimited` 移除先前設定的限制。此 type 也實作 `Default`，其 placeholder 為空字串。

## `TextInputStyle`

`TextInputStyle::default()` 是可保存、clone 且可比較的 native style value。缺少固定 width 代表填滿可用寬度；所有尺寸是 logical pixels。

```rust
impl TextInputStyle {
    pub fn width(self, width: Pixels) -> Self;
    pub fn fill_width(self) -> Self;
    pub fn height(self, height: Pixels) -> Self;
    pub fn padding(self, padding: Pixels) -> Self;
    pub fn padding_x(self, padding: Pixels) -> Self;
    pub fn padding_y(self, padding: Pixels) -> Self;
    pub fn background_color(self, color: impl Into<Hsla>) -> Self;
    pub fn text_color(self, color: impl Into<Hsla>) -> Self;
    pub fn placeholder_color(self, color: impl Into<Hsla>) -> Self;
    pub fn border_color(self, color: impl Into<Hsla>) -> Self;
    pub fn focus_border_color(self, color: impl Into<Hsla>) -> Self;
    pub fn selection_color(self, color: impl Into<Hsla>) -> Self;
    pub fn caret_color(self, color: impl Into<Hsla>) -> Self;
    pub fn border_width(self, width: Pixels) -> Self;
    pub fn corner_radius(self, radius: Pixels) -> Self;
    pub fn font(self, font: Font) -> Self;
    pub fn font_family(self, family: impl Into<SharedString>) -> Self;
    pub fn font_size(self, size: Pixels) -> Self;
    pub fn disabled_opacity(self, opacity: f32) -> Self;
}
```

負數尺寸 clamp 至零，font size 至少 1 logical pixel。finite disabled opacity clamp 至 `0.0..=1.0`；非 finite 值不改變原設定。color builders 接受任何可轉成 native `Hsla` 的值。

## `TextInput`

```rust
pub fn TextInput::new(
    placeholder: impl Into<String>,
    cx: &mut Context<'_, Self>,
) -> Self;
pub fn TextInput::with_config(
    config: TextInputConfig,
    cx: &mut Context<'_, Self>,
) -> Self;
pub fn text(&self) -> &str;
pub const fn style(&self) -> &TextInputStyle;
pub fn set_style(&mut self, style: TextInputStyle, cx: &mut Context<'_, Self>);
pub const fn is_disabled(&self) -> bool;
pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<'_, Self>);
pub const fn is_read_only(&self) -> bool;
pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<'_, Self>);
pub const fn max_length(&self) -> Option<usize>;
pub fn set_max_length(&mut self, limit: Option<usize>, cx: &mut Context<'_, Self>);
pub fn set_text(&mut self, value: impl Into<String>, cx: &mut Context<'_, Self>);
pub fn clear(&mut self, cx: &mut Context<'_, Self>);
pub fn set_placeholder(&mut self, value: impl Into<String>, cx: &mut Context<'_, Self>);
pub fn focus_handle(&self) -> &FocusHandle;
pub fn focus(&self, window: &mut Window);
pub fn blur(&self, window: &mut Window);
pub fn is_composing(&self) -> bool;
pub fn selected_range(&self) -> Range<usize>;
```

`TextInput::new` 建立一個空值、default style、enabled 且 editable 的底層 component state，等同 `TextInput::with_config(TextInputConfig::new(placeholder), cx)`。應用端通常使用 `text_input(placeholder, parent_cx)` 取得 retained `Entity<TextInput>`；只有在自行構造 entity state 時才直接呼叫 associated constructor。

`set_text` / `clear` / `set_max_length` 是程式化 controlled update，會 notify input entity，但不偽造 user `Change` event。`set_text` 會正規化為單行，但不套用 user-edit limit；降低 `max_length` 也不會暗中截短既有 controlled value。selection range 是 UTF-8 byte range，平台 IME 的 UTF-16 range 會在內部安全換算。

disabled input 拒絕 focus 與所有 user interaction，但仍接受程式化 update。read-only input 可 focus、選取、複製與 emit `Submit`，但拒絕 user edit。將已 focus 的 input disabled 會在下一次 render 釋放 focus。

## `TextInputEvent`

```rust
pub enum TextInputEvent {
    Change(String),
    Submit(String),
    Escape,
}
```

透過 `cx.subscribe` / `subscribe_in` 監聽。`Change` 是使用者可見文字改變，`Submit` 由確認鍵產生，`Escape` 供 parent 決定取消行為。

## `TextModelBinding`

```rust
pub fn TextModelBinding::bind<Owner, Read, Write>(
    input: &TextInputHandle,
    initial_value: impl Into<String>,
    cx: &mut Context<'_, Owner>,
    read: Read,
    write: Write,
) -> Self
where
    Owner: 'static,
    Read: FnMut(&Owner) -> String + 'static,
    Write: FnMut(&mut Owner, String, &mut Context<'_, Owner>) + 'static;

pub fn TextModelBinding::bind_ref<Owner: 'static>(
    input: &TextInputHandle,
    model: Ref<String>,
    cx: &mut Context<'_, Owner>,
) -> Self;

pub fn detach(self);
```

binding 持有兩個 native subscriptions：input `Change` 寫回 parent；parent notification 再以 `read` 靜默 reconcile input。初始值會在 callbacks 安裝前寫入。相等 value 由 `TextInput::set_text` 忽略，避免 IME intermediate value 的 echo 清除 marked range。

drop binding 會取消兩個方向。`detach` 讓它們持續到相關 entities release，只有 callbacks 刻意超過 parent slot lifetime 時才用。`bind_ref` 的外部 `Ref<String>` mutation 必須以 parent context 作 notifier，parent-to-input observer 才會執行。

::: warning 密碼輸入
目前沒有 secure/password mode。以普通 `TextInput` 隱藏 glyph 仍可能讓明文進入 selection、clipboard、IME 與 accessibility 路徑，不構成安全控制項。
:::

## `Slot<Props>`

```rust
pub const fn Slot::empty() -> Slot<Props>;
pub fn Slot::new<E>(renderer: impl Fn(Props, &mut Window, &mut App) -> E + 'static) -> Slot<Props>
where E: IntoElement;
pub fn Slot::from_fn<E>(renderer: impl Fn(Props) -> E + 'static) -> Slot<Props>
where E: IntoElement;
pub const fn is_present(&self) -> bool;
pub fn render(&self, props: Props, window: &mut Window, app: &mut App) -> Option<SlotContent>;
pub fn render_or_else<E>(
    &self,
    props: Props,
    window: &mut Window,
    app: &mut App,
    fallback: impl FnOnce(Props, &mut Window, &mut App) -> E,
) -> SlotContent
where E: IntoElement;
```

slot provider 是 lazy 且 `'static`，只在 receiver 呼叫時才產生一個 element。`SlotContent::new` 抹除具體 element type；`into_inner` 取回 `AnyElement`。一般應使用 declarative `<slot>` 與 `<template #name>`，手動 API 適合 Rust-body template。

## `AsyncResource<Value, Error = String>`

```rust
pub struct AsyncResource<Value, Error = String> { /* private fields */ }

impl<Value, Error> AsyncResource<Value, Error> {
    pub const fn new() -> Self;
    pub const fn ready(value: Value) -> Self;
    pub const fn state(&self) -> &AsyncState<Value, Error>;
    pub const fn value(&self) -> Option<&Value>;
    pub const fn error(&self) -> Option<&Error>;
    pub const fn is_loading(&self) -> bool;
    pub const fn generation(&self) -> u64;

    pub fn load<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        locate: Locate,
        load: Load,
    ) -> bool
    where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncContext) -> Result<Value, Error> + 'static;

    pub fn reload<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        locate: Locate,
        load: Load,
    )
    where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncContext) -> Result<Value, Error> + 'static;

    pub fn load_in<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        window: &Window,
        locate: Locate,
        load: Load,
    ) -> bool
    where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncWindowContext) -> Result<Value, Error> + 'static;

    pub fn reload_in<Owner, Locate, Load>(
        &mut self,
        cx: &mut Context<'_, Owner>,
        window: &Window,
        locate: Locate,
        load: Load,
    )
    where
        Owner: 'static,
        Value: 'static,
        Error: 'static,
        Locate: for<'owner> Fn(&'owner mut Owner) -> &'owner mut Self + 'static,
        Load: AsyncFnOnce(&mut AsyncWindowContext) -> Result<Value, Error> + 'static;

    pub fn cancel<Owner>(&mut self, cx: &mut Context<'_, Owner>) -> bool
    where Owner: 'static;
}
```

resource 必須保存在 `locate` 所選的同一個 owner entity；這個 callback 讓 task 在 `.await` 後從 weak owner 找回 resource。`load` 只在 `Idle` 時啟動並回傳 `true`，其他 state 會回傳 `false`，且不呼叫 loader。`reload` 一律取消並取代前一個 request；`load_in` / `reload_in` 是提供 `AsyncWindowContext` 的 window-aware 對應版本。

`new()` / `Default` 建立 generation 0 的 `Idle` resource；`ready(value)` 建立 generation 0 的 `Ready(value)` resource。`state` 借用完整 enum，三個 convenience getter 分別借用 ready value、error 或判斷 loading；`generation` 主要供 diagnostics 與 deterministic tests，render 通常應以 `state()` 為準。

request 開始時會同步切成 `Loading` 並 notify owner。每次 load、reload 或 cancel 都遞增 generation；只有仍為 current 的 completion 能提交 `Ready` / `Error`，所以被取消的舊 future 即使稍後完成也不能覆寫新 state。`cancel` 回到 `Idle`、notify，並回傳取消前是否正在 loading。drop resource 會 drop owned task，因而取消尚未完成的工作。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

loader 的 `Err(Error)` 是可呈現的 domain error，不是排程失敗；它會成為 `AsyncState::Error`。generation 在單一 resource 壽命內溢出 `u64` 時會 panic，這是刻意避免舊 generation 重新成為 current 的防衛條件。window-aware variants 的實際工作仍由目前 GPUI host/event loop 執行。

## `AsyncState<Value, Error = String>`

```rust
pub enum AsyncState<V, E = String> { Idle, Loading, Ready(V), Error(E) }
pub const fn is_loading(&self) -> bool;
pub const fn value(&self) -> Option<&V>;
pub const fn error(&self) -> Option<&E>;
pub fn map<M>(self, f: impl FnOnce(V) -> M) -> AsyncState<M, E>;
```

它是 `AsyncResource::state()` 暴露的可窮舉 UI state，也能獨立使用。`map` 只轉換 `Ready` value，其餘 variant 原樣保留。`async_state::Task<T>` 是 GPUI cancellable task 的 re-export；直接呼叫 `spawn` / `spawn_in` 時必須保留回傳 task，或把 loading/error ownership 交給 `AsyncResource`。

## 另見

- [Forms 指南](/guide/essentials/forms)
- [Slots API](/api/options-composition)
- [Async Components 指南](/guide/components/async)
