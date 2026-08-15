# Composition API: Lifecycle

gpui-vue 有兩層 lifecycle：`component!` 的 visual host hooks，以及可在任何 entity 中使用的 effects helpers。它們遵循 GPUI frame/effect 時序，不承諾瀏覽器 DOM hook 時序。

## Component hooks

```rust
component! {
    /// Lifecycle-aware status.
    component Status {
        mounted(this, window, cx) { /* first completed delegated draw */ }
        updated(this, window, cx) { /* later dirty delegated draw */ }
        unmounted(this, cx) { /* visual host removed */ }
        template(_this, _window, _cx) { <text>"Ready"</text> }
    }
}
```

| Hook | 參數 | 時機 |
| --- | --- | --- |
| `mounted` | `&mut Self`, `&mut Window`, `&mut Context<Self>` | 第一次成功 delegated draw 後，在該 effect cycle 尾端 |
| `updated` | 同上 | 後續 dirty draw 後；已排程工作會 coalesce |
| `unmounted` | `&mut Self`, `&mut App` | keyed visual host 消失後，至多一次 |

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#lifecycle_probe{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#lifecycle_probe -->

`v-if` 移除或 component key 改變會移除 identity；intrinsic `v-show` 不會。直接呼叫 generated `Component::new` 建立的 naked entity 沒有 visual host hooks；視窗根應使用 `DesktopApp::run_component`。

## `spawn` / `spawn_in`

```rust
pub type AsyncContext = gpui::AsyncApp;
pub type AsyncWindowContext = gpui::AsyncWindowContext;
pub type WeakOwner<Owner> = gpui::WeakEntity<Owner>;

pub fn spawn<Owner, AsyncFn, Output>(
    cx: &Context<'_, Owner>,
    operation: AsyncFn,
) -> Task<Output>
where
    Owner: 'static,
    AsyncFn: AsyncFnOnce(WeakOwner<Owner>, &mut AsyncContext) -> Output + 'static,
    Output: 'static;

pub fn spawn_in<Owner, AsyncFn, Output>(
    cx: &Context<'_, Owner>,
    window: &Window,
    operation: AsyncFn,
) -> Task<Output>
where
    Owner: 'static,
    AsyncFn: AsyncFnOnce(WeakOwner<Owner>, &mut AsyncWindowContext) -> Output + 'static,
    Output: 'static;
```

兩個 helper 都在 GPUI foreground executor 啟動 owner-safe 工作，不建立另一個 runtime。`operation` 收到 weak owner，所以 task 不會單憑自己延長 component entity 壽命；`.await` 後可用 weak handle 的 fallible update，owner 已釋放時應忽略該結果。

回傳的 `Task<Output>` 是工作 ownership token：保存它會讓工作繼續，drop 就取消。`spawn_in` 額外綁定 originating window，讓 closure 可透過 `AsyncWindowContext` 更新該視窗。需要 `Idle` / `Loading` / `Ready` / `Error` state、replacement cancellation 與 stale-result protection 時，應由 owner 保存 `AsyncResource`，而非自行拼接 detached task。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#async_resource_demo -->

helper 本身不回傳排程 error；future 的 domain error 應放在 `Output`，panic 則仍遵循 native task executor 的 panic policy。`spawn_in` 需要有效的 native window/context，但沒有特定作業系統限定。

## `next_frame`

```rust
pub fn next_frame<O>(
    cx: &Context<'_, O>,
    window: &mut Window,
    callback: impl FnOnce(&mut O, &mut Window, &mut Context<'_, O>) + 'static,
);
```

在下一個 native frame 執行一次 owner callback。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#deferred_content{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#deferred_content -->

## `defer`

```rust
pub fn defer<O>(
    cx: &mut Context<'_, O>,
    window: &Window,
    callback: impl FnOnce(&mut O, &mut Window, &mut Context<'_, O>) + 'static,
);
```

把 callback 排到目前 native effect cycle 尾端。適合避免正在進行的 borrow / dispatch 內立即重入，不等同 JavaScript microtask。

## `on_release`

```rust
pub fn on_release<O>(
    cx: &Context<'_, O>,
    cleanup: impl FnOnce(&mut O, &mut App) + 'static,
) -> Subscription;
```

在 GPUI 釋放 owner entity 時執行 cleanup。必須保留回傳的 `Subscription`；drop subscription 會取消 callback。這是 entity release，不是 process finalizer。

## Watch functions

`watch_entity[_in]` 與 `watch_event[_in]` 的完整 ownership 規則見 [Reactivity Advanced](/api/reactivity-advanced)。所有 callback 都要求 `'static`，避免借用短於 native subscription。

## 另見

- [Lifecycle 指南](/guide/essentials/lifecycle)
- [Reactivity Advanced](/api/reactivity-advanced)
- [Application API](/api/application)
