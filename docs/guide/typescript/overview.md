# Rust 型別系統

`gpui-vue` 不需要額外的 TypeScript 層：template expression、props、events、slots 與 GPUI elements 都是 Rust。macro 只產生 Rust items，最終由 rustc 檢查所有權、生命週期與 trait bounds。這一節對應 Vue 文件中的型別主題，但使用的是 Rust。

## Props 在建構時完整

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_props_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#typed_props_demo{rust}

::: tip 執行結果
`ComponentCardProps` builder 只有在 required `label` 已提供後才暴露可用的 `build`；defaulted `tone` 可保持預設或設成「Rust typestate」。錯字和錯誤型別由 rustc 拒絕。
:::

Required props 使用 sealed typestate `RequiredProp<T, PropMissing/PropSet>`，不依賴 runtime map。generated props derive `PartialEq`，因此 prop fields 也必須支援比較。

## 沒有 template type island

`{expression}` 與 `:prop={expression}` 仍遵循 Rust move/borrow 規則。string literal 不會自動變成 `String`；宣告 `String` prop 時需寫 `"value".to_owned()`。kebab-case prop 名會正規化到 snake_case setter。

## 常見界線

Rust 的 `Option<T>` 對應資料是否存在，enum 表達有限狀態，`Result<T, E>` 表達失敗；不要以多個布林旗標模擬它們。`SharedString` 適合頻繁 clone 的 UI label，domain model 則選擇最符合所有權的型別。

進一步看[組合式寫法的型別](./composition-api.md)與[component 區段的型別](./options-api.md)。
