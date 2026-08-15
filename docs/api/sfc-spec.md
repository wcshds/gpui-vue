# Rust Component File Format

gpui-vue 沒有 `.vue` Single-File Component parser。對應的 source unit 是普通 `.rs` 模組，其中 `component!` 把 component contract 放在一起，其他 Rust items 則承擔 imports、types、helpers 與 tests。

## Canonical shape

```rust
use gpui_vue::prelude::*;
use gpui_vue::ui::SharedString;

component! {
    /// File status rendered by a native entity.
    pub component FileStatus {
        props {
            /// File name.
            pub name: SharedString,
        }
        state {
            /// Whether the file changed.
            pub dirty: Local<bool> = Local::new(false),
        }
        template(this, _window, _cx) {
            <text>{format!("{}{}", this.props().name, if this.dirty.get() { " *" } else { "" })}</text>
        }
    }
}
```

component section ordering可自由安排，但每種至多一次且 `template` 必須存在。markup、styles 與 state declaration 由同一次 Rust compilation檢查。

可執行 repository fixture 中的同類完整宣告：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#typed_stepper{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_stepper -->

## 與 Vue SFC 的映射

| Vue SFC | gpui-vue |
| --- | --- |
| `<script setup>` / `<script>` | module imports、Rust types/functions、`state` / `setup` sections |
| `<template>` | `template(this, window, cx)` 的 direct markup 或 `view!` |
| `<style>` | compile-time `class` utilities、typed `:style`、native builders |
| compiler macros | `component!` 產生 props/events/slots/entity contracts |

## 不支援

沒有 `.vue` loader、JavaScript/TypeScript execution、file-level `<style scoped>`、CSS Modules、custom block、source-map 到 SFC block 或 hot module replacement。這些是工具鏈/瀏覽器 frontend 能力，不應假裝由 Rust macro 提供。

## 另見

- [Rust 單檔元件指南](/guide/scaling-up/sfc)
- [Component Setup](/api/component-setup)
- [Native Style Features](/api/native-style-features)
