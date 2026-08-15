---
layout: home
title: gpui-vue
titleTemplate: false

hero:
  name: gpui-vue
  text: 用熟悉的宣告式思路，構建真正的原生桌面介面
  tagline: Vue 啟發的 Rust 編譯期語法，直接生成 GPUI 元素。沒有 JavaScript 執行環境、VDOM 或執行期 CSS 解析器。
  actions:
    - theme: brand
      text: 開始使用
      link: /guide/quick-start
    - theme: alt
      text: 查看 API
      link: /api/

features:
  - title: 編譯期模板
    details: view! 與 component! 在編譯時完成語法檢查和程式碼生成，錯誤停在來源位置，而不是留到執行期。
  - title: 原生 GPUI
    details: 元素、佈局、輸入、焦點、文字與 GPU 渲染仍由 GPUI 擁有；gpui-vue 不維護第二棵 UI 樹。
  - title: Rust 型別邊界
    details: Props、事件、插槽與狀態都保留靜態型別，元件之間不需要字串註冊表或動態屬性映射。
---

<div class="home-ledger">
  <section class="home-ledger__item">
    <p class="home-ledger__eyebrow">Guide</p>
    <h2><a href="./guide/introduction">先建立正確心智模型</a></h2>
    <p>理解模板如何編譯成 GPUI、何時需要穩定識別，以及本地狀態如何觸發原生更新。</p>
  </section>
  <section class="home-ledger__item">
    <p class="home-ledger__eyebrow">Examples</p>
    <h2><a href="./examples/kage-editor">從完整桌面應用學習</a></h2>
    <p>KAGE Editor 展示畫布、IME、網路請求、剪貼簿、手勢與多面板工作區的實際組合。</p>
  </section>
  <section class="home-ledger__item">
    <p class="home-ledger__eyebrow">Reference</p>
    <h2><a href="./capability-matrix">精確了解能力邊界</a></h2>
    <p>逐項區分已實現、部分實現、原生語義不同與僅適用於 Web 的能力，避免模糊相容性承諾。</p>
  </section>
</div>

## 同一份程式碼，同一個原生結果

下圖由可執行的 `docs_gallery` GPUI example 截取。Guide 的核心程式碼區塊直接 include 該 example 的 source regions，CI 也會實際編譯它，因此文件中的程式碼與可見結果不會分成兩份手動維護的示意。

![gpui-vue Guide Gallery 的原生 macOS 執行結果](/screenshots/guide-gallery.png)

```bash
cargo run --locked -p gpui-vue --example docs_gallery --features desktop
```

Guide 的六張核心結果圖也不是總覽裁圖。可在命令末尾加上 `-- local-counter`、`status-view`、`component-card`、`layer-list`、`search-panel` 或 `registration`，單獨掛載同一個已編譯元件並重現對應畫面。
