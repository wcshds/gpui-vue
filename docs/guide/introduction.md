# 介紹

`gpui-vue` 是一層面向 GPUI 的 Rust 編譯期介面語法。這個 workspace 固定使用 [GPUI-CE](https://github.com/MostlyKIGuess/gpui-ce) 的相容 revision，以保留目前依賴的原生輸入與手勢能力。它借鑑 Vue 清晰的宣告式模板與元件組織方式，但不嵌入 Vue、JavaScript 或瀏覽器執行環境。

你寫下的 `view!` 和 `component!` 會在編譯時展開為普通的 GPUI 元素建構器。視窗、實體、輸入、焦點、佈局、文字與繪製仍由 GPUI 負責，因此應用程式只有一棵原生 UI 樹。

## 一個最小元件

```rust
use gpui_vue::prelude::*;

component! {
    /// 顯示並更新一個本地計數值。
    component Counter {
        state {
            /// 當前計數。
            count: Local<i32> = Local::new(0),
        }

        template(this, _window, cx) {
            <button
                id="counter"
                class="rounded-lg bg-blue-600 px-4 py-2 text-white hover:bg-blue-500"
                @click={cx.listener(|this, _, _, cx| {
                    this.count.update(|count| count + 1, cx);
                })}
            >
                {format!("Count: {}", this.count.get())}
            </button>
        }
    }
}
```

這段程式碼同時展示了三件核心事情：

- `component!` 產生有型別的 Rust 元件、輸入與建構 API。
- `Local<T>` 把狀態直接保存在元件實體內，更新時透過 GPUI context 通知重繪。
- `view!` 形狀的模板在編譯期轉換，`class` 也會被解析成有型別的 GPUI style calls。

## 它與 Vue 有何不同？

熟悉 Vue 會讓模板結構更容易閱讀，但兩者的執行模型並不相同。

| 面向 | gpui-vue | Web Vue |
| --- | --- | --- |
| 表達式 | Rust 表達式與型別 | JavaScript / TypeScript |
| 輸出目標 | 原生 GPUI 元素 | DOM 或自訂 renderer |
| 更新排程 | GPUI entity 通知與 render | Vue reactivity scheduler |
| 樣式 | 編譯期支援的 class 子集與 typed style | CSS cascade |
| 元件邊界 | Rust structs、traits 與 GPUI Entity | Vue component instance |

這不是一個「在桌面裡執行網頁」的方案，也不是完整 Vue 原始碼相容層。當瀏覽器語義沒有可靠的原生對應時，gpui-vue 會提供明確的原生 API，或在編譯期拒絕不準確的近似。

## 適合用來做什麼？

gpui-vue 適合希望保留 Rust 與 GPUI 原生能力，同時想要更緊湊宣告式介面的桌面應用，例如：

- 工程與創作工具；
- 多面板資料檢視器；
- 需要鍵盤、滑鼠、觸控板或 IME 的生產力應用；
- 帶有自訂繪製表面、網路資料與原生剪貼簿的工具。

如果你的主要需求是 DOM、瀏覽器 CSS、SSR 或既有 Vue npm 生態，直接使用 Vue 會更合適。

## 你需要知道什麼？

本指南假設你已熟悉 Rust 的基本所有權、閉包與泛型。你不必先精通 GPUI；不過在處理 entity、context、非同步 task 或自訂繪製時，理解 GPUI 的生命週期會很有幫助。

建議依序閱讀：

1. [快速開始](/guide/quick-start)，建立並執行第一個原生視窗。
2. [API 總覽](/api/)，了解目前公開的模組與穩定邊界。
3. [Counter 範例](/examples/counter) 與 [KAGE Editor](/examples/kage-editor)，查看從小型元件到完整工具的組織方式。
4. [能力矩陣](/capability-matrix)，在採用某項 Vue 或 Tailwind 寫法前確認精確語義。

::: warning 專案狀態
gpui-vue 目前是持續演進中的原生編譯器前端，不宣稱完整 Vue 或 Tailwind 相容性。文件會把已支援、原生差異與尚未實現的能力分開說明。
:::
