# Mnemark

**[English](README.md) | [繁體中文](README.zh-TW.md)**

<p align="center">
  <img src="mnemark_icon.svg" alt="Mnemark" width="128" />
</p>

<p align="center">
  <strong>Find anything you've copied.</strong>（找到任何你複製過的內容。）<br />
  <sub>發音 /ˈniː.mɑːrk/ — “NEE-mark”。名稱結合 <em>mneme</em>（記憶）與 <em>mark</em>（標記）。</sub>
</p>

你複製的每一件事都會留下痕跡——Mnemark 讓這些痕跡可被搜尋，讓你能隨時找回曾複製過的內容。

Mnemark 是使用 Tauri 2 開發的 Windows 10／11 剪貼簿管理工具。按下全域快捷鍵即可開啟精簡的浮動面板，搜尋近期內容，再貼回原本使用的應用程式。

專案提供支援背景更新的 NSIS 安裝版，以及免安裝、不寫入登錄檔的可攜版執行檔。

## 功能

- 記錄文字、圖片與已複製的檔案路徑
- 即時搜尋 Clip 內容與來源應用程式
- 鍵盤操作，並可選用 `j`／`k` Vim 瀏覽方式
- 最多釘選 10 則 Clip，使其不受容量限制自動淘汰
- 貼回原本聚焦的應用程式，或只複製而不關閉面板
- 將檔案歷史貼成實際檔案（`CF_HDROP`）或路徑文字
- 刪除後可在 3 秒內復原
- 自訂歷史容量、快捷鍵、主題、語言與擷取防抖動時間
- 排除指定應用程式所複製的剪貼簿內容
- 從系統匣暫停監聽
- 選擇是否以 SQLite 保存歷史
- 全介面支援繁體中文與英文

## 系統需求

- 64 位元 Windows 10 或 Windows 11
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)

Windows 11 與多數已更新的 Windows 10 已包含 WebView2。精簡版或 LTSC 系統可能需要另行安裝 Evergreen 版本。

## 下載

請從 [GitHub Releases](https://github.com/LiuTouo/Mnemark/releases/latest) 下載最新版：

| 版本 | 適合情境 | 更新方式 | 資料位置 |
| --- | --- | --- | --- |
| NSIS 安裝版（`*-setup.exe`） | 需要一般安裝流程 | 啟用自動更新後，在背景下載並安裝經簽章的更新 | `%APPDATA%\Mnemark` |
| 可攜版（`*-portable.exe`） | 需要獨立執行檔 | 從「關於」視窗檢查並下載經簽章的新執行檔，再由使用者手動取代執行中的版本 | 執行檔旁 |

可攜版不需要安裝，也不使用登錄檔 `Run` 機碼。選用的開機自啟功能會在 `shell:startup` 建立捷徑。

## 快速開始

1. 下載並執行任一版本。
2. Mnemark 啟動後常駐系統匣，不會開啟主視窗。
3. 照常複製文字、圖片或檔案。
4. 按 `Ctrl+Shift+V` 開啟剪貼簿歷史。
5. 選取 Clip，將其貼回原本使用的應用程式。

全域快捷鍵可在設定中變更。如果選定的快捷鍵已被其他應用程式占用，Mnemark 會開啟設定並要求改用其他組合。

## 使用方式

| 操作 | 結果 |
| --- | --- |
| `Ctrl+Shift+V` | 開啟或關閉歷史面板 |
| 方向鍵或 `j`／`k` | 在 Clip 間移動（`j`／`k` 需先啟用 Vim 模式） |
| `Enter` 或點擊 Clip | 貼上選取內容並關閉面板 |
| `Esc` 或點擊面板外 | 關閉面板 |
| 釘選 | 將 Clip 保留在頂部，避免被自動淘汰 |
| 複製 | 將 Clip 放入剪貼簿，但不關閉面板 |
| 刪除 | 移除 Clip，並顯示 3 秒復原操作 |
| 系統匣選單 | 暫停監聽、開啟設定或關於視窗，或結束程式 |

搜尋會比對 Clip 預覽、來源應用程式名稱與來源視窗標題，且不區分大小寫。

檔案歷史預設會寫入 Windows 實際檔案拖放格式，貼上行為等同在檔案總管複製檔案。原始檔案必須仍然存在；也可在設定中改為貼上路徑文字。

## 資料與隱私

- 剪貼簿歷史預設只保留於記憶體，Mnemark 結束後即消失。
- 啟用持久化後，歷史會寫入 `mnemark.db`；停用時會刪除該資料庫。
- 可攜版的設定與資料位於執行檔旁；安裝版使用 `%APPDATA%\Mnemark`。
- 預設排除清單包含 `1Password.exe`、`Bitwarden.exe` 與 `KeePass.exe`。其中任一應用程式位於前景時所複製的內容會被捨棄。
- 暫停監聽期間複製的內容會被捨棄，恢復監聽後不會補抓。
- 文字與圖片歷史容量可調整。超過限制時，會優先移除最舊且未釘選的 Clip。

## 已知限制

- 未提權的 Mnemark 無法將模擬貼上輸入送至以系統管理員身分執行的應用程式，因為 Windows UIPI 會加以阻擋。選取內容仍會留在剪貼簿，可手動按 `Ctrl+V` 貼上。
- 應用程式排除功能依複製當下的前景程式判斷。若密碼管理程式本身不在前景，便無法辨識其自動填入內容。
- 檔案歷史只保存路徑參照，不會複製檔案內容。如果來源檔案已移動或刪除，便無法貼成實際檔案。

## 從原始碼建置

安裝 [Node.js](https://nodejs.org/)、[Rust](https://rustup.rs/) 與 Tauri 2 所需的 Windows 前置元件，然後執行：

```powershell
git clone https://github.com/LiuTouo/Mnemark.git
cd Mnemark
npm ci
npm run build:app
```

正式版執行檔會產生於 `src-tauri/target/release/mnemark.exe`。正式建置請使用 `npm run build:app`，因為此指令會啟用 Tauri 必要的 `custom-protocol` feature。

若要以熱更新模式開發：

```powershell
npm run tauri dev
```

常用驗證指令：

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

## 文件

- [更新日誌](CHANGELOG.md)
- [行為與領域參考](CONTEXT.md)
- [架構決策紀錄](docs/adr/README.md)

## 授權

Mnemark 採用 [GNU General Public License v3.0](LICENSE) 授權。
