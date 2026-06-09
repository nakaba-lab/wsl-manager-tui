# WSL Manager TUI — 設計ドキュメント

- 日付: 2026-06-06
- ステータス: ドラフト（ユーザーレビュー待ち）
- 対象プラットフォーム: Windows 10/11（WSL2 前提）

## 1. 概要 / 目的

`wsl.exe` をコマンドラインで叩く代わりに、登録済み WSL ディストリビューションの
状態確認・操作・リソース監視・設定編集を **1 画面の TUI** で完結させる
ターミナルアプリケーションを開発する。単一バイナリ（`.exe`）として配布できる
品質を目指す。

ターゲットユーザー: 複数の WSL ディストロを日常的に扱う Windows 開発者。

## 2. スコープ

### 2.1 含む（v1 / フル機能）

- ディストロ一覧表示（名前 / 状態 / WSL バージョン / デフォルト印 / ディスク使用量）
- 起動 / 停止 / Terminate / デフォルト設定
- シェル起動（インライン復帰型 + Windows Terminal 新規タブ型の両対応）
- Unregister（登録解除＝削除、確認モーダル必須）
- Export（バックアップ）/ Import（復元・新規登録）
- 新規ディストロのインストール（`wsl --list --online` から選択して `--install`）
- リソース監視: VM メモリ（`vmmemWSL`）・ディスク（`ext4.vhdx` サイズ）・
  ディストロ内 `free`/`df` 値を、自動ポーリング + スパークラインで表示
- 設定ファイル編集: `.wslconfig`（Windows 側）と `wsl.conf`（ディストロ内 `/etc/wsl.conf`）を
  既知キーのフォーム編集 + 生テキスト編集の併用で編集
- UI 言語切替（英語 / 日本語）
- ヘルプオーバーレイ、フィルタ（インクリメンタル検索）

### 2.2 含まない（将来拡張）

- ディストロ複製（clone）※「フル」案では当初候補だったが v1 では Export→Import で代替し、
  ワンアクション clone は将来拡張に回す
- winget / Microsoft Store パッケージ配布（v1 は GitHub Release のみ）
- WSL1 ⇄ WSL2 の相互変換 UI（`--set-version`）※将来拡張
- リモート/SSH 経由の管理

## 3. 技術選定

| 項目 | 採用 | 理由 |
|---|---|---|
| 言語 | Rust（2021 edition 以降） | 単一バイナリ・型安全・配布容易 |
| TUI | `ratatui` + `crossterm` | デファクト。Windows ターミナル対応良好 |
| 非同期 | `tokio`（multi-thread, process, time, sync） | 遅い wsl コマンドで UI を固めない |
| シリアライズ | `serde` + `toml` | アプリ設定の永続化 |
| 設定パース | `wsl.conf`/`.wslconfig` は INI 風。`rust-ini` 相当 or 自前パーサ | 既知キー＋生編集の両立 |
| システム情報 | `sysinfo`（プロセスメモリ取得） | `vmmemWSL` の RSS 取得 |
| Windows API | `windows` crate（レジストリ）or `winreg` | `Lxss` レジストリ読み取り |
| エラー | `thiserror`（ライブラリ層）+ `anyhow`（アプリ層） | 型付きエラー＋伝播の簡潔さ |

> 注: `wsl.exe` の標準出力は **UTF-16LE**（BOM 付き）で返る。パーサは UTF-16 デコードと
> ロケール差異（状態文字列のローカライズ）に耐える実装にする。状態判定はできる限り
> `--list --verbose` の列位置や `--list --quiet` と組み合わせ、文字列マッチ依存を減らす。

## 4. アーキテクチャ

### 4.1 全体方針: MVU（Model–View–Update / Elm 構造）+ 非同期

一方向データフロー:

```
            ┌─────────────┐
   入力 ───▶│   Message   │
 (key/tick/ │   キュー     │
  task結果) └──────┬──────┘
                   ▼
            ┌─────────────┐      Command（副作用要求）
   Model ─▶│   Update     │──────────────┐
            │ (純粋関数)    │              ▼
            └──────┬──────┘        ┌──────────────┐
                   │ new Model      │ async タスク  │
                   ▼                │ (wsl/metrics)│
            ┌─────────────┐         └──────┬───────┘
            │    View      │                │ 完了
            │  (ratatui)   │                ▼
            └─────────────┘          Message として再投入
```

- **Model**: アプリ全状態（ディストロ一覧、選択位置、現在の画面/モーダル、メトリクス履歴、
  進行中オペレーション、言語、prefs）。
- **Update**: `(Model, Message) -> (Model, Vec<Command>)` の純粋関数。端末・IO に触れない＝
  ヘッドレスにユニットテスト可能。
- **Command**: 副作用の要求（例: `StartDistro(name)`, `Export{name, path}`, `RefreshList`）。
  ランタイムが async タスクとして実行し、結果を `Message` で返す。
- **View**: Model を ratatui ウィジェットに描画する純粋関数（状態を変更しない）。

### 4.2 モジュール構成

```
src/
  main.rs          エントリ：CLI引数、端末セットアップ、ランタイム起動、後始末
  runtime/         イベントループ（端末イベント + tick + タスク結果チャネルの合流）
                   - インラインシェル用のサスペンド/復帰（alt screen 退避→wsl実行→復元）
                   - 端末状態はRAIIガードで異常時も必ず復元
  wsl/             wsl.exe ラッパ。trait `WslBackend` で抽象化（テストでモック差し替え）
                   - list_distros / start / stop(terminate) / set_default
                   - unregister / export / import / install / list_online
                   - get_version。UTF-16デコード + 出力パーサ
  registry/        HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss を読み
                   各ディストロの GUID / BasePath / DefaultUid / Flags / vhdxパス取得
  metrics/         vmmemWSL メモリ(sysinfo)・vhdxサイズ(ファイルサイズ)・
                   distro内 df/free（wsl -d <name> -- 経由）。履歴リングバッファ
  config/          .wslconfig と wsl.conf の読み書き（既知キースキーマ + 生テキスト）
                   - パス解決（.wslconfig=%USERPROFILE%, wsl.conf=distro内 /etc/wsl.conf）
  app/             Model 定義 + Update（Messageリデューサ、純粋関数）+ Command 定義
  ui/              View 層。サブモジュール:
                   - table（一覧）/ detail（詳細・スパークライン）/ statusbar
                   - modal（確認 / フォーム / 進捗 / インストール選択 / エラー）
                   - help（キーバインド一覧）
  i18n/            英日メッセージカタログ（key→文字列、言語切替）
  prefs/           アプリ設定の永続化（%APPDATA%\wsl-manager-tui\config.toml）
                   - polling間隔 / 言語 / キーバインド流儀 / シェル起動の既定
  error.rs         エラー型（thiserror）
```

各モジュールの契約（何をするか / どう使うか / 何に依存するか）を doc コメントで明示する。
`wsl/`・`registry/`・`metrics/`・`config/` は UI 非依存（端末知識を持たない）。

### 4.3 データフロー（具体）

1. 起動 → `prefs` 読込 → 端末を alt screen / raw mode へ → 初回 `RefreshList` 発行 → 描画
2. tick（既定 2 秒、設定可）→ `RefreshList` + `RefreshMetrics` を async 実行 →
   `Message::Refreshed{distros, metrics}` → Update が Model 更新 → 再描画
3. キー入力 → `Message::Key` → Update（モーダル開閉、または `Command` 発行）→
   async 副作用 → 完了 `Message`（`OpDone` / `OpProgress` / `OpFailed`）→ Update → 再描画
4. 長時間操作（Export/Import/Install）→ 進捗モーダル表示。タスクは進捗を
   `mpsc` で push し `Message::OpProgress` に変換。キャンセルは可能な範囲で対応（プロセス kill）
5. インラインシェル（Enter）→ ランタイムが描画ループを一時停止 → alt screen を抜けて
   `wsl.exe -d <name>` を前面実行 → 終了で端末復元・ループ再開・一覧リフレッシュ

## 5. 機能仕様

### 5.1 一覧 / 詳細

- 一覧列: Name / State（● Running ○ Stopped）/ Version / Default(★) / Disk
- ソート: 既定はデフォルト→名前順。列ヘッダ上でのソート切替は将来拡張。
- 詳細ペイン: 状態・バージョン・デフォルト・BasePath・vhdxパス・Disk・
  VM メモリ（数値 + スパークライン）。VM メモリは全ディストロ共有である旨を併記。

### 5.2 ライフサイクル操作

| 操作 | キー | コマンド | 確認 |
|---|---|---|---|
| シェル起動(インライン) | Enter | `wsl -d <name>` 前面実行 | 無 |
| シェル起動(新規タブ) | Shift+Enter | `wt.exe -w 0 nt wsl -d <name>`（wt不在時はフォールバック通知） | 無 |
| Start | s | （明示起動。`wsl -d <name> -- true` で起動） | 無 |
| Stop（このディストロ） | x | `wsl --terminate <name>` | 任意（軽い確認） |
| Shutdown（全WSL VM停止） | X | `wsl --shutdown` | 要確認 |
| Default 設定 | d | `wsl --set-default <name>` | 無 |
| Unregister | u | `wsl --unregister <name>` | **要確認**（名前タイプ確認） |
| Export | e | `wsl --export <name> <path>` | パス選択 |
| Import | m | `wsl --import <name> <dir> <tar>` | 入力フォーム |
| Install | i | `--list --online` 選択 → `wsl --install -d <name>` | 進捗 |

> 補足: WSL2 では「Start」という独立コマンドは無い。`wsl --terminate` で停止、起動は
> 何らかのコマンド実行で行われるため、`s` は「軽量コマンドを投げて起動状態にする」挙動とする。

### 5.3 リソース監視

- VM メモリ: `sysinfo` で `vmmemWSL`（旧 `vmmem`）プロセスの RSS を取得。
- ディスク: 各ディストロの `ext4.vhdx`（registry の BasePath 配下）のファイルサイズ。
- ディストロ内: 起動中のみ `wsl -d <name> -- cat /proc/meminfo`・`df -h /` を取得（任意・失敗許容）。
- 履歴: 直近 N 点（既定 60）をリングバッファに保持しスパークライン描画。
- ポーリング間隔は prefs で変更可（既定 2 秒）。ポーリングは UI をブロックしない。

### 5.4 設定編集

- `.wslconfig`（`%USERPROFILE%\.wslconfig`）: `[wsl2]` セクションの memory / processors / swap /
  swapFile / localhostForwarding / networkingMode などをフォーム表示。未知キーは生編集タブで保持。
- `wsl.conf`（distro 内 `/etc/wsl.conf`）: `[boot]` `[automount]` `[network]` `[interop]` `[user]` 等。
  読み書きは `wsl -d <name> -u root -- ...`（書込は権限が要るためルート実行）で行う。
- 保存前にバックアップ（`.bak`）を作成。パースは「既知キー→フォーム」「全文→生編集」両ビュー。
- 変更反映には WSL シャットダウン（`wsl --shutdown`）が必要な旨を保存時に案内。

### 5.5 国際化（i18n）

- メッセージは `key -> { en, ja }` のカタログで一元管理。
- 起動時の既定言語は prefs（無ければ OS ロケールから推定、最終フォールバック en）。
- 実行中に `L` キーでトグル。表示幅は CJK 全角を考慮（`unicode-width` でカラム計算）。

## 6. UX / 画面仕様

### 6.1 メイン画面（モックアップ）

```
┌ WSL Manager ─────────────────────────────────── v0.1.0 · EN ┐
│  NAME           STATE      VER  DEF   DISK                    │
│ ▶Debian         ● Running   2    ★    4.2 GB                  │
│  Ubuntu-24.04   ○ Stopped   2         8.1 GB                  │
│  kali-linux     ○ Stopped   2         2.0 GB                  │
│                                                               │
├─ Detail: Debian ─────────────────────────────────────────────┤
│ State   : Running        Default : yes                        │
│ Version : 2              Path    : C:\Users\…\Debian          │
│ Disk    : 4.2 GB (ext4.vhdx)                                  │
│ VM Mem  : 1.8 / 8.0 GB   ▁▂▃▅▇▆▄▃   (vmmemWSL, 全VM共有)       │
├───────────────────────────────────────────────────────────────┤
│ Enter Shell · s Start · x Stop · X Shutdown · d Default       │
│ i Install · e Export · m Import · u Unregister · c Config     │
│ / Filter · ? Help · L 言語 · q Quit                           │
└───────────────────────────────────────────────────────────────┘
```

### 6.2 モーダル種別

- 確認モーダル（Terminate / Unregister / Import 上書き）: 破壊的操作は名前タイプ確認も検討。
- フォームモーダル（Import / Config フォーム編集）: フィールド単位の入力・検証。
- 進捗モーダル（Export / Import / Install）: 進捗テキスト + スピナー + キャンセル。
- インストール選択モーダル（`--list --online` の一覧から選択）。
- エラーモーダル / ステータストースト（操作結果・wsl エラーの提示）。

### 6.3 キーバインド

- 移動: 矢印キー + vim 風（j/k）を両対応（prefs で流儀選択も可）。
- 文字キーは上表の通り。`Esc` でモーダル閉じ / フィルタ解除。`q` で終了確認。
- ヘルプ `?` で全キー一覧オーバーレイ。

## 7. エラー処理

- `wsl.exe` の非ゼロ終了コード・stderr を捕捉し、エラーモーダル/ステータスで提示。UI は panic させない。
- UTF-16 デコード失敗やパース不能は「不明状態」として安全側で扱い、ログに残す。
- インラインシェルや外部プロセス実行は、異常時も端末状態を RAII ガードで必ず復元。
- 破壊的操作はすべて確認を挟み、誤操作を防ぐ。

## 8. テスト戦略

- **Update（純粋関数）**: Message 列を流して Model 遷移を検証（端末不要・高速）。中核ロジックの主戦場。
- **wsl パーサ**: 実際の UTF-16LE 出力をフィクスチャ化し、`list --verbose` 等のパースをユニットテスト。
- **config**: `.wslconfig`/`wsl.conf` の読み→編集→書きの round-trip テスト（未知キー保持・コメント保持方針を検証）。
- **WslBackend モック**: trait を差し替え、Update + Command 発行の結合を検証。
- **i18n**: 全 key が en/ja 双方に存在するかの網羅テスト。
- **統合スモーク**: 実 `wsl.exe` を叩くテストは `#[ignore]` or feature フラグで任意実行（CI 既定では走らせない）。
- ターゲット: 中核（wsl/config/app）はカバレッジ高め、ui は描画スナップショット中心。

## 9. 配布 / CI

- ビルド: `cargo build --release` → `wsl-manager-tui.exe` 単一バイナリ。
- CI（GitHub Actions, windows-latest）: fmt / clippy / test を PR で実行。
- リリース: タグ push（`v*`）で release ビルド → バイナリを GitHub Release に添付。
- 同梱: `README.md`（英語、スクリーンショット/キーバインド表）、`LICENSE`（MIT）。
- winget マニフェストは将来検討。

## 10. 非機能要件

- 起動 < 500ms（初回一覧取得は非同期、画面は即表示）。
- ポーリングによる体感ラグ無し（UI スレッドをブロックしない）。
- ターミナルリサイズに追従。CJK 幅を正しく計算。
- 端末は終了/異常時に必ず原状復帰（カーソル表示・raw 解除・alt screen 退出）。

## 11. 想定リスク / 留意点

- `wsl.exe` 出力フォーマットは将来変わり得る → パーサは堅牢に、状態は安全側にフォールバック。
- ローカライズされた状態文字列（"実行中" 等）依存を避ける（列位置/quiet 併用）。
- `wsl.conf` 書込はルート権限が必要 → `-u root` 実行と失敗時の明確なメッセージ。
- VM メモリはディストロ単位に厳密分解不可（共有 VM）→ UI で明記。
- Windows Terminal 不在環境での新規タブ起動フォールバック。

## 12. マイルストーン（実装計画の素案 — 詳細は writing-plans で）

1. 骨組み: ratatui + tokio のイベントループ、空 Model/Update/View、終了処理
2. wsl 層 + registry 層 + 一覧表示（読み取り専用）
3. ライフサイクル操作（start/stop/terminate/default）+ 確認モーダル
4. インライン/新規タブのシェル起動
5. metrics + スパークライン + 自動ポーリング
6. Export / Import / Install（進捗モーダル）
7. config 編集（フォーム + 生編集）
8. i18n（en/ja 切替）+ prefs 永続化
9. 仕上げ: エラー処理網羅、ヘルプ、フィルタ、README、CI/Release
