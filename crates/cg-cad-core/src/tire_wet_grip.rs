//! タイヤトレッドパターンの雨天時グリップ簡易参考指標(MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: 本モジュール
//! が行うのは、実務のタイヤメーカーが行う本格的なFEMタイヤモデル・
//! 路面接触シミュレーション・実車/実タイヤの湿潤路面試験(ウェットグリップ
//! 等級試験等)ではなく、**トレッドの溝面積比率(ネガティブ率)・溝深さ・
//! サイプ密度から求めた極めて単純化した排水性/グリップの参考スコア**
//! である。実際のゴム配合(コンパウンド)特性・接地圧分布・温度依存性・
//! 摩耗進行・実路面のミクロ/マクロテクスチャは一切考慮していない。
//! 出力される数値は教育的な参考値に過ぎず、実際のタイヤ設計・法規適合性
//! (欧州タイヤラベリング等)の根拠には一切使えない。
//!
//! # 参照した一般原理(公開情報)
//!
//! - 「溝面積比率(ネガティブ率)が高いほど排水性が向上し、ハイドロプレーニング
//!   を抑制する」「サイプ(細溝)がエッジ効果によりウェットグリップを高める」
//!   という定性的原理は、タイヤ工学の一般解説・公開特許明細書レベルで
//!   広く公開されている一般原理であり、特定タイヤメーカーの独自コンパウンド
//!   技術・特許意匠そのものではない。
//! - 本コードはこれらの公開されている一般原理を単純化した代表式として
//!   実装したものであり、特定企業の実際のトレッドパターンを再現・模倣
//!   するものではない。

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum TireWetGripError {
    #[error("groove area ratio must be within [0, 1], got {0}")]
    InvalidGrooveRatio(f64),
    #[error("groove depth must be non-negative, got {0}")]
    NegativeGrooveDepth(f64),
    #[error("sipe density must be non-negative, got {0}")]
    NegativeSipeDensity(f64),
}

/// トレッドパターンパラメータ。
#[derive(Debug, Clone, Copy)]
pub struct TreadPattern {
    /// 溝面積比率(ネガティブ率、0.0〜1.0。トレッド接地面のうち溝が占める割合)。
    pub groove_area_ratio: f64,
    /// 溝深さ(mm)。
    pub groove_depth_mm: f64,
    /// サイプ密度(1cm幅あたりの本数の代表値、簡易指標)。
    pub sipe_density_per_cm: f64,
}

impl Default for TreadPattern {
    fn default() -> Self {
        TreadPattern {
            groove_area_ratio: 0.3,
            groove_depth_mm: 8.0,
            sipe_density_per_cm: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WetGripResult {
    /// 排水性スコア(0以上、大きいほど排水性が良いと見なす簡易参考値)。
    pub drainage_score: f64,
    /// エッジ効果スコア(サイプによる引っかかり効果の簡易参考値)。
    pub edge_effect_score: f64,
    /// 総合ウェットグリップ参考指標(0〜100、あくまで相対比較用)。
    pub wet_grip_index: f64,
    /// ハイドロプレーニング参考限界速度(km/h、溝面積比率・溝深さからの
    /// 極めて単純化した目安値)。
    pub hydroplaning_reference_speed_kmh: f64,
}

/// トレッドパターンから雨天時グリップの簡易参考指標を求める。
pub fn evaluate_wet_grip(pattern: &TreadPattern) -> Result<WetGripResult, TireWetGripError> {
    if !(0.0..=1.0).contains(&pattern.groove_area_ratio) {
        return Err(TireWetGripError::InvalidGrooveRatio(
            pattern.groove_area_ratio,
        ));
    }
    if pattern.groove_depth_mm < 0.0 {
        return Err(TireWetGripError::NegativeGrooveDepth(
            pattern.groove_depth_mm,
        ));
    }
    if pattern.sipe_density_per_cm < 0.0 {
        return Err(TireWetGripError::NegativeSipeDensity(
            pattern.sipe_density_per_cm,
        ));
    }

    // 排水性は溝面積比率×溝深さにおおむね比例するという単純化。
    let drainage_score = pattern.groove_area_ratio * pattern.groove_depth_mm;

    // エッジ効果はサイプ密度に比例するという単純化。
    let edge_effect_score = pattern.sipe_density_per_cm * 10.0;

    // 総合指標: 排水性・エッジ効果の加重和を0〜100へ正規化する簡易式
    // (代表的な最大値: groove_ratio=1.0, depth=12mm → drainage=12,
    //  sipe_density=3 → edge=30 を概ね100に近づける係数で正規化)。
    let raw = drainage_score * 5.0 + edge_effect_score * 1.0;
    let wet_grip_index = (raw / 1.9).min(100.0);

    // ハイドロプレーニング参考限界速度: 溝面積比率・溝深さが大きいほど
    // 高くなる(排水しきれる速度域が上がる)という単純な線形近似の代表式。
    let hydroplaning_reference_speed_kmh =
        60.0 + pattern.groove_area_ratio * 60.0 + pattern.groove_depth_mm * 2.0;

    Ok(WetGripResult {
        drainage_score,
        edge_effect_score,
        wet_grip_index,
        hydroplaning_reference_speed_kmh,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_groove_ratio() {
        let p = TreadPattern {
            groove_area_ratio: 1.5,
            ..Default::default()
        };
        assert_eq!(
            evaluate_wet_grip(&p).unwrap_err(),
            TireWetGripError::InvalidGrooveRatio(1.5)
        );
    }

    #[test]
    fn zero_groove_ratio_yields_zero_drainage() {
        let p = TreadPattern {
            groove_area_ratio: 0.0,
            groove_depth_mm: 8.0,
            sipe_density_per_cm: 0.0,
        };
        let r = evaluate_wet_grip(&p).unwrap();
        assert_eq!(r.drainage_score, 0.0);
        assert_eq!(r.edge_effect_score, 0.0);
    }

    #[test]
    fn more_grooves_improve_wet_grip_index_monotonically() {
        let low = TreadPattern {
            groove_area_ratio: 0.1,
            groove_depth_mm: 4.0,
            sipe_density_per_cm: 0.5,
        };
        let high = TreadPattern {
            groove_area_ratio: 0.4,
            groove_depth_mm: 8.0,
            sipe_density_per_cm: 1.5,
        };
        let r_low = evaluate_wet_grip(&low).unwrap();
        let r_high = evaluate_wet_grip(&high).unwrap();
        assert!(
            r_high.wet_grip_index > r_low.wet_grip_index,
            "high={}, low={}",
            r_high.wet_grip_index,
            r_low.wet_grip_index
        );
        assert!(
            r_high.hydroplaning_reference_speed_kmh > r_low.hydroplaning_reference_speed_kmh
        );
    }

    #[test]
    fn wet_grip_index_is_capped_at_100() {
        let extreme = TreadPattern {
            groove_area_ratio: 1.0,
            groove_depth_mm: 100.0,
            sipe_density_per_cm: 100.0,
        };
        let r = evaluate_wet_grip(&extreme).unwrap();
        assert_eq!(r.wet_grip_index, 100.0);
    }
}
