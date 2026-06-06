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
    StatusHint,
    Loading,
    ErrorPrefix,
    NoDistros,
    ColName,
    ColState,
    ColVer,
    ColDefault,
    ColDisk,
    DetailState,
    DetailVersion,
    DetailDefault,
    DetailDisk,
    DetailPath,
    DetailVmMem,
    StateRunning,
    StateStopped,
    StateInstalling,
    StateUnknown,
    VmNotRunning,
    VmSharedNote,
    ConfirmHintTyped,
    ConfirmHintYesNo,
    ErrorDismiss,
    ProgressHint,
    InstallTitle,
    InstallHint,
    ConfigSaveHint,
    PromptTerminate,
    PromptShutdown,
    PromptUnregister,
    PromptImportOverwrite,
}

impl Key {
    /// All keys (used to verify catalog completeness in tests).
    pub const ALL: &'static [Key] = &[
        Key::StatusHint,
        Key::Loading,
        Key::ErrorPrefix,
        Key::NoDistros,
        Key::ColName,
        Key::ColState,
        Key::ColVer,
        Key::ColDefault,
        Key::ColDisk,
        Key::DetailState,
        Key::DetailVersion,
        Key::DetailDefault,
        Key::DetailDisk,
        Key::DetailPath,
        Key::DetailVmMem,
        Key::StateRunning,
        Key::StateStopped,
        Key::StateInstalling,
        Key::StateUnknown,
        Key::VmNotRunning,
        Key::VmSharedNote,
        Key::ConfirmHintTyped,
        Key::ConfirmHintYesNo,
        Key::ErrorDismiss,
        Key::ProgressHint,
        Key::InstallTitle,
        Key::InstallHint,
        Key::ConfigSaveHint,
        Key::PromptTerminate,
        Key::PromptShutdown,
        Key::PromptUnregister,
        Key::PromptImportOverwrite,
    ];
}

/// The (English, Japanese) pair for a key.
fn entry(key: Key) -> (&'static str, &'static str) {
    match key {
        Key::StatusHint => (
            "j/k move · Enter shell · s start · x stop · X shutdown · d default · u unreg · e export · m import · i install · c/C config · L lang · r refresh · q quit",
            "j/k 移動 · Enter シェル · s 起動 · x 停止 · X 全停止 · d 既定 · u 登録解除 · e エクスポート · m インポート · i インストール · c/C 設定 · L 言語 · r 更新 · q 終了",
        ),
        Key::Loading => ("loading…", "読み込み中…"),
        Key::ErrorPrefix => ("error", "エラー"),
        Key::NoDistros => ("No distributions.", "ディストリビューションがありません。"),
        Key::ColName => ("NAME", "名前"),
        Key::ColState => ("STATE", "状態"),
        Key::ColVer => ("VER", "Ver"),
        Key::ColDefault => ("DEFAULT", "既定"),
        Key::ColDisk => ("DISK", "ディスク"),
        Key::DetailState => ("State", "状態"),
        Key::DetailVersion => ("Version", "バージョン"),
        Key::DetailDefault => ("Default", "既定"),
        Key::DetailDisk => ("Disk", "ディスク"),
        Key::DetailPath => ("Path", "パス"),
        Key::DetailVmMem => ("VM Mem", "VMメモリ"),
        Key::StateRunning => ("Running", "実行中"),
        Key::StateStopped => ("Stopped", "停止"),
        Key::StateInstalling => ("Installing", "インストール中"),
        Key::StateUnknown => ("Unknown", "不明"),
        Key::VmNotRunning => ("— (WSL VM not running)", "— (WSL VM 停止中)"),
        Key::VmSharedNote => (
            "(vmmemWSL, shared by all distros)",
            "(vmmemWSL, 全ディストロ共有)",
        ),
        Key::ConfirmHintTyped => (
            "Enter: confirm (must match) · Esc: cancel",
            "Enter: 確定(一致必須) · Esc: キャンセル",
        ),
        Key::ConfirmHintYesNo => (
            "Enter / y: confirm · Esc / n: cancel",
            "Enter / y: 確定 · Esc / n: キャンセル",
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
    }
}

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
}
