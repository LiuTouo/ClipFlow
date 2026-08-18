# 更新日誌

本專案所有重要變更都記錄於此檔案。

格式參考 [Keep a Changelog](https://keepachangelog.com/zh-TW/1.1.0/)，版本號遵循[語意化版本](https://semver.org/lang/zh-TW/)。

## [0.6.4] - 2026-08-18

### Fixed

- 修正重複使用的跨視窗拖曳預覽在重新顯示時可能落到其他應用程式後方；每次開始拖曳時重新套用原生最上層視窗狀態

## [0.6.3] - 2026-08-18

### Added

- 新增抽屜功能：可建立、重新命名、刪除與拖曳排序抽屜，將剪貼簿項目加入多個抽屜，並從抽屜切換回完整歷史紀錄
- 新增抽屜開關快捷鍵與設定介面，支援僅按修飾鍵的快捷方式，並於首次啟動教學說明完整操作流程
- 新增抽屜與歷史項目的跨視窗拖放提示：拖動時顯示獨立置頂預覽、可放置目標與重複項目阻擋狀態

### Changed

- 歷史面板可直接瀏覽抽屜內容，搜尋、分類、預覽、複製與貼上操作會套用至目前選取的資料集
- 重新設計抽屜的返回歷史介面與拖放動畫；按下項目拖曳把手後立即拿起，預覽視窗會依螢幕邊界與 DPI 調整位置
- 教學新增抽屜的開啟、建立、加入項目、排序、瀏覽與返回歷史操作語義

### Fixed

- 修正跨視窗拖曳預覽會被歷史面板或抽屜視窗邊界裁切的問題
- 修正以鍵盤選取項目後無法使用 Space 預覽，以及預覽狀態在焦點切換時不一致的問題

## [0.6.2] - 2026-08-18

### Added

- 安裝程式背景自動更新防護：當 Mnemark NSIS 安裝程式以 `/UPDATE` 模式執行且偵測到舊版 ClipFlow 解除安裝紀錄時，改為顯示重新下載的更名提示（內含下載網址，並詢問是否開啟下載頁面；選「是」以 ShellExec 開啟網址），無論選擇與否皆中止跨品牌自動更新、保留舊版 ClipFlow 安裝；不影響全新安裝、用於遷移 ClipFlow 的手動 Mnemark 安裝程式，以及未來的 Mnemark-to-Mnemark 自動更新

## [0.6.1] - 2026-08-18

### Fixed

- 修正安裝程式無法移除舊版 ClipFlow：v0.6.0 安裝程式從錯誤的製造商/產品登錄機碼讀取 `InstallLocation`，得到空值並產生空 `_?=` 的解除安裝指令，導致舊版 ClipFlow 未被移除。現改為從同一個舊版解除安裝登錄機碼（`Uninstall\ClipFlow`）同時讀取 `UninstallString` 與 `InstallLocation`（HKCU 優先、其次 HKLM，並追蹤所選 hive），正規化引號，於 `InstallLocation` 遺失時由 `UninstallString` 推導父目錄，絕不以空 `_?=` 執行；解除安裝改採 Tauri 原生被動模式（`/P` + `_?=<安裝目錄>`），分離處理啟動失敗與非零退出碼，且只複查所選 hive 避免誤判

## [0.6.0] - 2026-08-17

### Changed

- 品牌更名為 Mnemark：應用程式、文件、更新與發行資產全面由 ClipFlow 改名為 Mnemark（`mnemark.exe`、`mnemark.config.json`、`mnemark.db`、`mnemark-update.exe`、`Mnemark.lnk`），GitHub 儲存庫移至 `LiuTouo/Mnemark`
- 首次啟動時自動將舊版 ClipFlow 資料遷移至 Mnemark：設定檔、SQLite 資料庫與開機自啟捷徑以複製方式遷移，舊檔保留為可復原備份；NSIS 安裝程式會清理舊版 ClipFlow 安裝
- 面板預覽提示的 localStorage 鍵由 `clipflow.previewHintSeen.v1` 遷移至 `mnemark.previewHintSeen.v1`

### Added

- ADR 0003：記錄 Mnemark 品牌識別、資料遷移、衝突規則、安裝程式清理與舊名稱保留清單

## [0.5.7] - 2026-08-17

### Changed

- 面板內嵌 SVG 圖示改用 DOM API（`createElementNS`）以程式化方式產生，取代原本的 `innerHTML` 字串拼接

## [0.5.6] - 2026-08-17

### Added

- 前端測試執行器：新增 vitest 與 `npm test` 指令，並為預覽切換狀態機（PreviewController）加入單元測試
- CI workflow（`.github/workflows/ci.yml`）：推送或 PR 即執行前端測試與建置、Rust 格式化／clippy／cargo check／測試

### Changed

- 預覽互動改為「按下 Space 切換」：滑鼠懸停項目後按一下 Space 開啟預覽、再按一下關閉（原為按住開啟、放開關閉）；新增 PreviewController 純狀態機處理按鍵連發、焦點往返與後端狀態同步，面板暫時隱藏時保留預覽內容、再次開啟時還原
- 停用持久化時不再直接刪除資料庫：改為記錄上次清理時間並釋放連線，啟動時於超過 72 小時後交易式清理不在歷史中的殘留列
- 設定檔改為原子寫入（先寫暫存檔再重新命名覆蓋），損毀的設定檔保留為 `.bak` 後回退預設值
- Rust 程式碼統一 `cargo fmt` 格式

### Fixed

- 圖片記憶體預算設定（`image_memory_budget_mb`）過去未於淘汰時生效，僅以筆數上限淘汰；現一併強制執行記憶體預算
- 停用持久化時清理門檻（cleanup gate）寫入失敗會丟棄連線、造成狀態遺失；改為保留狀態

## [0.5.5] - 2026-08-16

### Fixed

- 安裝版更新失敗：更新時未先關閉執行中的 ClipFlow，導致安裝檔寫入被鎖住的執行檔而失敗、需手動關閉後重試。現於 NSIS 安裝／解除安裝前自動強制結束執行中的 clipflow.exe，再進行寫入

## [0.5.4] - 2026-08-16

### Added

- 歷史項目支援按住 Space 預覽完整內容：滑鼠懸停項目後按住 Space 開啟獨立預覽視窗，顯示文字、圖片或檔案的完整內容與中繼資料（來源、擷取時間、大小），截斷內容會標示僅保存開頭部分
- 搜尋列新增 `/` 快捷鍵提示，按下 `/` 即可聚焦搜尋框

### Fixed

- 預覽 Space 按鍵處理：修正按鍵連發、跨焦點保留 latch、以及預覽視窗中 Space 誤輸入文字等問題；搜尋框需明確取得焦點才接受輸入

## [0.5.3] - 2026-08-15

### Added

- 設定頁新增載入中、未儲存變更、儲存中、儲存失敗等狀態提示；儲存失敗保留已填表單內容並可重試
- 「儲存」按鈕改為 dirty + 驗證通過才啟用：表單未變更或欄位無效（空值、min/max、step 不符）時停用
- 快捷鍵欄位支援鍵盤操作：Enter／Space 開始錄製、Esc 取消；狀態訊息與熱鍵錯誤加入 aria-live 與 role="alert" 無障礙標記
- 新增 prefers-reduced-motion 支援，啟用「減少動態效果」時停用過渡動畫

### Changed

- 重新整理設定頁視覺與資訊層級：改以卡片式區塊分組（快捷鍵、文字歷史、圖片歷史、行為、外觀、排除清單），統一標題與輔助說明
- 動作列改為 sticky footer，窄視窗下保持可見且不遮擋內容
- 語言設定同步更新 `<html lang>` 屬性

## [0.5.2] - 2026-08-14

### Added

- 新增 UI 不透明度設定，範圍 50%-100%，預設 96%，100% 為完全不透明

## [0.5.1] - 2026-08-12

### Fixed

- 修復長剪貼簿歷史導致搜尋列與類型過濾列被壓縮重疊的問題

## [0.5.0] - 2026-08-12

### Added

- 歷史面板新增「全部、文字、圖片、檔案、連結」類型過濾器，可與搜尋條件交集使用，並支援鍵盤方向鍵操作
- 新增「記住歷史記錄過濾器」設定，可在應用程式執行期間保留上次選擇的類型
- 內嵌 IBM Plex Sans TC 字型，無需連線即可提供一致的中英文介面字型

### Changed

- 重新設計歷史面板與設定頁視覺，包括卡片式設定區塊、切換開關、操作圖示與刪除選單
- 歷史面板改為在滑鼠游標所在螢幕中央開啟，改善多螢幕使用體驗
- 刪除操作移至每筆記錄的更多選單，釘選與僅複製操作維持直接存取

## [0.4.9] - 2026-08-10

### Changed

- 所有 ClipFlow 應用程式與品牌圖示更新為透明背景設計

## [0.4.8] - 2026-08-10

### Fixed

- 面板每次開啟或重新取得焦點時，清除搜尋框、選取第一筆記錄、捲動至頂部；新增、刪除、釘選等面板開啟期間的操作仍保留目前捲動位置

## [0.4.7] - 2026-07-26

### Changed

- 移除手寫的 Win32 `extern "system"` 記憶體函數宣告，改用 windows crate 原生介面（GlobalAlloc / GlobalLock / GlobalFree 等），消除簽名錯誤導致未定義行為的殘留風險
- 圖片位元組查詢封裝進 HistoryStore 並檢查 Clip 類型：對非圖片項目請求圖片時回報明確錯誤，不再顯示誤導的「找不到 Clip」
- 面板設定載入失敗時現在會記錄錯誤至主控台（仍沿用預設語言 fallback）

## [0.4.6] - 2026-07-24

### Fixed

- 剪貼簿監聽執行緒加入 panic 防護：任一輪擷取異常（例如畸形圖片資料）只記錄並繼續監聽，共享狀態鎖也可自中毒復原，剪貼簿歷史不再靜默停止
- 面板開啟時收到新剪貼內容，列表不再跳回頂部，保留目前捲動位置
- 啟動時熱鍵被其他程式占用，自動開啟的設定視窗現在會顯示內聯錯誤說明原因
- 防抖動視窗內連續複製不同內容後，刻意重複複製相同內容可能被誤判為雙擊雜訊而丟棄；改以目前內容的首次觀察時間判定
- 關於頁「開啟資料夾」按鈕失效：shell open 的預設驗證不含本機路徑，現已明確放行 https 連結與本機絕對路徑
- 熱鍵錄製中點擊他處會把「請按下按鍵…」占位文字誤存為熱鍵；失焦現在會還原已存熱鍵，錄製結束後欄位保持唯讀

### Changed

- 後端錯誤訊息本地化：釘選上限、無可復原的刪除、無效熱鍵等提示不再以英文顯示
- IPC 不再複製原始圖片位元組：面板列表改傳中繼資料、來源查詢只取所需欄位，大幅降低記憶體拷貝
- 歷史寫入 SQLite 的整批 dump 改為單一交易，中途當機不再殘留半份歷史
- 設定檔數值加上下限夾取，手改組態檔不再能造成無上限的記憶體增長
- 啟動時自動清理殘留的 clipflow-update.exe 舊更新檔
- 對以系統管理員身分執行的應用程式貼上會被 Windows 阻擋（UIPI），失敗現在會記錄且內容保留在剪貼簿供手動貼上；README 新增已知限制說明

### Security

- 收緊 webview 權限：移除前端未使用的 global-shortcut 與 event emit 權限

## [0.4.5] - 2026-07-23

### Fixed

- 貼上不再用固定延遲猜測焦點：隱藏 Panel 後會等到焦點真的離開才模擬 Ctrl+V，焦點切換慢的程式不再漏貼
- 熱鍵開啟 Panel 後快速按 Enter 貼上時，會先釋放仍按住的 Shift/Alt 再還原，不再被目標程式誤判為 Ctrl+Shift+V（選擇性貼上）而看似貼上無效；模擬按鍵改用 SendInput
- 剪貼簿被其他程式短暫占用時，寫入會自動重試（5 次），不再一次失敗就放棄
- 焦點落在桌面時不再送出 Ctrl+V，避免檔案歷史被複製一份到桌面；內容保留在剪貼簿供手動貼上
- 貼上／複製失敗時改為顯示提示，不再靜默失敗

## [0.4.4] - 2026-07-23

### Fixed

- 歷史達到上限時，新複製的內容會被立刻淘汰（最舊的反而永久保留）；改為正確淘汰最舊的未釘選項目
- 複製超過單則文字大小上限的內容時，截斷點落在多位元組字元中間會使監聽執行緒崩潰、剪貼簿監聽靜默失效；改為依 UTF-8 字元邊界截斷
- 刻意重複複製相同內容不再被靜默丟棄：會更新時間並移到最頂（防抖動現在僅在時間窗內丟棄相同內容）
- 讀取剪貼簿文字與檔案清單時缺少邊界檢查，畸形資料可能造成越界讀取；一併拒絕 ANSI 檔案清單格式，圖片 DIB 解碼加入維度上限與溢位檢查
- 「復原刪除」改為依 Clip id 比對，過期的復原請求不再可能還原錯誤的項目

### Security

- 啟用 Content Security Policy，限制 webview 可載入的指令碼、樣式、圖片與連線來源
- 免安裝版更新下載前改以 minisign 簽章驗證（與安裝版 updater 同一把金鑰），驗證通過才寫入磁碟；下載來源限定 GitHub 網域、逐跳驗證重新導向，另加下載大小與逾時上限。CI 發行流程同步改為自動簽署免安裝執行檔

## [0.4.3] - 2026-07-22

### Changed

- 無功能變更；此版本僅用於驗證安裝版（NSIS）背景自動更新流程

## [0.4.2] - 2026-07-22

### Fixed

- 安裝版被誤判為免安裝版：管道偵測從 exe 路徑啟發式改為讀取 NSIS 解除安裝登錄檔（`Uninstall\ClipFlow` 的 `InstallLocation`），涵蓋 per-user 與 per-machine 安裝
- 免安裝版更新永遠失敗並轉開下載頁：GitHub 資產 CDN 不提供 CORS 標頭，webview fetch 無法跟隨重新導向，下載改由 Rust 端執行（ureq），並移除不再需要的 plugin-fs
- 安裝版設定與歷史無法保存：安裝目錄（如 Program Files）使用者無寫入權，設定檔與 SQLite 改存 `%APPDATA%\ClipFlow`（免安裝版仍在 exe 旁）

## [0.4.1] - 2026-07-22

### Changed

- 無功能變更；此版本僅用於驗證安裝版（NSIS）背景自動更新流程

## [0.4.0] - 2026-07-22

### Added

- 貼上檔案歷史時複製實際檔案（CF_HDROP，等同在檔案總管複製；貼上時來源檔案必須仍存在），可於設定關閉改貼路徑文字
- 安裝版（NSIS）背景自動更新：啟動時檢查、下載、驗證簽章、安裝，確認後重新啟動
- 免安裝版於「關於」頁檢查更新並自動下載新執行檔，使用者關閉程式後手動覆蓋
- 「關於」頁新增更新區塊（檢查／安裝／重新啟動／開啟資料夾），開啟時隨「自動檢查更新」設定自動檢查
- 設定新增「貼上檔案歷史時複製實際檔案」與「自動檢查更新」開關
- 系統匣提示顯示版本號（編譯期取自 Cargo.toml）
- GitHub Actions 發行流程：推送 v* tag 自動建置 NSIS 安裝檔、updater latest.json 與免安裝執行檔並上傳 Release
- `scripts/bump-version.ps1`（`npm run bump -- x.y.z`）：同步 Cargo.toml、package.json、package-lock.json 並插入更新日誌骨架

### Fixed

- package-lock.json 版本與 package.json 脫節（停留在 0.1.0）

## [0.2.1] - 2026-07-22

### Fixed

- 剪貼簿被其他應用程式佔用時複製內容永久遺失：捕捉成功才消費序號，佔用時下輪自動重試；各格式捕捉的失敗路徑一律關閉剪貼簿（修正長期佔用導致其他應用程式無法複製）
- 防抖動設定大於 200ms 時，快速連續複製的內容被永久捨棄，改為延遲至逾時後捕捉最新內容
- 正式版誤註冊全域 `Ctrl+Shift+I`，搶走瀏覽器／IDE 開發者工具快捷鍵，改為僅偵錯建置註冊
- 搜尋過濾後按 Enter 或方向鍵貼上錯誤項目（選擇索引對照未過濾陣列）
- 面板顯示後端已淘汰的幽靈項目（對其操作報「Clip not found」）：淘汰事件現在隨更新同步到面板
- 從面板複製或貼上後，該則的來源應用程式被覆寫成 ClipFlow 自己，現保留原始來源
- 圖片原圖不再以 JSON 陣列跨 IPC（10 MB 圖放大約三倍），改為貼上時由後端按 id 取出位元組
- Vim 模式下搜尋框打不出 j／k：改為 Esc 先進入導航模式，此時 j／k 才移動選擇，輸入任意字元自動回到搜尋
- 釘選後項目不會即時移到頂部；刪除或淘汰後選擇索引可能越界，收斂集中到渲染時
- vim 模式、主題設定變更需重開才生效，改為面板聚焦時即時重載；主題設定（跟隨系統／深色／淺色）此前儲存後從未套用，現所有頁面生效
- 熱鍵可註冊無修飾鍵單鍵（如 `A`）導致該鍵全系統失效，錄製介面與後端雙層拒絕；錄製另支援 Super（Windows）鍵，後端錯誤訊息本地化
- 搜尋無結果時畫面一片空白，新增「無符合的項目」提示
- 寫入剪貼簿失敗路徑洩漏記憶體（補 `GlobalFree`）；`CF_TEXT`（ANSI）後備以 UTF-16 誤讀產生亂碼，移除該後備
- 刪除與復原的鎖定順序相反（死結隱患），改為不嵌套持鎖
- 手編設定檔可將筆數上限設為 0 導致文字歷史立即全數淘汰，後端加入下限保護

### Changed

- 偵錯日誌在偵錯建置輸出至 stderr，正式建置為 no-op
- 移除未使用的指令（`paste_file_paths`、`pause/resume/is_monitoring`）與 `Clip` 未使用的 `Deserialize`
- 關於頁改用獨立 `about-body` 樣式類別（不再借用設定頁）

### Added

- 建立 `docs/adr/`（對應 CONTEXT.md 的 Decisions 引用）

## [0.2.0] - 2026-07-21

### Added

- 設定介面全面繁體中文化，新增語言下拉選單（繁體中文／English）；面板、關於頁、系統匣右鍵選單同步套用語言設定
- 可選 SQLite 持久化：設定勾選後歷史寫穿至 exe 旁 `clipflow.db`，重開保留；取消勾選時刪除資料庫
- 開機自啟選項：於 `shell:startup` 建立捷徑（不寫登錄檔）
- 圖片單張大小上限（`image_size_limit_mb`）強制執行：超過上限的圖片自動降解析度重新編碼為 24bpp DIB 後儲存
- 關於頁版本號改為動態讀取（版本單一來源為 `src-tauri/Cargo.toml`），並新增更新日誌連結
- 設定頁熱鍵變更即時生效免重啟；衝突時顯示行內錯誤並保留舊組合
- 系統匣暫停監聽選單文字隨狀態切換（暫停／繼續）

### Changed

- 歷史面板改為真透明圓角視窗：移除 DWM 預設白框，CSS 陰影完整呈現；不透明度提高改善可讀性
- 刪除吐司提示延長為 4 秒，文案本地化（已刪除／復原）
- 捲動條與設定頁表單控制項（下拉、核取框、數字輸入）去除原生網頁外觀
- 版本號移至 `Cargo.toml` 單一來源管理（`tauri.conf.json` 不再重複設定）

### Fixed

- 正式建置缺少 `custom-protocol` feature，導致所有視窗顯示「無法連線」（只有 dev server 存在時才能顯示）
- 關閉最後一個視窗會結束整支程式，改為退回系統匣背景常駐
- 圖片縮圖從未產生：BMP 檔頭寫入缺少像素偏移欄位，改寫為手動 DIB 解碼器＋正確的 BMP 包裝備案
- 來源應用程式永遠顯示 Unknown（取得 pid 後未讀取 exe 名稱）；排除清單自此真正生效
- 暫停監聽期間的複製會在恢復後被抓進歷史，改為暫停時同步序號、永久捨棄
- 貼上會貼到面板自己：改為先隱藏面板讓焦點回到原應用程式再模擬 Ctrl+V
- 面板建立瞬間的焦點抖動導致一開即關：失焦關閉改為首次聚焦後才啟用
- 重複複製已存在的 Clip 不會移到歷史頂部
- 移除 CF_BITMAP 的不安全讀取（HBITMAP 非記憶體區塊；作業系統會自動合成 CF_DIB）

## [0.1.0] - 2026-07-20

### Added

- 初始版本：剪貼簿監聽（文字／圖片／檔案路徑）、SHA-256 內容去重、容量限制與淘汰、釘選（上限 10 則、永不淘汰）、即時搜尋、Raycast 風格浮動面板（`Ctrl+Shift+V`）、貼上模擬、刪除復原、系統匣常駐、排除清單、深淺色主題跟隨系統、免安裝可攜（設定存於 exe 旁）

[0.6.4]: https://github.com/LiuTouo/Mnemark/compare/v0.6.3...v0.6.4
[0.6.3]: https://github.com/LiuTouo/Mnemark/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/LiuTouo/Mnemark/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/LiuTouo/Mnemark/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/LiuTouo/Mnemark/compare/v0.5.7...v0.6.0
[0.5.7]: https://github.com/LiuTouo/ClipFlow/compare/v0.5.6...v0.5.7
[0.5.6]: https://github.com/LiuTouo/ClipFlow/compare/v0.5.5...v0.5.6
[0.5.5]: https://github.com/LiuTouo/ClipFlow/compare/v0.5.4...v0.5.5
[0.5.4]: https://github.com/LiuTouo/ClipFlow/compare/v0.5.3...v0.5.4
[0.5.3]: https://github.com/LiuTouo/ClipFlow/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/LiuTouo/ClipFlow/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/LiuTouo/ClipFlow/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.9...v0.5.0
[0.4.9]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.8...v0.4.9
[0.4.8]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.7...v0.4.8
[0.4.7]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.6...v0.4.7
[0.4.6]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.5...v0.4.6
[0.4.5]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.4...v0.4.5
[0.4.4]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/LiuTouo/ClipFlow/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/LiuTouo/ClipFlow/compare/v0.2.1...v0.4.0
[0.2.1]: https://github.com/LiuTouo/ClipFlow/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/LiuTouo/ClipFlow/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/LiuTouo/ClipFlow/releases/tag/v0.1.0
