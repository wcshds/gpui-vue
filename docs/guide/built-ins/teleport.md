# 浮層與原生定位

選單、popover 與 floating inspector 需要在原生視窗內避開邊界，並在祖先內容之後繪製。gpui-vue 的 `anchored_overlay` 與 `deferred_overlay` 正式包裝這兩個 GPUI 能力；它們不建立 DOM portal。

## 組合定位與繪製順序

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#overlay_demo{rust}

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#overlay_demo -->

::: tip 執行結果
Gallery 的 Show/Hide 按鈕控制一個 native popup。popup 以 anchor corner 和 offset 定位，距視窗四邊至少 12 logical pixels；deferred priority 讓它在祖先內容後繪製。
:::

兩層各自負責一件事：

- `anchored_overlay(child)` 決定 corner、window/local coordinate mode、offset，以及溢出時換 anchor 或貼齊視窗；
- `deferred_overlay(child)` 延後 paint，`priority` 越大越晚繪製。

anchoring 不會改變 paint order，所以常見 popup 會像範例一樣把 anchored element 包進 deferred boundary。child 不應帶外部 margin；把間距放進 child，避免 native anchored measurement 得到錯誤 bounds。

## Ownership 沒有被傳送

這些 helper 仍保留原本的 component owner、element tree、event routing、focus 與 lifecycle。它們沒有把 subtree 移到另一個 root，也沒有全域 target selector 或 overlay registry，因此不是 Vue `<Teleport>`。

`occlude` 可阻擋後方 pointer hit testing，但不是 modal policy。focus trap、Escape、預設按鈕、accessibility modality，以及關閉後恢復 focus，仍要由應用明確管理。

## 明確邊界

目前沒有跨 component-tree portal、process-wide/root overlay registry、跨視窗 target 或宣告式 `<Teleport to=...>`。`deferred_overlay` 只改繪製排程，`anchored_overlay` 只改原生視窗內的定位與 fitting。

等待資料的 popup 可把 `AsyncResource` state 當普通 input；非同步 ownership 見[非同步邊界](./suspense.md)。

## 相關閱讀

- [Built-in Components API](/api/built-in-components)
- [Focus 與原生 Handle](../essentials/template-refs)
