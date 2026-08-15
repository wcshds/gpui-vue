# Composition API: App-wide State

gpui-vue 目前提供 application-wide typed globals。它適合 theme、服務與 app session；它**不是** Vue 的最近祖先 `provide` / `inject`，沒有 component-subtree shadowing。

## `Global`

```rust
pub use gpui::{Global, ReadGlobal, UpdateGlobal};
```

值型別實作 marker trait `Global` 後，便可用型別作為 application key。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#theme_global_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#theme_global_demo -->

## Functions

```rust
pub fn provide_global<T: Global>(app: &mut App, value: T);
pub fn has_global<T: Global>(app: &App) -> bool;
pub fn global<T: Global>(app: &App) -> &T;
pub fn try_global<T: Global>(app: &App) -> Option<&T>;
pub fn global_mut<T: Global>(app: &mut App) -> &mut T;
pub fn default_global<T: Global + Default>(app: &mut App) -> &mut T;
pub fn watch_global<T: Global>(
    app: &mut App,
    callback: impl FnMut(&mut App) + 'static,
) -> Subscription;
pub fn remove_global<T: Global>(app: &mut App) -> T;
```

- `provide_global` 安裝或替換一個值。
- `global` / `global_mut` 要求該型別已存在；`try_global` 是非 panic 版本。
- `global_mut` 的 host access 會通知 observers，即使 caller 最後沒有改值；若需要精確相等抑制，應在更高層先比較。
- `default_global` 在缺少時安裝 `Default::default()`。
- `watch_global` 的 callback 會在目前 effect cycle 之後排程；保留 subscription 才能持續觀察。
- `remove_global` 取出並移除值。

### Panics

`global`、`global_mut` 與 `remove_global` 在未提供 `T` 時 panic。library/plugin 不確定安裝順序時應使用 `try_global` 或 `default_global`。

## 安裝時機

通常從 `DesktopApp::setup` / `plugin` 安裝，確保第一個視窗與 component 建構前已可讀取：

```rust
DesktopApp::new(window)
    .setup(|app| provide_global(app, GalleryTheme { dark: true }))
    .run_component(build_root);
```

## Component-subtree boundary

目前沒有 `provide`、`inject`、injection key 或 ancestor shadowing API。若資料只屬於一棵 component subtree，請由 owner entity 持有並透過 typed props/events/slots 傳遞；不可把 app global 描述成 subtree injection。

## 另見

- [Provide / Inject 指南](/guide/components/provide-inject)
- [Application API](/api/application)
- [Options Composition](/api/options-composition)
