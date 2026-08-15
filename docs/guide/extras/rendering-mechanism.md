# 渲染機制

理解 `view!` 的關鍵不是把它想成另一個 DOM，而是把它看成 GPUI builder 的編譯期語法。執行程式中沒有 template interpreter、VDOM 或 CSS engine。

## 從 source 到 native element

```text
Rust token stream
  → proc macro 驗證 tag、binding、結構指令與 class
  → 產生 typed GPUI element builder calls
  → rustc 做型別與所有權檢查
  → GPUI layout / prepaint / paint
```

static `class` 與可列舉的 `:class` 分支都在 expansion 時降為 style methods。runtime `:style` 只接受 `StyleRefinement → StyleRefinement` callback，不解析 CSS string。

## `v-if` 與 `v-for`

條件分支直接成為 Rust control flow；`v-for` 消費 iterator 並要求 root `:key`。key 會 namespace 迭代內的 element state，所以資料重排不會把 focus 或 component entity交給另一列。

## Component 為何能保留 state

PascalCase tag 降為 `ComponentElement`。GPUI 以 per-window global element ID 保存 `ComponentMount`；後續 frame reconcile input 到同一 child entity。`HostedEntity` 直接委派 child 的 layout、prepaint 和 paint，沒有額外 host div。props 只在 `PartialEq` 變更時 notify；slot closure 每次替換且保守標記 lifecycle dirty。

## Visual lifecycle

首次 delegated draw 後排入 `mounted`；之後 dirty draw 後排入 `updated`；keyed host 消失時最多執行一次 `unmounted`。child 的 deferred layout effect 先於 ancestor。這是 GPUI effect-cycle 語意，不是 DOM paint 時序。

## 成本邊界

Template element 值仍會依 render 重建；retained 的是 GPUI/entity/element state，而非一棵 `gpui-vue` VDOM。需要完全自訂 layout/paint 時，使用 typed element 或 `paint::drawing_surface`，不要另建平行 renderer。實作 helper 的方法見[渲染函式](./render-function.md)。
