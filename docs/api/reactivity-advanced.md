# Reactivity API: Advanced

進階狀態 API 保持 cache 與 effect ownership 可見。它們沒有隱藏的 dependency collection 或 scheduler。

## `Memo<T, D = Revision>`

```rust
pub const fn Memo::new() -> Memo<T, D>;
pub fn get(&self) -> Option<&T>;
pub fn dependencies(&self) -> Option<&D>;
pub fn invalidate(&mut self);
pub fn get_or_update(&mut self, dependencies: D, compute: impl FnOnce() -> T) -> &T
where D: PartialEq;
```

cache 初始為空。dependency key 不同時才呼叫 `compute`；多個依賴可用 `(Revision, Revision)` 等 typed tuple。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#memo_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#memo_demo -->

若 `compute` panic，原有 entry 保持不變，panic 會繼續向上傳播。`invalidate` 立即丟棄 key 與結果，但不通知任何 entity。

## `EffectScope`

```rust
pub const fn EffectScope::new() -> EffectScope;
pub fn track(&mut self, subscription: Subscription);
pub fn len(&self) -> usize;
pub fn is_empty(&self) -> bool;
pub fn clear(&mut self);
pub fn detach(self);
```

scope 持有 cancellable native `Subscription`。clear 或 drop 會取消全部 callback；`detach` 消耗 scope 並讓 callback 延續至相關 entities 被釋放。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#effect_scope_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#effect_scope_demo -->

一般元件應把 scope 放在 `state` 內，並讓 visual/entity lifetime 自然回收；只有 callback 刻意超過 owner lifetime 時才 detach。

## Watch helpers

`watch_entity` / `watch_entity_in` 觀察另一個 entity 的 notification；`watch_event` / `watch_event_in` 訂閱其 typed event。每個函式都回傳必須保留的 `Subscription`。

```rust
use gpui_vue::effects::{EffectScope, watch_entity};

// 在 component setup/mounted 中：
// effects.track(watch_entity(cx, &model, |_owner, _model, cx| cx.notify()));
```

帶 `_in` 版本還把 `&mut Window` 傳給 callback。這些 API 觀察 GPUI notification/event stream，不追蹤 `Local::get` 或 `Ref::read`。

## 另見

- [Lifecycle API](/api/composition-api-lifecycle)
- [Watchers 指南](/guide/essentials/watchers)
- [Reactivity in Depth](/guide/extras/reactivity-in-depth)
