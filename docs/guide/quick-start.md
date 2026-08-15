# 快速開始

這一頁會建立一個只有 Rust 與原生 GPUI 的桌面計數器。整個流程不需要 Node.js、WebView 或前端打包器；Node.js 只用於建置你正在閱讀的文件網站。

## 前置條件

請先準備：

- 最新穩定版 Rust 與 Cargo；
- 目標平台所需的 GPUI 系統相依項；
- macOS、Linux，或 GPUI host 支援的其他桌面環境。

GPUI 的 Linux backend 可能需要額外的視窗與字型開發套件。若首次連結失敗，請依編譯器訊息安裝對應平台套件。

## 建立專案

```bash
cargo new hello-gpui-vue
cd hello-gpui-vue
cargo add gpui-vue --git https://github.com/wcshds/gpui-vue --features desktop
```

若你正在這個 repository 內開發，可以改用 path dependency：

```toml
[dependencies]
gpui-vue = { path = "../gpui-vue/crates/gpui-vue", features = ["desktop"] }
```

`desktop` feature 只負責原生應用啟動相關能力。模板、元件、狀態與 headless-friendly 測試預設不需要它。

## 寫下第一個視窗

以以下內容取代 `src/main.rs`：

```rust
use gpui_vue::desktop::{DesktopApp, WindowConfig};
use gpui_vue::prelude::*;

component! {
    /// 應用程式的根元件。
    component AppRoot {
        state {
            /// 目前顯示的計數。
            count: Local<i32> = Local::new(0),
        }

        template(this, _window, cx) {
            <view class="h-full w-full flex flex-col items-center justify-center gap-4 bg-slate-950 text-white">
                <text class="text-sm text-slate-400">"Native GPUI · Rust"</text>
                <text class="text-4xl font-bold">
                    {format!("{}", this.count.get())}
                </text>
                <button
                    id="increment"
                    class="rounded-lg bg-blue-600 px-5 py-3 font-semibold hover:bg-blue-500 active:bg-blue-700"
                    @click={cx.listener(|this, _, _, cx| {
                        this.count.update(|count| count + 1, cx);
                    })}
                >
                    "增加"
                </button>
            </view>
        }
    }
}

fn main() {
    let window = WindowConfig::new("Hello gpui-vue", 720.0, 480.0)
        .min_size(520.0, 360.0);

    DesktopApp::new(window)
        .run_component(|_, cx| AppRoot::new(AppRootProps::new(), cx));
}
```

接著執行：

```bash
cargo run --locked
```

你應該會看到一個原生視窗。每次點擊按鈕時，`Local<i32>` 更新 revision，並透過傳入的 context 通知 GPUI 重新 render 根元件。

## 剛才發生了什麼？

### `DesktopApp` 建立原生應用

`WindowConfig` 保存第一個視窗的標題、尺寸與原生 titlebar 選項。`run_component` 將 `component!` 產生的 entity 掛到帶生命週期的根 host。

### `component!` 定義狀態與模板

`state` 欄位只在 entity 建立時初始化一次。`template` 則是原生 `Render` 實作；當 context 收到通知時，它會再次產生目前這一幀的 GPUI 元素。

### 模板需要穩定身份

互動元素需要穩定的 `id`。在 `v-for` 內，則必須提供由項目資料產生的動態 `:key`。這讓 GPUI 能可靠地保留焦點、輸入與其他 element state。

### `class` 不是執行期 CSS

class literal 在 proc macro 展開時就會驗證並降低為 typed GPUI calls。未知 utility、無法可靠映射的瀏覽器語義，以及任意執行期 class 字串都會被拒絕。

## 執行 repository 內的範例

在 gpui-vue repository 根目錄可以直接執行：

```bash
cargo run --locked -p gpui-vue --example counter --features desktop
cargo run --locked -p kage-editor
```

先閱讀 [Counter 範例](/examples/counter) 掌握模板與狀態，再前往 [KAGE Editor](/examples/kage-editor) 了解大型原生工具如何組織畫布、輸入、網路與多面板狀態。

## 下一步

- 在 [API 總覽](/api/) 中選擇下一個要學習的模組。
- 閱讀 [架構決策](/architecture)，理解為何 gpui-vue 不維護 VDOM。
- 遇到 Vue 或 Tailwind 寫法是否可用的疑問時，查閱 [能力矩陣](/capability-matrix)。
