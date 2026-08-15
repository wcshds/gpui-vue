# 正式部署

原生應用的發布物不是一組靜態網站檔案，而是帶平台 backend、資產與授權責任的可執行程式。能在開發機開窗不代表它已可重現地交付。

## 發布前的固定 gates

```bash
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets --no-default-features
cargo clippy --locked --workspace --all-targets --no-default-features -- -D warnings
cargo check --locked --workspace --all-targets --no-default-features --features desktop
cargo clippy --locked --workspace --all-targets --no-default-features --features desktop -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps --no-default-features
pnpm install --frozen-lockfile
pnpm docs:check
```

::: tip 執行結果
通過後可確認格式、headless 行為、desktop feature、lint、rustdoc 與文件連結都和 lockfiles 對齊。這不是簽章或 notarization，但能排除大部分來源與文件漂移。
:::

## 應用啟動配置

用 `DesktopApp` 集中安裝 assets、HTTP client、plugins、quit mode 與首個 `WindowConfig`。不要在 render 中讀取環境或重複註冊全域服務。正式 build 應選擇明確的 `WindowBackgroundAppearance`、最小尺寸和平台 app ID。

## 平台交付

- macOS：確認 bundle identifier、圖示、簽章、entitlements 與 notarization。
- Windows：建立一致的 application identity、圖示與 installer/signing 流程。
- Linux：確認 GPUI backend 所需的動態函式庫、desktop entry 與圖示安裝。

這些步驟目前不由 `gpui-vue` CLI 自動產生。不同平台必須在實際 OS 上做啟動、IME、clipboard、縮放與關閉行為 smoke test。

## 資產與授權

`EmbeddedAssets` 適合小型、版本固定的資產；大型或可更新內容可由應用定義外部儲存。發布前盤點字型、圖示、git dependency 與遠端資料來源的授權。效能調整見[效能](./performance.md)，輸入邊界見[安全](./security.md)。
