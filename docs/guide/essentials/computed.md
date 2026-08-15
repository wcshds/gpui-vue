# 計算與快取

有些顯示值可在每次 render 直接算出；另一些計算較昂貴，只應在輸入變動後重跑。gpui-vue 的 `Memo<T, D>` 是顯式依賴鍵快取，不會掃描 closure 讀取了哪些狀態。

## 以 revision 作為依賴

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#square_counter{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#square_counter -->

::: tip 執行結果
按鈕由「2² = 4」開始。每次點擊只讓 count revision 前進一次；下一幀重算平方值，未變動的 render 則沿用 `Memo` cache。
:::

第一次呼叫 `get_or_update` 會執行 closure。之後只要 `Revision` 相等，就回傳快取中的 `&T`；`Local` 發生有效 mutation 時 revision 前進，下一次 render 才重算。

先把 dependency 與計算需要的資料讀到區域變數，可讓 Rust 清楚看見 `count` 和 `square` 是兩個互不重疊的欄位借用。

## 多個依賴

依賴鍵 `D` 可以是任何 `PartialEq` type，常見做法是 revision tuple：

```rust
use gpui_vue::{Local, Memo};

fn memo_area() {
    let width = Local::new(12_u32);
    let height = Local::new(8_u32);
    let mut area = Memo::<u32, (_, _)>::new();

    let key = (width.revision(), height.revision());
    let value = *area.get_or_update(key, || width.get() * height.get());
    assert_eq!(value, 96);
}
```

也可以用 domain-specific key，例如 `(document_id, zoom_level)`。`invalidate()` 會主動丟棄 key 與值；`get()` 和 `dependencies()` 則只檢查現有 cache，不觸發計算。

## 何時不需要 `Memo`

字串格式化、簡單算術或短列表篩選通常直接在 render 計算更清楚。`Memo` 不會通知 UI，也不會自行安排 render；它只避免相同 key 下重做 closure。

`Memo` 與 Vue `computed` 的共同點是按需快取，差異是依賴完全由呼叫者提供。目前沒有 read-only / writable computed ref、依賴自動追蹤或跨 entity invalidation。

::: warning 尚未實作
gpui-vue 尚無能自動收集 `Local` / `Ref` 讀取的 computed graph。不要把 `Memo` 的 dependency key 當作框架自動保證；漏放一個 revision 會得到過期結果。
:::

## 相關閱讀

- [反應式狀態基礎](./reactivity-fundamentals)
- [Watchers](./watchers)
- [元件生命週期](./lifecycle)
