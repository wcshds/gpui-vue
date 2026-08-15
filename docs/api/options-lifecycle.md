# Options API: Lifecycle

生命週期 section 是 `component!` 的可選 declarations。只有至少宣告一個 hook 的 component 才產生 lifecycle mount state；沒有 hook 的 component 不付出該層 runtime bookkeeping。

## `mounted`

```rust
mounted(this, window, cx) { /* returns () */ }
```

在 visual host 第一次成功 delegated draw 之後排程。它不是 entity constructor：`state` 與 `setup` 已完成，parent component event subscription 也已建立。

## `updated`

```rust
updated(this, window, cx) { /* returns () */ }
```

在稍後 dirty delegated draw 後執行。相同 effect cycle 的多次 dirty work 會 coalesce。comparable props 改變、component notification 或保守的 slot reconciliation 都可能令 visual host dirty。

## `unmounted`

```rust
unmounted(this, cx) { /* cx: &mut App */ }
```

visual identity 因條件移除、key 改變或 host drop 而消失後執行，至多一次。沒有 `Window` 參數，因為舊視窗可能已不可安全使用。

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#lifecycle_probe{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#lifecycle_probe -->

## Ordering 與限制

- nested component 的 mounted/updated 通常由 descendant 先排程，再到 ancestor；同層 teardown order 不保證。
- `v-show` 只 hidden intrinsic，不 unmount。
- naked `Component::new` entity 沒有 visual hook；root 使用 `run_component`。
- application shutdown 時，外部刻意持有 entity 可能使 queued teardown 不被 foreground executor polling；不要把 `unmounted` 當 process finalizer。

## 另見

- [Composition Lifecycle](/api/composition-api-lifecycle)
- [Lifecycle 指南](/guide/essentials/lifecycle)
