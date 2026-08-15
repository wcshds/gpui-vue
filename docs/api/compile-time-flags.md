# Compile-time Flags

gpui-vue 以 Cargo features控制需要平台 backend 的部分。沒有 Vue-style runtime compiler flags或 bundler global replacement。

## Cargo features

```toml
[dependencies]
gpui-vue = { version = "0.1", features = ["desktop"] }
```

| Feature | Default | 內容 |
| --- | --- | --- |
| `desktop` | 否 | 啟用 `gpui_vue::desktop`，並開啟 host font-kit、Wayland、X11 backend features |

default feature set 為空。`view!`、`component!`、reactivity、effects（包含 `spawn` / `spawn_in`）、`AsyncResource` / async state、anchored/deferred overlay、text input types、animation、media、paint、HTTP contract與virtual-list bridge不以額外 gpui-vue feature分拆。

`counter` 與 `docs_gallery` Cargo examples標記 `required-features = ["desktop"]`，因此執行時需：

```sh
cargo run -p gpui-vue --example docs_gallery --features desktop --locked
```

該命令編譯並執行的根 app 與文件引用同一份 source：

<<< ../../crates/gpui-vue/examples/docs_gallery.rs#app_root{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#app_root -->

## `cfg` 與平台差異

視窗 backend、pinch gesture、IME、clipboard、window material與filesystem能力仍會因 target OS / host build不同。Cargo feature只決定編譯哪些 backend，不保證所有平台呈現相同 affordance。

## 不存在的 flags

沒有 `__VUE_OPTIONS_API__`、`__VUE_PROD_DEVTOOLS__`、hydration mismatch details或 browser devtools flags。Rust release/debug、lints與host features由 workspace Cargo profile控制。

## Errors

未啟用 `desktop` 時匯入 `gpui_vue::desktop` 會在編譯期失敗；這是預期的 feature boundary。若只建立 library components或headless tests，無需啟用該 feature。

## 另見

- [Application API](/api/application)
- [Production Deployment](/guide/best-practices/production-deployment)
