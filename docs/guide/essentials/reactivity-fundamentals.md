# 反應式狀態基礎

在原生 GPUI 中，資料變更不會經過 JavaScript proxy。gpui-vue 提供兩種小型容器，並把「哪個 entity 需要重繪」保留為明確選擇：元件自有狀態使用 `Local<T>`，需要共享 clone handle 時使用 `Ref<T>`。

## 元件內的 `Local<T>`

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#local_counter{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#local_counter -->

::: tip 執行結果
按鈕初始顯示「計數：0」。每點擊一次，`Local<i32>` 增加 1、通知 `LocalCounter` entity，按鈕文字在下一次 render 更新。
:::

<NativeResult
  src="/screenshots/gallery-state.png"
  alt="單獨執行的 LocalCounter 原生範例，初始顯示計數 0"
  caption="以 docs_gallery local-counter 啟動上方同一個 LocalCounter 後擷取。"
/>

`Local<T>` 將值 inline 放進 component struct。`set` 或 `update` 只有在新舊值不同時才增加 `Revision` 並呼叫傳入的 `ChangeNotifier`；`gpui::Context` 已實作這個 trait，因此範例中的 `cx` 會通知目前 entity 重繪。

讀取時可依成本選擇：

```rust
use gpui_vue::Local;

fn read_title() {
    let title = Local::new(String::from("Editor"));
    let copied = title.get();              // T: Clone
    let length = title.read(String::len);  // callback 期間借用
    let borrowed = title.as_ref();         // 直接 &T
    let _ = (copied, length, borrowed);
}
```

## 共享的 `Ref<T>`

`Ref<T>` 內部使用單執行緒 `Rc<RefCell<T>>`。clone 得到同一份資料的 handle：

```rust
use gpui_vue::ref_;

fn shared_ref() {
    let count = ref_(0);
    let sibling = count.clone();
    let mut notifications = 0;

    count.set(1, &mut || notifications += 1);
    assert_eq!(sibling.get(), 1);
    assert_eq!(notifications, 1);
}
```

每次 mutation 仍需明確傳入 notifier。若傳 `&mut ()`，資料會更新但沒有 UI entity 被通知；這適合無畫面的資料操作，不適合期待畫面自動刷新。

## 普通欄位也可以是狀態

`state` section 接受任何具體 Rust type，不強制包成 `Local`。普通欄位可在 `cx.listener` 中修改，再手動 `cx.notify()`。`Local` 的價值在於相等抑制、revision 與一致的 mutation 入口，不是能否保存資料。

## 與 Vue reactivity 的界線

目前沒有自動 dependency graph、deep proxy、`watchEffect` 或微任務 scheduler。render 讀取 `Local` / `Ref` 不會自動註冊依賴；mutation 只通知你交給它的 notifier。這讓 entity ownership 與排程保持可見，也表示共享 `Ref` 不會自行找出所有讀取者。

對跨 entity 狀態，通常應由擁有資料的 GPUI entity 更新並讓其他 entity 透過原生 observe/subscribe 模式接收變化。gpui-vue 尚未提供高階 store abstraction。

## 相關閱讀

- [計算與快取](./computed)
- [Watchers](./watchers)
- [事件處理](./event-handling)
- [反應式 API](/api/)
