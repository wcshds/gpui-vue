# Application API

此頁記錄 `gpui_vue::desktop`。模組只有在啟用 `desktop` Cargo feature 時存在。

## `DesktopApp`

```rust
pub struct DesktopApp { /* private fields */ }

impl DesktopApp {
    pub fn new(window: WindowConfig) -> Self;
    pub fn http_client(self, client: impl HttpClient) -> Self;
    pub fn assets(self, assets: EmbeddedAssets) -> Self;
    pub fn quit_mode(self, mode: QuitMode) -> Self;
    pub fn on_open_urls<Handler>(self, handler: Handler) -> Self
    where Handler: FnMut(Vec<String>) + 'static;
    pub fn on_reopen<Handler>(self, handler: Handler) -> Self
    where Handler: FnMut(&mut App) + 'static;
    pub fn register_url_scheme<Scheme>(self, scheme: Scheme) -> Self
    where Scheme: Into<String>;
    pub fn setup(self, setup: impl FnOnce(&mut App) + 'static) -> Self;
    pub fn plugin<P: AppPlugin>(self, plugin: P) -> Self;
    pub fn run<V>(self, build_root: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static)
    where V: Render;
    pub fn run_component<C>(self, build_root: impl FnOnce(&mut Window, &mut App) -> Entity<C> + 'static)
    where C: NativeComponent;
}
```

`new` 建立 platform application 並保存第一個視窗設定。所有 builder 都消耗並回傳 `Self`；多個 `setup` / `plugin` 依註冊順序，在視窗開啟前執行。`on_open_urls` 可被平台呼叫多次並傳入一批 URL 字串；`on_reopen` 在平台要求重新顯示已執行的應用時取得 live `App`。兩種 callback 的實際觸發時機都由作業系統決定。

`register_url_scheme` 接受不含 `://` 的 scheme 名稱，例如 `"gpui-vue"`。它在 setup 階段啟動 host registration task；非同步平台錯誤會由 GPUI 記錄，builder 本身不回傳 registration result。應用仍須 parse 並驗證收到的每個 URL，不能把 OS dispatch 當作可信輸入。

```rust
let app = DesktopApp::new(window)
    .register_url_scheme("gpui-vue")
    .on_open_urls(|urls| {
        for url in urls {
            eprintln!("open request: {url}");
        }
    })
    .on_reopen(|app| {
        // 可在沒有適當視窗時呼叫 open_window / open_component_window。
        let _ = app;
    });
```

這是 builder fragment；最後仍需呼叫 `run` 或 `run_component` 啟動 event loop。

`run_component` 是 `component!` 根元件的首選，因為它會安裝 visual lifecycle host。`run` 接受任何 GPUI `Render` entity，但不附加 generated component 的 `mounted` / `updated` / `unmounted` 語意。兩者都啟動 event loop，不回傳控制權。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#app_root{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#app_root -->

### Panics

`run` 與 `run_component` 在平台無法開啟初始視窗時 panic。應用程式啟動後的錯誤策略由 host、HTTP transport 與業務程式碼決定。

## Additional windows

```rust
pub fn open_window<View, BuildRoot>(
    app: &mut App,
    config: WindowConfig,
    build_root: BuildRoot,
) -> DesktopResult<WindowHandle<View>>
where
    View: 'static + Render,
    BuildRoot: FnOnce(&mut Window, &mut App) -> Entity<View>;

pub fn open_component_window<ComponentType, BuildRoot>(
    app: &mut App,
    config: WindowConfig,
    build_root: BuildRoot,
) -> DesktopResult<AnyWindowHandle>
where
    ComponentType: NativeComponent,
    BuildRoot: FnOnce(&mut Window, &mut App) -> Entity<ComponentType>;
```

兩個 helper 都使用與初始視窗相同的 `WindowConfig` conversion。`open_window` 接受任意 raw `Render` root，回傳保留 concrete view type 的 handle，但不安裝 generated component visual lifecycle。`open_component_window` 會把 generated component 掛到 lifecycle-aware root；因真正的 native root 是內部 host，所以回傳 type-erased `AnyWindowHandle`。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#desktop_window_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#desktop_window_demo -->

兩者在視窗成功建立後回傳 handle；平台拒絕建立視窗時透過 `desktop::DesktopResult` 回傳 host error，不會 panic。window kind、背景材質、URL registration、reopen callback 及其可用性都是 platform-dependent；呼叫端不應假設每個 backend 都提供相同 affordance。

## `AppPlugin`

```rust
pub trait AppPlugin: 'static {
    fn install(self, app: &mut App);
}
```

任何 `FnOnce(&mut App) + 'static` 已實作此 trait。plugin 可註冊 globals、actions、key bindings 或 menus；它是 application 級安裝點，不是元件 registry。

## `WindowConfig`

```rust
impl WindowConfig {
    pub fn new(
        title: impl Into<SharedString>,
        width: f32,
        height: f32,
    ) -> Self;
}
```

建立置中的原生視窗。`width` / `height` 是 logical pixels。可鏈接：

| 方法 | 作用 |
| --- | --- |
| `min_size(w, h)` | 設定可調整下限 |
| `transparent_titlebar(bool)` | 讓內容延伸至 titlebar |
| `traffic_light_position(x, y)` | 設定 macOS traffic lights 位置 |
| `focused(bool)` / `visible(bool)` | 初始焦點與可見性 |
| `kind(WindowKind)` | Normal、PopUp 等原生類型 |
| `movable(bool)` / `resizable(bool)` / `minimizable(bool)` | 視窗操作政策 |
| `background(WindowBackgroundAppearance)` | Opaque、Blurred 等 host 材質 |
| `app_id(String)` | 桌面環境分組識別 |
| `tabbing_identifier(String)` | 原生 tab group 識別 |

所有座標也是 logical pixels。部分 kind、背景與 traffic-light 選項是平台專屬；不支援的平台由 GPUI host 決定如何處理。

## Re-exports

`desktop` 重新匯出 `AnyWindowHandle`、`WindowHandle`、`DesktopResult`、`QuitMode`、`WindowBackgroundAppearance` 與 `WindowKind`。

## 另見

- [應用程式指南](/guide/essentials/application)
- [App-wide State](/api/composition-api-dependency-injection)
- [Compile-time Flags](/api/compile-time-flags)
