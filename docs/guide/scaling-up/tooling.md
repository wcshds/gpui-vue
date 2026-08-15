# 工具鏈

原生 UI 的正確性同時跨過 Rust 型別、macro expansion、平台 backend 與文件範例。開發迴圈應把快速的 headless 檢查與較慢的桌面檢查分開。

## 與 CI 相同的完整檢查

```bash
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets --no-default-features
cargo clippy --locked --workspace --all-targets --no-default-features -- -D warnings
cargo check --locked --workspace --all-targets --no-default-features --features desktop
cargo clippy --locked --workspace --all-targets --no-default-features --features desktop -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps --no-default-features
pnpm install --frozen-lockfile
pnpm docs:check
```

platform-neutral test 與 Clippy 可在沒有視窗伺服器的環境驗證 macro、reactivity 與大部分型別；帶 `desktop` feature 的 check 與 Clippy 會再編譯平台啟動、window 與 IME 路徑，但不開啟視窗。rustdoc 與兩次 Clippy 都把 warning 視為錯誤；最後兩步以提交的 pnpm lockfile 安裝依賴，再同時執行文件結構驗證與 VitePress build。

::: tip 執行結果
所有命令成功時，Rust 程式、desktop feature 與 VitePress dead-link 檢查同時通過。任何不支援的 tag、binding、class utility 或錯誤 event signature 會在 Cargo 步驟中成為編譯錯誤。
:::

## 編輯器支援

安裝 rust-analyzer，讓 generated Rust item 的錯誤回到 component 呼叫點。proc macro 報錯以 Rust source span 為準；它不需要 Volar，也沒有瀏覽器 DevTools element inspector。複雜 markup 若難以推斷型別，可先抽成回傳 `impl IntoElement` 的 helper。

## 固定依賴與重現

workspace 鎖定 GPUI-CE git revision；CI 與交付指令應使用提交的 `Cargo.lock`。文件使用自己的 Node lockfile，VitePress build 也要在 CI 以 frozen/locked 模式執行。

## 目前缺口

尚無 gpui-vue 專用 hot reload、macro preview、視覺 snapshot harness 或 IDE template language server。不要以 markdown 截圖代替 native 行為測試；核心 docs gallery 範例應由 Cargo 真正編譯，畫面描述才可視為契約。測試分層詳見[測試](./testing.md)。
