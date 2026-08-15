# 安全

`gpui-vue` 沒有 `v-html` 或 DOM HTML parser，文字 expression 會成為原生文字內容，這消除了一類 HTML injection；它不會自動保護檔案、網路、clipboard 或 command 邊界。

## 把不可信內容當資料

<!-- verified: crates/gpui-vue/examples/docs_gallery.rs#untrusted_text_demo -->

<<< ../../../crates/gpui-vue/examples/docs_gallery.rs#untrusted_text_demo{rust}

::: tip 執行結果
即使 `value` 包含 `<button>` 或 `</text>`，它也以文字顯示，不會被重新解析成 template 或建立可點擊元素。
:::

`view!` 的 tag、binding 與靜態 class 都在編譯期決定。不要為了載入遠端配置而自行增加「字串轉 template」或任意 Rust/JS evaluator；這會破壞目前的安全邊界。

## 原生 I/O 邊界

- 驗證檔案路徑，避免把使用者片段直接 join 到可寫入的任意位置。
- HTTP 回應要有大小、timeout、content-type 與解析深度限制；遠端 image 也會消耗 native decoder 資源。
- clipboard 內容是不可信輸入；解析 KAGE、JSON 或 URI 時回傳可呈現的錯誤，不要 panic。
- 若啟動外部 command，使用結構化 argv，不要把輸入拼成 shell 字串。
- secret 不放入 `EmbeddedAssets`；它會進入可執行檔。

## 依賴供應鏈

保留 `Cargo.lock`，審核固定 git revision 與授權，在 CI 使用 `--locked`。Node/VitePress 依賴也使用 lockfile；文件 build 不應在發布時執行不受信任的遠端程式。

Web 的 CSP、cookie、same-origin 與 XSS header 不直接適用於原生 renderer；若應用另外嵌入 WebView，該邊界必須獨立威脅建模，不能視為 `gpui-vue` 的一部分。
