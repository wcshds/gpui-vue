# Props

Props 是 parent 對 child 的 typed input。`component!` 會產生 `<Component>Props`、constructor、fluent default overrides 與 typestate builder；錯字、缺少 required prop 或型別不符都會留給巨集/rustc 在編譯時指出。

## Required 與 defaulted props

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#typed_props{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#typed_props -->

::: tip 執行結果
畫面顯示「永.kage · 未儲存」。刪除 `name`、把它改成整數，或寫未知 prop 都會讓 fixture 編譯失敗，而不是在執行時忽略。
:::

bare `dirty` 等同傳入 Rust `true`。`label="literal"` 會傳 `&'static str`，不會偷偷配置 `String`；目標 prop 若是 `String`，請寫 `label={String::from("...")}`。

## 直接建構 props

required props 出現在 `new` 參數，defaulted props 有 `with_*` override。同一 fixture 的 `construct_file_props` 也示範 typestate builder：setter 順序不限，只有 required fields 都設定後才存在 `build()`。

模板可用 `<FileStatus :props={props} />` 傳完整值。完整 `:props` 與個別 attrs 互斥，避免兩套來源競爭。

## 更新與比較

generated props derive `PartialEq`，所以每個 field type 都必須可比較。相同 visual identity 的 parent host 會在每幀比較 props；值真正變動時才替換 child input 並通知 child，state initializer 與 `setup` 不會重跑。

Props 在 child 中透過 `this.props()` 共享借用，不應直接修改。需要 child-to-parent 改變時，請 emit typed event，讓資料 owner 決定是否提供新 props。

## 目前限制

沒有 runtime validator、attribute coercion、prop mutation proxy、`defineProps` macro 或 fallthrough attrs。default 是 declaration 中的 Rust expression；只有全部 props 都有 default 時，generated props type 才實作 `Default`。

## 相關閱讀

- [元件事件](./events)
- [元件上的 `v-model`](./v-model)
- [Fallthrough Attributes](./attrs)
