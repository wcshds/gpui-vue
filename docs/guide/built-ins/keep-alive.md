# 保留非顯示中的元件

切換文件分頁時，使用者通常期待游標、捲動位置與未送出的文字仍在。單純以 `v-if` 移除子元件會卸載其 keyed visual host；下次加入時會建立新的 component entity。

## 目前狀態

`gpui-vue` 尚未提供 `<KeepAlive>` 或包含 LRU 上限的 component cache。`v-show` 可隱藏 intrinsic element，但不是任意 PascalCase 元件的 cache 協議，也沒有 activated/deactivated lifecycle。

## 現行安全模式：由文件 owner 保存狀態

把重要狀態提升到長壽命的 document entity，頁籤只決定哪個 editor subtree 可見。元件卸載不會遺失 document model；重新掛載時 props 傳入同一個 entity handle。

```text
Workspace entity
├─ documents: HashMap<DocumentId, Entity<DocumentModel>>
├─ active: DocumentId
└─ render: active document 的 EditorView
```

這是架構模式，不是可直接貼上的 `gpui-vue` API 片段。`Entity` 的建立與觀察目前由底層 GPUI context 完成；請讓 workspace 成為 handle 的明確 owner。

## 不要只保存視圖 entity

直接強持有由 `Component::new` 建立的 generated component entity 雖可保存欄位，卻繞過 persistent visual host 的完整語意：裸 entity 不會自動附加 `mounted`／`updated`／`unmounted` mount state。若要保存資料，優先保存獨立 model；視圖仍由正常的 PascalCase tag 掛載。

## 能力缺口

完整實作仍需 cache key、容量政策、視覺 detach/attach、focus 轉移、訂閱暫停，以及 activated/deactivated hooks。跨視窗移動時，GPUI 的 per-window element identity 也不能假設可直接轉移。

若你的目標只是把浮層放到另一個視覺區域，應閱讀[浮層與傳送](./teleport.md)，那是不同的生命週期問題。
