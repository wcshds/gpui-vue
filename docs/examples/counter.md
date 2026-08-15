# Counter

Counter 是最小但完整的桌面範例。它把元件內狀態、條件渲染、帶 key 的列表、互動 class 與原生視窗放在同一個檔案中，適合先用來確認本機 GPUI toolchain。

## 執行

在 repository 根目錄執行：

```bash
cargo run --locked -p gpui-vue --example counter --features desktop
```

程式會開啟一個 720 × 520 logical-pixel 視窗。第一個按鈕增加計數，第二個按鈕切換衍生資訊區。

<NativeResult
  src="/screenshots/counter.png"
  alt="實際執行 gpui-vue Counter example 的原生畫面"
  caption="畫面由本頁直接 include 的 counter.rs 編譯執行後擷取。"
/>

## 值得注意的部分

### 元件、狀態與模板來自同一份可執行來源

<<< ../../crates/gpui-vue/examples/counter.rs#counter_component{rust}

<!-- verified: crates/gpui-vue/examples/counter.rs#counter_component -->

兩個 `Local` 都直接屬於 `Counter`。按鈕 listener 收到 component context，再由 `update` 變更值並通知 render。

### 條件是 Rust `bool`

`v-if` 不使用 JavaScript truthiness。表達式必須由 rustc 檢查為 `bool`。

### 列表的身份是明確的

每個 `v-for` root 都需要由項目產生的動態 key。這個規則比 Vue 的無 key list 更嚴格，目的是保護 GPUI element state 與 stateful descendants。

## 啟動層

<<< ../../crates/gpui-vue/examples/counter.rs#counter_main{rust}

<!-- verified: crates/gpui-vue/examples/counter.rs#counter_main -->

範例直接使用 `DesktopApp` / `WindowConfig`，並透過 `run_component` 掛載 root component，因此 root 與巢狀元件都會取得一致的 visual lifecycle。

下一步可以閱讀 [KAGE Editor](/examples/kage-editor)，看看同一套元件和模板能力如何延伸到畫布型專業工具。
