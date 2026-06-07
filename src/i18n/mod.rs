//! Internationalization: an English/Japanese message catalog keyed by enum,
//! with runtime language switching. Static strings are returned by reference;
//! [`tf`] does simple positional `{}` substitution for dynamic text.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// UI language.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// English.
    #[default]
    En,
    /// Japanese.
    Ja,
}

impl Lang {
    /// The other language.
    pub fn toggled(self) -> Lang {
        match self {
            Lang::En => Lang::Ja,
            Lang::Ja => Lang::En,
        }
    }

    /// A short label for the language indicator.
    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Ja => "JA",
        }
    }
}

/// A translatable message key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // Status line / general chrome.
    StatusHint,
    Loading,
    ErrorPrefix,
    NoDistros,
    FilterApplied,
    // Table columns.
    ColName,
    ColState,
    ColVer,
    ColDefault,
    ColDisk,
    // Detail pane.
    DetailTitle,
    DetailState,
    DetailVersion,
    DetailDefault,
    DetailDisk,
    DetailInnerDisk,
    DetailPath,
    DetailVmMem,
    DetailVmMemTrend,
    // Distro state.
    StateRunning,
    StateStopped,
    StateInstalling,
    StateUnknown,
    VmNotRunning,
    VmSharedNote,
    // Modal titles.
    ConfirmTitle,
    ErrorTitle,
    ProgressTitle,
    HelpTitle,
    QuitTitle,
    FormExportTitle,
    FormImportTitle,
    ConfigEditPrefix,
    ModeForm,
    ModeRaw,
    // Modal hints / bodies.
    ConfirmHintTyped,
    ConfirmHintYesNo,
    ConfirmTypedLine,
    ErrorDismiss,
    ProgressHint,
    InstallTitle,
    InstallHint,
    ConfigSaveHint,
    FormFooter,
    HelpBody,
    QuitPrompt,
    // Form field labels.
    LabelExportPath,
    LabelImportNameOnly,
    LabelImportCustomArchive,
    // Import picker / managed folder.
    PickImportTitle,
    PickImportEmpty,
    PickImportHints,
    ExportFormatHint,
    PromptDeleteArchive,
    DoneDeletedArchive,
    // Prompts.
    PromptTerminate,
    PromptShutdown,
    PromptUnregister,
    PromptImportOverwrite,
    // Transient status messages.
    StatusFetching,
    StatusLoading,
    StatusStarting,
    StatusSettingDefault,
    StatusLaunchingShell,
    StatusCancelling,
    StatusSaving,
    StatusReturnedFrom,
    // Progress titles.
    ProgExporting,
    ProgImporting,
    ProgInstalling,
    // Operation results.
    DoneStarted,
    DoneTerminated,
    DoneShutdown,
    DoneSetDefault,
    DoneUnregistered,
    DoneExported,
    DoneImported,
    DoneInstalled,
    DoneOpenedTab,
    DoneSavedConfig,
    FailOp,
    FailListOnline,
    FailLoadConfig,
    FailSaveConfig,
    WtNotFound,
    ShellBanner,
}

impl Key {
    /// All keys (used to verify catalog completeness in tests).
    pub const ALL: &'static [Key] = &[
        Key::StatusHint,
        Key::Loading,
        Key::ErrorPrefix,
        Key::NoDistros,
        Key::FilterApplied,
        Key::ColName,
        Key::ColState,
        Key::ColVer,
        Key::ColDefault,
        Key::ColDisk,
        Key::DetailTitle,
        Key::DetailState,
        Key::DetailVersion,
        Key::DetailDefault,
        Key::DetailDisk,
        Key::DetailInnerDisk,
        Key::DetailPath,
        Key::DetailVmMem,
        Key::DetailVmMemTrend,
        Key::StateRunning,
        Key::StateStopped,
        Key::StateInstalling,
        Key::StateUnknown,
        Key::VmNotRunning,
        Key::VmSharedNote,
        Key::ConfirmTitle,
        Key::ErrorTitle,
        Key::ProgressTitle,
        Key::HelpTitle,
        Key::QuitTitle,
        Key::FormExportTitle,
        Key::FormImportTitle,
        Key::ConfigEditPrefix,
        Key::ModeForm,
        Key::ModeRaw,
        Key::ConfirmHintTyped,
        Key::ConfirmHintYesNo,
        Key::ConfirmTypedLine,
        Key::ErrorDismiss,
        Key::ProgressHint,
        Key::InstallTitle,
        Key::InstallHint,
        Key::ConfigSaveHint,
        Key::FormFooter,
        Key::HelpBody,
        Key::QuitPrompt,
        Key::LabelExportPath,
        Key::LabelImportNameOnly,
        Key::LabelImportCustomArchive,
        Key::PickImportTitle,
        Key::PickImportEmpty,
        Key::PickImportHints,
        Key::ExportFormatHint,
        Key::PromptDeleteArchive,
        Key::DoneDeletedArchive,
        Key::PromptTerminate,
        Key::PromptShutdown,
        Key::PromptUnregister,
        Key::PromptImportOverwrite,
        Key::StatusFetching,
        Key::StatusLoading,
        Key::StatusStarting,
        Key::StatusSettingDefault,
        Key::StatusLaunchingShell,
        Key::StatusCancelling,
        Key::StatusSaving,
        Key::StatusReturnedFrom,
        Key::ProgExporting,
        Key::ProgImporting,
        Key::ProgInstalling,
        Key::DoneStarted,
        Key::DoneTerminated,
        Key::DoneShutdown,
        Key::DoneSetDefault,
        Key::DoneUnregistered,
        Key::DoneExported,
        Key::DoneImported,
        Key::DoneInstalled,
        Key::DoneOpenedTab,
        Key::DoneSavedConfig,
        Key::FailOp,
        Key::FailListOnline,
        Key::FailLoadConfig,
        Key::FailSaveConfig,
        Key::WtNotFound,
        Key::ShellBanner,
    ];
}

/// The (English, Japanese) pair for a key.
fn entry(key: Key) -> (&'static str, &'static str) {
    match key {
        Key::StatusHint => (
            "j/k move · Enter shell · s start · x stop · X shutdown · d default · u unreg · e export · m import · i install · c/C config · L lang · / filter · ? help · q/Esc quit",
            "j/k 移動 · Enter シェル · s 起動 · x 停止 · X 全停止 · d 既定 · u 登録解除 · e エクスポート · m インポート · i インストール · c/C 設定 · L 言語 · / フィルタ · ? ヘルプ · q/Esc 終了",
        ),
        Key::Loading => ("loading…", "読み込み中…"),
        Key::ErrorPrefix => ("error", "エラー"),
        Key::NoDistros => ("No distributions.", "ディストリビューションがありません。"),
        Key::FilterApplied => ("filter: {} · Esc clears", "フィルタ: {} · Esc で解除"),
        Key::ColName => ("NAME", "名前"),
        Key::ColState => ("STATE", "状態"),
        Key::ColVer => ("VER", "Ver"),
        Key::ColDefault => ("DEFAULT", "既定"),
        Key::ColDisk => ("DISK", "ディスク"),
        Key::DetailTitle => ("Detail", "詳細"),
        Key::DetailState => ("State", "状態"),
        Key::DetailVersion => ("Version", "バージョン"),
        Key::DetailDefault => ("Default", "既定"),
        Key::DetailDisk => ("Disk", "ディスク"),
        Key::DetailInnerDisk => ("In-distro", "内部ディスク"),
        Key::DetailPath => ("Path", "パス"),
        Key::DetailVmMem => ("VM Mem", "VMメモリ"),
        Key::DetailVmMemTrend => ("Trend", "推移"),
        Key::StateRunning => ("Running", "実行中"),
        Key::StateStopped => ("Stopped", "停止"),
        Key::StateInstalling => ("Installing", "インストール中"),
        Key::StateUnknown => ("Unknown", "不明"),
        Key::VmNotRunning => ("— (WSL VM not running)", "— (WSL VM 停止中)"),
        Key::VmSharedNote => (
            "(vmmemWSL, shared by all distros)",
            "(vmmemWSL, 全ディストロ共有)",
        ),
        Key::ConfirmTitle => (" Confirm ", " 確認 "),
        Key::ErrorTitle => (" Error ", " エラー "),
        Key::ProgressTitle => (" Working ", " 処理中 "),
        Key::HelpTitle => (" Help — keybindings ", " ヘルプ — キー操作 "),
        Key::QuitTitle => (" Quit ", " 終了 "),
        Key::FormExportTitle => (" Export distribution ", " ディストロをエクスポート "),
        Key::FormImportTitle => (" Import distribution ", " ディストロをインポート "),
        Key::ConfigEditPrefix => ("Edit", "編集"),
        Key::ModeForm => ("Form", "フォーム"),
        Key::ModeRaw => ("Raw", "生"),
        Key::ConfirmHintTyped => (
            "Enter: confirm (must match) · Esc: cancel",
            "Enter: 確定(一致必須) · Esc: キャンセル",
        ),
        Key::ConfirmHintYesNo => (
            "Enter / y: confirm · Esc / n: cancel",
            "Enter / y: 確定 · Esc / n: キャンセル",
        ),
        Key::ConfirmTypedLine => (
            "type \"{}\" to confirm: {}",
            "確認のため \"{}\" と入力: {}",
        ),
        Key::ErrorDismiss => ("Press any key to dismiss.", "任意のキーで閉じます。"),
        Key::ProgressHint => (
            "This may take a while. Esc to cancel.",
            "時間がかかる場合があります。Esc でキャンセル。",
        ),
        Key::InstallTitle => (
            " Install — select a distribution ",
            " インストール — ディストロを選択 ",
        ),
        Key::InstallHint => (
            "type to filter · ↑/↓ select · Enter install · Esc cancel",
            "入力で絞り込み · ↑/↓ 選択 · Enter インストール · Esc キャンセル",
        ),
        Key::ConfigSaveHint => (
            "Tab: form/raw · Ctrl+S: save · Esc: cancel",
            "Tab: フォーム/生 · Ctrl+S: 保存 · Esc: キャンセル",
        ),
        Key::FormFooter => (
            "Tab / ↑↓: move · Enter: submit · Esc: cancel",
            "Tab / ↑↓: 移動 · Enter: 実行 · Esc: キャンセル",
        ),
        Key::HelpBody => (HELP_EN, HELP_JA),
        Key::QuitPrompt => (
            "Quit wslm?\n\nEnter / y: quit · Esc / n: stay",
            "wslm を終了しますか？\n\nEnter / y: 終了 · Esc / n: 戻る",
        ),
        Key::LabelExportPath => ("Output file name", "出力ファイル名"),
        Key::LabelImportNameOnly => ("New distro name", "新しいディストロ名"),
        Key::LabelImportCustomArchive => ("Source archive path", "元アーカイブのパス"),
        Key::PickImportTitle => (
            " Import — pick an archive ",
            " インポート — アーカイブを選択 ",
        ),
        Key::PickImportEmpty => (
            "(no archives in exports\\ — press c for a custom path)",
            "(exports\\ にアーカイブがありません — c で任意パス)",
        ),
        Key::PickImportHints => (
            "↑/↓ move · Enter import · c custom · d delete · Esc back",
            "↑/↓ 移動 · Enter 取込 · c 任意 · d 削除 · Esc 戻る",
        ),
        Key::ExportFormatHint => (
            "Saved under exports\\; extension picks format (.tar/.tar.gz/.tar.xz/.vhdx)",
            "exports\\ に保存。拡張子で形式選択 (.tar/.tar.gz/.tar.xz/.vhdx)",
        ),
        Key::PromptDeleteArchive => ("Delete archive '{}'?", "アーカイブ '{}' を削除しますか？"),
        Key::DoneDeletedArchive => ("Deleted '{}'", "'{}' を削除しました"),
        Key::PromptTerminate => ("Terminate (stop) '{}'?", "'{}' を停止しますか？"),
        Key::PromptShutdown => (
            "Shut down ALL running WSL distributions?",
            "実行中の全 WSL ディストロを停止しますか？",
        ),
        Key::PromptUnregister => (
            "PERMANENTLY delete '{}' and ALL its data.",
            "'{}' とその全データを完全に削除します。",
        ),
        Key::PromptImportOverwrite => (
            "'{}' already exists. Overwrite it?",
            "'{}' は既に存在します。上書きしますか？",
        ),
        Key::StatusFetching => (
            "Fetching available distributions…",
            "インストール可能なディストロを取得中…",
        ),
        Key::StatusLoading => ("Loading {}…", "{} を読み込み中…"),
        Key::StatusStarting => ("Starting {}…", "{} を起動中…"),
        Key::StatusSettingDefault => ("Setting {} as default…", "{} を既定に設定中…"),
        Key::StatusLaunchingShell => ("Launching '{}' shell…", "'{}' のシェルを起動中…"),
        Key::StatusCancelling => ("Cancelling…", "キャンセル中…"),
        Key::StatusSaving => ("Saving {}…", "{} を保存中…"),
        Key::StatusReturnedFrom => ("Returned from '{}'", "'{}' から復帰しました"),
        Key::ProgExporting => ("Exporting '{}'", "'{}' をエクスポート中"),
        Key::ProgImporting => ("Importing '{}'", "'{}' をインポート中"),
        Key::ProgInstalling => ("Installing '{}'", "'{}' をインストール中"),
        Key::DoneStarted => ("Started {}", "{} を起動しました"),
        Key::DoneTerminated => ("Terminated {}", "{} を停止しました"),
        Key::DoneShutdown => ("WSL shut down", "WSL を全停止しました"),
        Key::DoneSetDefault => ("Set {} as default", "{} を既定に設定しました"),
        Key::DoneUnregistered => ("Unregistered {}", "{} を登録解除しました"),
        Key::DoneExported => ("Exported '{}'", "'{}' をエクスポートしました"),
        Key::DoneImported => ("Imported '{}'", "'{}' をインポートしました"),
        Key::DoneInstalled => ("Installed '{}'", "'{}' をインストールしました"),
        Key::DoneOpenedTab => (
            "Opened '{}' in a new Windows Terminal tab",
            "'{}' を Windows Terminal の新規タブで開きました",
        ),
        Key::DoneSavedConfig => (
            "Saved {} — run `wsl --shutdown` to apply",
            "{} を保存しました — 反映には `wsl --shutdown` が必要です",
        ),
        Key::FailOp => ("Operation failed: {}", "操作に失敗しました: {}"),
        Key::FailListOnline => (
            "Failed to list online distros: {}",
            "オンライン一覧の取得に失敗: {}",
        ),
        Key::FailLoadConfig => ("Failed to load config: {}", "設定の読み込みに失敗: {}"),
        Key::FailSaveConfig => ("Failed to save config: {}", "設定の保存に失敗: {}"),
        Key::WtNotFound => (
            "Windows Terminal (wt.exe) not found. Press Enter for an inline shell instead.",
            "Windows Terminal (wt.exe) が見つかりません。Enter でインラインシェルを使ってください。",
        ),
        Key::ShellBanner => (
            "Launching WSL shell for '{}' — type 'exit' to return to wslm.",
            "'{}' の WSL シェルを起動します — 'exit' で wslm に戻ります。",
        ),
    }
}

const HELP_EN: &str = "\
 j/k · ↑/↓     move selection
 /             filter list (Esc clears)
 Enter         inline shell (exit returns to wslm)
 w             shell in a new Windows Terminal tab
 s             start (boot) the distro
 x             stop (terminate) the distro
 X             shut down the whole WSL VM
 d             set as default
 u             unregister — delete (type name to confirm)
 e             export to the managed folder
 m             import (pick from the managed folder)
 i             install from the online catalog
 c / C         edit .wslconfig / wsl.conf
 L             toggle English / Japanese
 r             refresh now
 ?             this help
 q / Esc       quit  (Ctrl+C too; all ask to confirm)

 Press any key to close.";

const HELP_JA: &str = "\
 j/k · ↑/↓     選択を移動
 /             一覧をフィルタ (Esc で解除)
 Enter         インラインシェル (exit で wslm に復帰)
 w             Windows Terminal の新規タブでシェル
 s             起動 (boot)
 x             停止 (terminate)
 X             WSL VM 全体を停止
 d             既定に設定
 u             登録解除 — 削除 (名前入力で確認)
 e             管理フォルダにエクスポート
 m             インポート（管理フォルダから選択）
 i             オンライン一覧からインストール
 c / C         .wslconfig / wsl.conf を編集
 L             英語 / 日本語 切替
 r             今すぐ更新
 ?             このヘルプ
 q / Esc       終了  (Ctrl+C も同様・いずれも確認あり)

 任意のキーで閉じます。";

/// The translated string for `key` in `lang`.
pub fn t(lang: Lang, key: Key) -> &'static str {
    let (en, ja) = entry(key);
    match lang {
        Lang::En => en,
        Lang::Ja => ja,
    }
}

/// The translated string with positional `{}` placeholders replaced by `args`.
pub fn tf(lang: Lang, key: Key, args: &[&str]) -> String {
    let mut text = t(lang, key).to_string();
    for arg in args {
        if let Some(pos) = text.find("{}") {
            text.replace_range(pos..pos + 2, arg);
        }
    }
    text
}

/// Guess the default language from the environment, defaulting to English.
pub fn detect_default_lang() -> Lang {
    for var in ["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Some(value) = std::env::var_os(var) {
            if value.to_string_lossy().to_ascii_lowercase().contains("ja") {
                return Lang::Ja;
            }
        }
    }
    Lang::En
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_both_languages() {
        for &key in Key::ALL {
            assert!(!t(Lang::En, key).is_empty(), "missing EN for {key:?}");
            assert!(!t(Lang::Ja, key).is_empty(), "missing JA for {key:?}");
        }
    }

    #[test]
    fn toggled_swaps() {
        assert_eq!(Lang::En.toggled(), Lang::Ja);
        assert_eq!(Lang::Ja.toggled(), Lang::En);
    }

    #[test]
    fn tf_substitutes_positionally() {
        assert_eq!(
            tf(Lang::En, Key::PromptTerminate, &["Debian"]),
            "Terminate (stop) 'Debian'?"
        );
        assert_eq!(
            tf(Lang::Ja, Key::PromptTerminate, &["Debian"]),
            "'Debian' を停止しますか？"
        );
    }

    #[test]
    fn tf_handles_two_placeholders() {
        assert_eq!(
            tf(Lang::En, Key::ConfirmTypedLine, &["Debian", "Deb"]),
            "type \"Debian\" to confirm: Deb"
        );
    }
}
