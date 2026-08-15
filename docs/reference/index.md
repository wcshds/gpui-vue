# 參考資料

這一區記錄設計約束、精確能力邊界與實作決策。它不是入門閱讀順序；第一次使用 gpui-vue，請先從 [介紹](/guide/introduction) 和 [快速開始](/guide/quick-start) 開始。

## 架構決策

[閱讀架構決策](/architecture)

說明為什麼模板直接編譯到 GPUI builders、為什麼不嵌入 JavaScript runtime / VDOM，以及 component host、slots、events、lifecycle 與 typed utility cascade 如何維持單一原生 UI tree。

## 能力矩陣

[閱讀能力矩陣](/capability-matrix)

逐項比較 Vue-shaped syntax、Tailwind-oriented classes 與 GPUI host 的實際語義。矩陣中的狀態採嚴格定義：

- **Implemented**：目前 source 中存在，且有測試或完整範例覆蓋；
- **Partial**：可用子集已實現，同一列會寫明缺口；
- **Next**：適合原生設計，但尚未實現；
- **Host-different**：可以有原生類比，但不能宣稱 DOM / CSS 等價；
- **Not targeted**：刻意不放入原生 hot path。

當指南與矩陣描述不一致時，以目前 source、測試與矩陣中更保守的邊界為準，並應修正文檔，而不是擴大相容性宣稱。

## API 與範例

- [API 總覽](/api/)：從公開 Rust 介面出發尋找正確模組。
- [Counter](/examples/counter)：最小原生狀態與模板範例。
- [KAGE Editor](/examples/kage-editor)：多面板、畫布、IME、網路與桌面整合。
