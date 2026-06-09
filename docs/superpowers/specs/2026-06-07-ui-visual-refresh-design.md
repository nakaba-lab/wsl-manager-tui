# wslm ビジュアルリフレッシュ — 設計ドキュメント

- 日付: 2026-06-07
- ステータス: ドラフト（ユーザーレビュー待ち）
- 対象プラットフォーム: Windows 10/11（WSL2 前提 / Windows Terminal 起動）

## 1. 概要 / 目的

`wslm` の TUI を「洗練ミニマル」方向で全面的にビジュアルリフレッシュする。
機能・MVU アーキテクチャ・ロケール非依存のステート判定はそのまま維持し、
**見た目の質感・情報レイアウト・操作性**のみを引き上げる。

ねらい:

- 色・枠・記号を整理し、状態が一目で読み取れる落ち着いたモダンな画面にする。
- メモリ / CPU をインラインゲージで可視化し、数値の羅列をやめる。
- 文脈連動フッターで、選択中ディストロに対して「今できる操作」を提示する。

非目標: 機能追加（新しい WSL 操作）・データ取得ロジックの変更・MVU 境界の変更。

## 2. 前提と制約

- **端末能力**: Truecolor（24bit）は使用してよい。ただし記号はどこでも描画できる
  標準 Unicode（`● ○ ◐ ★ ▶ ▌ █ ░ ▕ ▏ ▁..▇` 等）に限定する。**Nerd Font 非依存**。
- **MVU 純粋性**: `update`（reducer）は純粋関数のまま。本リフレッシュは原則 `ui` 層
  （`src/ui/`）に閉じる。`app`/`wsl`/`metrics` 等のロジック層は変更しない。
  例外として、フッターのキー候補導出に必要な `Model` の読み取り専用参照のみ使う。
- **crossterm 非依存（app 層）**: 既存方針どおり、`app` 層は crossterm を知らない。
  色・枠は `ui` 層（ratatui）でのみ扱う。
- **i18n**: ユーザー向け文字列はすべて `Key` 経由（ハードコード禁止）。`Key::ALL` と
  en/ja 両方を必ず埋め、`every_key_has_both_languages` を満たす。
- **CJK 幅**: 既存の `unicode-width` ベースの表示桁計算（`truncate_width` 等）を踏襲。
  新規のゲージ/整列も表示桁（バイト数/文字数ではない）で計算する。

## 3. カラーシステム（新規 `src/ui/theme.rs`）

パレットと色決定ロジックを 1 箇所に集約する。`ui` 層内のみで参照。

| 用途 | 色 |
|---|---|
| アクセント（タイトル / フォーカス枠 / 選択バー / フッター強調） | Teal/Cyan |
| State: Running | 緑 |
| State: Stopped | dim グレー |
| State: Installing | アンバー（黄系） |
| State: Unknown | 赤 |
| ゲージ: 通常 / 警告 ≥75% / 危険 ≥90% | ティール / アンバー / 赤 |
| スパークライン | シアン |
| 二次情報（ラベル・パス・テーブルヘッダ） | dim グレー |
| 既定 ★ | アンバー |

設計原則: **セマンティック色（緑/黄/赤）とアクセント色（ティール）を分離**する。
「意味の緑」と「装飾の緑」を混同させない。

公開 API（案、`ui` 内 `pub(crate)`/`pub(super)`）:

```rust
pub(super) const ACCENT: Color;          // ティール
pub(super) fn state_color(state: DistroState) -> Color;
pub(super) fn gauge_color(ratio: f64) -> Color;   // 0.0..=1.0 → teal/amber/red
pub(super) const DIM: Color;             // 二次情報
pub(super) const STAR: Color;            // 既定★ = アンバー
```

色は `Color` 定数として定義。テスト容易性のため、しきい値（0.75 / 0.90）は関数内に
明示し、境界値テストを書く。

## 4. レイアウト

トップレベルの縦分割を 3 → 4 に変更する（`src/ui/mod.rs::view`）:

```
table   Constraint::Min(5)
detail  Constraint::Length(10)
footer  Constraint::Length(1)   // 文脈連動キーヒント
status  Constraint::Length(1)   // ステータス/エラー/フィルタ/グローバルヒント
```

- 詳細ペイン内部は既存同様、内部高さに応じて末尾行（Trend → CPU ゲージ → …）を
  順に省略し、狭い端末でもパニックしない（既存 `two_trends` パターンを一般化）。
- フッター行とステータス行は、極端に狭い高さ（合計が確保できない場合）でも
  `Layout` が clamp するため安全。最低限テーブルが残る。

## 5. コンポーネント別の変更（`src/ui/mod.rs`）

### 5.1 枠（Block）
- 全パネルを `BorderType::Rounded` に統一。
- フォーカス側（テーブル）はティール枠＋ティールのタイトル。
- 詳細ペインは dim 枠＋ティールのタイトル。
- 共通生成ヘルパ `panel(title, focused: bool) -> Block` を `ui` 内に追加。

### 5.2 テーブル（`render_table` / `distro_row`）
- `Cell::from(String)` を、スタイル付き `Span`/`Line` に置き換える。
  - State 列: ドット（`● ○ ◐ ?`）を `state_color` で着色し、ラベルは通常色。
  - 二次列（VER / DISK / DEFAULT）: dim。DEFAULT の `★` はアンバー。
- ヘッダ: dim + 太字。
- 選択行: 反転（REVERSED）をやめ、`highlight_symbol("▌ ")` ＋
  `row_highlight_style`＝太字（必要なら淡い背景）。バーはアクセント感を出す。
  ※ ratatui の制約上、シンボルは行ハイライトスタイルを継承する。最終的な見栄えは
  実装時に微調整可（バー＋太字＋淡背景の範囲で）。

### 5.3 詳細ペイン（`render_detail`）
- 単一 `Paragraph::new(String)` を `Vec<Line>`（スパン構成）に変更し、要素ごとに着色。
- ラベルは右揃え整列（`State / Version / Path / Memory / CPU / Trend` 等）。
- State 行: ドット着色。Default ★ アンバー。
- Memory / CPU: **インラインゲージ** `▕████░░░░▏` ＋ 数値（`gauge_spans` 使用、
  しきい値で色変化）。Memory は `used/total` と `%`、CPU は `%`。
- Trend 行: ラベル＋シアンのスパークライン（Mem/CPU を横並び。幅が足りなければ
  Mem のみ → さらに足りなければ省略）。
- Inner disk があれば従来どおり追加（任意でゲージ化も可、まずは数値＋ゲージ）。

### 5.4 ゲージ描画（`src/ui/util.rs`）
純粋関数を追加:

```rust
/// ratio(0.0..=1.0) を幅 width 桁の ▕███░░░▏ ゲージのスパン列にする。
/// 充填部は gauge_color(ratio)、空部は DIM。両端 ▕ ▏ を含む。
pub(super) fn gauge_spans(ratio: f64, width: u16) -> Vec<Span<'static>>;
```

- ratio は呼び出し側で 0.0..=1.0 にクランプ。`None`（不明）時はゲージを描かず `—`。
- 充填セル数は表示桁ベースで四捨五入。ユニットテスト対象。

## 6. 文脈連動フッター（新規 `src/ui/footer.rs`）

`Model` から表示キー候補を導出する純粋関数を追加（`ui` 層、状態を変更しない）:

```rust
/// 選択中ディストロの state と keybind_style からフッター行を組む。
pub(super) fn context_hints(model: &Model) -> Line<'static>;
```

表示内容（実在キーに準拠。`src/app/update/mod.rs` のバインドを参照）:

- 実行中（Running）: `⏎ shell · w tab · x stop · d default · e export · m import`
- 停止中（Stopped）: `⏎ shell · s start · d default · e export · u remove`
- Installing / 選択なし: 最小限（`r refresh · ? help · q quit` 等）。
- ステータス行（2 行目, `render_status` 拡張）に常時グローバル:
  `↑↓/jk move · / filter · ? help · q quit`
  （`keybind_style.arrows_enabled()/vim_enabled()` に応じて ↑↓ / jk を出し分け）
- キー文字（`⏎ w x s d e m i u r / ?` 等）はアクセント、説明語は dim。
- 既存のステータス/エラー/フィルタ表示ロジック（黄/赤/緑/dim）はステータス行側に
  維持。フィルタ入力中（`filter_mode`）はフッター行を入力プロンプトに切替。

備考: `context_hints` は「キー記号＋ローカライズ動詞」を `tf`/`t` で組み立てる。

## 7. モーダル（`render_modal` 系）

- 全モーダルを `BorderType::Rounded` ＋ ティールのタイトルに統一。
- セマンティック枠色は維持: Confirm/Quit = 黄, Error = 赤, Progress = ティール。
- ピッカー（Install/Import）/フォーム/設定エディタの選択行も、テーブルと同じ
  `▌` 系ハイライトに合わせる。
- Help 本文（`Key::HelpBody`）に新規キー（`w` 新タブ, `m` 取込一覧, `i` 取込,
  `s` 起動, `d` 既定, `x` 停止, `X` 全停止, `u` 解除 等）を反映し、実バインドと一致させる。

## 8. i18n（`src/i18n/mod.rs`）

新規 `Key` を追加し、`Key::ALL` と en/ja の両方を埋める。想定キー（命名は実装時調整）:

- フッター動詞: `HintShell, HintTab, HintStart, HintStop, HintDefault, HintExport,
  HintImport, HintImportList, HintRemove, HintMove, HintFilter, HintHelp, HintQuit,
  HintRefresh`
- ラベル: `DetailTrend`（"Trend" / "推移"）等、必要に応じて追加。
- 既存の `HelpBody` を新キー反映で更新。

`every_key_has_both_languages` と `Key::ALL` 同期を維持する。

## 9. テスト方針

`TestBackend` のバッファはセル単位で**スタイル（fg 色）も保持**するため、記号だけでなく
色も検証できる。`ui::tests` のヘルパを拡張し、`(symbol, fg)` を取れるようにする。

新規テスト:

- テーブル: Running ドットが緑 / Stopped が dim / Installing がアンバー。
- 選択行: アクセントの `▌` が描画される。
- `gauge_spans`: 充填比率（0%/50%/100%）と、しきい値色（74%→ティール, 76%→アンバー,
  91%→赤）の境界。
- `context_hints`: 停止中は "start" を含み "stop" を含まない / 実行中はその逆 /
  `keybind_style` で ↑↓ と jk が切り替わる。

既存テストの更新:

- レイアウト/文言変更で壊れるアサーション（例 `"Detail: Debian"`、`matches("Mem")`
  カウント等）を新レイアウトに合わせて更新。意味（「メモリ行とトレンド行が両方出る」等）は
  保ったまま、検査文字列を新仕様に直す。

`cargo fmt --all -- --check` / `cargo clippy --all-targets -- -D warnings` /
`cargo test --all` をすべて緑にする。

## 10. スコープ外（YAGNI）

- Nerd Font アイコン（ディストロロゴ等）/ powerline 区切り。
- テーマ切替・カスタムパレットの設定項目（固定ティールテーマ 1 本）。
- マウス操作、新規アニメーション（既存スピナー以外）。
- 機能追加・データ取得ロジック・MVU 境界の変更。

## 11. 影響ファイル一覧

- 新規: `src/ui/theme.rs`, `src/ui/footer.rs`
- 変更: `src/ui/mod.rs`（view 分割・各 render 関数のスパン化・モーダル枠統一・テスト更新）、
  `src/ui/util.rs`（`gauge_spans` 追加）、`src/i18n/mod.rs`（新 `Key` + HelpBody 更新）
- 不変: `src/app/**`（reducer・Model）、`src/wsl/**`、`src/metrics/**`、`src/runtime/**`
