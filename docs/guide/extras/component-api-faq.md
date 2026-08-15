# 元件 API 常見問題

## 為何 required prop 不是 `Option<T>`？

required prop 由 typestate builder 在編譯期保證。只有資料領域本身允許缺值時才宣告 `Option<T>`；不要用它繞過 component contract。

## 為何 `String` prop 不能直接寫字面值？

setter 接受宣告的精確 Rust 型別，不做隱式配置。使用 `label={"Ready".to_owned()}`，或把 prop 定義成適合共享 UI 文本的 `SharedString`。

## 為何每個 `v-for` root 都要 `:key`？

GPUI 以 global element identity 保存 focus、listener 與 component mount state。索引或缺少 key 會讓 state 在重新排序時對到另一筆資料；應使用資料 ID。

## `v-show` 會觸發 `unmounted` 嗎？

不會。`v-show` 隱藏既有 intrinsic element；`v-if` 移除 keyed component visual host，或改變 component key，才會形成卸載。`unmounted` 是 native visual teardown，不是 process-finalization hook。

## Slot 為何要求 `'static` capture？

slot provider 可能由 retained child 在之後的 render 才執行。direct component markup 會捕捉 parent 的 weak entity並於執行時重入；獨立 `view!` 中自行建立的 provider則必須擁有其 capture。

## 為何 component event 不會冒泡？

它是 child entity 的 typed GPUI event stream。父 tag 的 `@change` 直接訂閱該 child；祖先不會因字串事件名自動收到。多個 listeners 在同一 tag 上共用一個 native subscription。

## 可以把 `Component::new` 當成掛載嗎？

不行。它建立裸 entity；`mounted`／`updated`／`unmounted` 由 persistent visual host 附加。root 使用 `DesktopApp::run_component`，nested component 使用 PascalCase tag，才能取得相同的 visual lifecycle。

## 為何沒有 provide/inject？

app-wide 資料可用 `state::Global` helpers，但它不等同 component-subtree scope。正式 provide/inject 尚未實作；目前以 props、slots、entity handles 或 root-owned context model 明確傳遞。

完整型別背景見[`component!` 區段與型別](../typescript/options-api.md)。
