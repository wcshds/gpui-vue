# 可組合邏輯

桌面程式很快會重複遇到「選取一個檔案後更新狀態、啟動背景工作、完成時重繪」這類流程。`gpui-vue` 不需要 Hook 執行器；可組合邏輯就是普通 Rust 函式、資料型別與明確的 GPUI context。

## 從一個可測試的狀態型別開始

把不依賴視窗的規則留在普通型別中，再由元件決定何時通知畫面：

```rust
use gpui_vue::{ChangeNotifier, Local};

struct Selection {
    index: Local<Option<usize>>,
}

impl Selection {
    fn new() -> Self {
        Self { index: Local::new(None) }
    }

    fn select(&mut self, index: usize, notify: &mut impl ChangeNotifier) {
        let _ = self.index.set(Some(index), notify);
    }
}
```

::: tip 執行結果
呼叫 `select(3, &mut notifier)` 後，`index` 變成 `Some(3)` 並通知一次；再次選取 3 會被 `Local::set` 的相等比較抑制，不產生多餘通知。
:::

將 `Selection` 放進 `component!` 的 `state` 區段後，事件處理器可傳入 `cx`。相同邏輯也能在單元測試裡傳入閉包計數器或 `()`，不必啟動桌面視窗。

## 何時回傳 `Ref<T>`

只有多個擁有者確實要共用同一個 cell 時才使用 `Ref<T>`。它的 clone 共用 `Rc<RefCell<T>>`，但並不追蹤誰讀取了值；`set` 或 `update` 只通知當次明確傳入的 `ChangeNotifier`。跨多個元件共享、且每位讀者都要失效時，應把狀態放進 GPUI `Entity<T>`，透過原生 `observe`／`subscribe` 建立關係。

## 把平台工作留在邊界

需要 `Window`、剪貼簿或非同步 executor 的函式，應明確接收相應參數。這比隱藏式全域 Hook 更容易看出生命週期：

```rust
use gpui_vue::ui::{App, write_clipboard_text};

fn copy_selection(app: &App, text: &str) {
    write_clipboard_text(app, text.to_owned());
}
```

owner-safe future 使用 `spawn(cx, operation)`；若 `.await` 後需要更新原視窗，使用 `spawn_in(cx, window, operation)`。兩者回傳的 native `Task` 必須由 caller 保存，drop 就取消。若 composable 還要公開 loading/ready/error state、reload 與 stale-result protection，直接讓 owner 保存 `AsyncResource<Value, Error>`；完整模式見[非同步元件邊界](../components/async.md)。

## 目前限制

目前沒有讀取任意 `Ref` 的 `watchEffect`、自動依賴收集、Hook 呼叫順序或 cleanup-on-rerun。`watch_entity`／`watch_event` 已提供 typed native source，返回的 `Subscription` 可交由 `EffectScope` 統一擁有；scope drop 或 `clear` 會確實解除訂閱。其他資源則由 RAII、元件的 `unmounted` 或 `on_release` 完成清理。

下一步可閱讀[狀態管理](../scaling-up/state-management.md)，判斷資料應留在單一元件、共享 handle，還是應提升為 entity。
