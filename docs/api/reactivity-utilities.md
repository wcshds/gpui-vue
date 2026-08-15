# Reactivity API: Utilities

這些 utilities 用來辨識狀態版本與控制讀取成本；它們不是 Vue proxy introspection API。

## `Revision`

```rust
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Revision(/* private */);

pub const Revision::ZERO: Revision;
pub const Revision::MAX: Revision;
pub const fn Revision::from_raw(raw: u64) -> Revision;
pub const fn get(self) -> u64;
pub const fn next(self) -> Revision;
```

`Local<T>` 每次有效 mutation 都前進一個 revision。計數使用 wrapping arithmetic；`MAX.next() == ZERO`，不會在 debug build panic。

```rust
use gpui_vue::{Local, Revision};

let value = Local::new(7_u32);
assert_eq!(value.revision(), Revision::ZERO);
assert_eq!(Revision::from_raw(41).next().get(), 42);
```

同一組 `Local` / `Revision` API 也被可執行 gallery 的 memo helper 使用：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#memo_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#memo_demo -->

## 選擇讀取方式

| 容器 | Clone 讀取 | callback 借用 | 直接借用 |
| --- | --- | --- | --- |
| `Local<T>` | `get()` | `read(f)` | `as_ref()` |
| `Ref<T>` | `get()` | `read(f)` | 無，避免 `RefCell` borrow 外洩 |

`Local::into_inner` 消耗容器並移出值。`Ref::ptr_eq` 比較 handle identity，不比較 `T` 的內容。

```rust
use gpui_vue::ref_;

let first = ref_(String::from("永"));
let same = first.clone();
let other = ref_(String::from("永"));
assert!(first.ptr_eq(&same));
assert!(!first.ptr_eq(&other));
assert_eq!(first.read(String::len), 3); // UTF-8 bytes
```

## 不提供的 Vue utilities

沒有 `isRef`、`unref`、`toRef`、`toRefs`、`isProxy` 或 `toRaw`。Rust 型別已在編譯期區分 `T`、`Local<T>` 與 `Ref<T>`；gpui-vue 也沒有 proxy/raw 雙重身份可檢查。

## Panics

`Ref::read` 內重入同一 cell 的 mutation 可能造成 `RefCell` borrow panic。`Revision` 自身不因 overflow panic。

## 另見

- [Reactivity Core](/api/reactivity-core)
- [Utility Types](/api/utility-types)
