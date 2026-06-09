//! UI カラーパレットと、状態・使用率から色を決める純粋関数。`ui` 層内のみ参照。
//! セマンティック色（緑/黄/赤）とアクセント色（ティール）を分離するのが原則。

use ratatui::style::Color;

use crate::wsl::DistroState;

/// アクセント（署名色）: タイトル・フォーカス枠・フッターのキー記号。
pub(super) const ACCENT: Color = Color::Rgb(38, 166, 154); // teal
/// 二次情報（ラベル・パス・ヘッダ・空ゲージ）。
pub(super) const DIM: Color = Color::DarkGray;
/// 既定★。
pub(super) const STAR: Color = Color::Yellow;
/// スパークライン。
pub(super) const SPARK: Color = Color::Cyan;
/// 選択行の背景（淡いティール）。
pub(super) const SELECTION_BG: Color = Color::Rgb(16, 48, 48);

/// ゲージ警告色（使用率 ≥75%）。
const WARN: Color = Color::Rgb(240, 160, 48); // amber
/// ゲージ危険色（使用率 ≥90%）。
const CRIT: Color = Color::Red;

/// ディストロ状態の色。Running=緑 / Stopped=dim / Installing=アンバー / Unknown=赤。
pub(super) fn state_color(state: DistroState) -> Color {
    match state {
        DistroState::Running => Color::Green,
        DistroState::Stopped => DIM,
        DistroState::Installing => WARN,
        DistroState::Unknown => CRIT,
    }
}

/// 使用率(0.0..=1.0)からゲージ充填色を決める。<0.75 ティール / <0.90 アンバー / それ以上 赤。
pub(super) fn gauge_color(ratio: f64) -> Color {
    if ratio >= 0.90 {
        CRIT
    } else if ratio >= 0.75 {
        WARN
    } else {
        ACCENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_color_maps_each_variant() {
        assert_eq!(state_color(DistroState::Running), Color::Green);
        assert_eq!(state_color(DistroState::Stopped), DIM);
        assert_eq!(state_color(DistroState::Installing), WARN);
        assert_eq!(state_color(DistroState::Unknown), CRIT);
    }

    #[test]
    fn gauge_color_thresholds() {
        assert_eq!(gauge_color(0.0), ACCENT);
        assert_eq!(gauge_color(0.749), ACCENT);
        assert_eq!(gauge_color(0.75), WARN);
        assert_eq!(gauge_color(0.899), WARN);
        assert_eq!(gauge_color(0.90), CRIT);
        assert_eq!(gauge_color(1.0), CRIT);
    }
}
