# Reactivity API: Core

gpui-vue 的 reactive primitive 只負責值、相等抑制與明確 notification。讀取不會建立 dependency graph；mutation 只通知傳入的 notifier。

## `Local<T>`

```rust
pub const fn Local::new(value: T) -> Local<T>;
pub fn get(&self) -> T where T: Clone;
pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R;
pub const fn as_ref(&self) -> &T;
pub const fn revision(&self) -> Revision;
pub fn into_inner(self) -> T;
pub fn set<N>(&mut self, next: T, notifier: &mut N) -> bool
where T: PartialEq, N: ChangeNotifier + ?Sized;
pub fn update<N>(&mut self, f: impl FnOnce(&T) -> T, notifier: &mut N) -> bool
where T: PartialEq, N: ChangeNotifier + ?Sized;
```

值 inline 儲存，mutation 需要 `&mut self`。`set` / `update` 僅在結果不同時增加 revision、通知並回傳 `true`。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#local_counter{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#local_counter -->

## `Ref<T>`

```rust
pub fn Ref::new(value: T) -> Ref<T>;
pub fn get(&self) -> T where T: Clone;
pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R;
pub fn set<N>(&self, next: T, notifier: &mut N) -> bool
where T: PartialEq, N: ChangeNotifier + ?Sized;
pub fn update<N>(&self, f: impl FnOnce(&mut T), notifier: &mut N) -> bool
where T: Clone + PartialEq, N: ChangeNotifier + ?Sized;
pub fn ptr_eq(&self, other: &Self) -> bool;
```

`Ref<T>` 是單執行緒 `Rc<RefCell<T>>` handle；clone 共享同一值。`update` 會 clone 舊值以比較前後，因而對大型 `T` 是 O(size of T)。`read` callback 期間再次 mutably borrow 同一 ref 會遵循 `RefCell` 規則而 panic。

## Constructors

```rust
pub fn ref_<T>(value: T) -> Ref<T>;
pub fn reactive_ref<T>(value: T) -> Ref<T>;
```

兩者目前完全等價。尾端底線避開 Rust 的 `ref` keyword；`reactive_ref` 不建立 JavaScript Proxy。

## `ChangeNotifier`

```rust
pub trait ChangeNotifier { fn notify(&mut self); }
```

`FnMut()`、`()` 與 `gpui::Context<'_, V>` 已實作此 trait。傳 `()` 只更新資料，不讓 UI entity 重繪；共享 `Ref` 也不會自行找到其他讀者。

## 另見

- [Reactivity Utilities](/api/reactivity-utilities)
- [Reactivity Advanced](/api/reactivity-advanced)
- [Watchers 指南](/guide/essentials/watchers)
