//! 法規制パラメータ(建ぺい率・容積率等)のチェック。
//!
//! **重要な制約(免責事項)**: ここで扱う`ZoningRules`は、ユーザーが
//! 明示的に設定する入力値であり、Google検索等による現行の法規制の
//! 自動取得・自動反映は本クレートの今回のスコープ外(将来課題)。
//! 実際の建築確認申請には、有資格者(一級建築士等)が現地の都市計画
//! 情報・建築基準法の最新の解釈を確認した上での正式な数値を用いる
//! こと。本モジュールが返す判定結果は「ドラフト生成の入力条件を
//! 満たすかどうか」の簡易チェックに過ぎず、法的な適合性を保証しない。

use thiserror::Error;

/// 敷地に適用する法規制パラメータ(ユーザー設定可能な入力値)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoningRules {
    /// 建ぺい率(0.0〜1.0、例: 60%なら0.6)。
    pub building_coverage_ratio: f64,
    /// 容積率(0.0以上、例: 200%なら2.0)。
    pub floor_area_ratio: f64,
    /// 日影規制・高さ制限等、今回未実装の追加規制の有無を示すフラグ
    /// (常にfalse固定でドラフトを生成する設計。将来、日照シミュレーション
    /// 等を実装する際に使うプレースホルダ)。
    pub shadow_regulation_checked: bool,
}

impl ZoningRules {
    pub fn new(building_coverage_ratio: f64, floor_area_ratio: f64) -> Result<Self, ZoningError> {
        if !(0.0..=1.0).contains(&building_coverage_ratio) {
            return Err(ZoningError::InvalidCoverageRatio(building_coverage_ratio));
        }
        if floor_area_ratio <= 0.0 {
            return Err(ZoningError::InvalidFloorAreaRatio(floor_area_ratio));
        }
        Ok(ZoningRules { building_coverage_ratio, floor_area_ratio, shadow_regulation_checked: false })
    }

    /// 許容される最大建築面積(敷地面積 x 建ぺい率)。
    pub fn max_building_footprint_m2(&self, site_area_m2: f64) -> f64 {
        site_area_m2 * self.building_coverage_ratio
    }

    /// 許容される最大延床面積(敷地面積 x 容積率)。
    pub fn max_total_floor_area_m2(&self, site_area_m2: f64) -> f64 {
        site_area_m2 * self.floor_area_ratio
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ZoningError {
    #[error("building coverage ratio must be within [0.0, 1.0], got {0}")]
    InvalidCoverageRatio(f64),
    #[error("floor area ratio must be positive, got {0}")]
    InvalidFloorAreaRatio(f64),
    #[error("proposed building footprint {footprint_m2:.2}m2 exceeds max allowed {max_m2:.2}m2 (site {site_area_m2:.2}m2 x coverage ratio {ratio})")]
    FootprintExceedsCoverage { footprint_m2: f64, max_m2: f64, site_area_m2: f64, ratio: f64 },
    #[error("proposed total floor area {total_m2:.2}m2 exceeds max allowed {max_m2:.2}m2 (site {site_area_m2:.2}m2 x floor area ratio {ratio})")]
    TotalFloorAreaExceedsRatio { total_m2: f64, max_m2: f64, site_area_m2: f64, ratio: f64 },
}

/// 提案された建築面積・延床面積が、敷地面積と法規制パラメータに
/// 対して適合するかを検証する。適合しない場合は`ZoningError`で
/// 明確に却下する(黙って通さない)。
pub fn check_zoning_compliance(
    site_area_m2: f64,
    rules: &ZoningRules,
    proposed_footprint_m2: f64,
    proposed_total_floor_area_m2: f64,
) -> Result<(), ZoningError> {
    // 浮動小数点の丸め誤差(sqrt等を経た面積スケーリングで生じうる、
    // 数値的に無視できるオーバーシュート)を誤って却下しないよう、
    // 相対誤差1e-9程度の許容差を設ける。実質的な超過(意味のある差分)は
    // 引き続き正しく却下される。
    let epsilon = |limit: f64| limit.abs() * 1e-9 + 1e-9;

    let max_footprint = rules.max_building_footprint_m2(site_area_m2);
    if proposed_footprint_m2 > max_footprint + epsilon(max_footprint) {
        return Err(ZoningError::FootprintExceedsCoverage {
            footprint_m2: proposed_footprint_m2,
            max_m2: max_footprint,
            site_area_m2,
            ratio: rules.building_coverage_ratio,
        });
    }

    let max_total_floor_area = rules.max_total_floor_area_m2(site_area_m2);
    if proposed_total_floor_area_m2 > max_total_floor_area + epsilon(max_total_floor_area) {
        return Err(ZoningError::TotalFloorAreaExceedsRatio {
            total_m2: proposed_total_floor_area_m2,
            max_m2: max_total_floor_area,
            site_area_m2,
            ratio: rules.floor_area_ratio,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// タスク指示に明記された具体的検証ケース:
    /// 敷地面積100㎡・建ぺい率60%の場合、建築面積60㎡を超える設計は
    /// 却下される。
    #[test]
    fn footprint_exceeding_60_percent_coverage_on_100m2_site_is_rejected() {
        let rules = ZoningRules::new(0.6, 2.0).unwrap();
        let result = check_zoning_compliance(100.0, &rules, 61.0, 100.0);
        assert!(matches!(result, Err(ZoningError::FootprintExceedsCoverage { .. })));
    }

    #[test]
    fn footprint_exactly_at_60_percent_coverage_on_100m2_site_is_accepted() {
        let rules = ZoningRules::new(0.6, 2.0).unwrap();
        let result = check_zoning_compliance(100.0, &rules, 60.0, 100.0);
        assert!(result.is_ok());
    }

    #[test]
    fn footprint_below_coverage_limit_is_accepted() {
        let rules = ZoningRules::new(0.6, 2.0).unwrap();
        let result = check_zoning_compliance(100.0, &rules, 50.0, 80.0);
        assert!(result.is_ok());
    }

    #[test]
    fn total_floor_area_exceeding_floor_area_ratio_is_rejected() {
        // 敷地面積100㎡・容積率200%なら延床面積上限は200㎡
        let rules = ZoningRules::new(0.6, 2.0).unwrap();
        let result = check_zoning_compliance(100.0, &rules, 50.0, 201.0);
        assert!(matches!(result, Err(ZoningError::TotalFloorAreaExceedsRatio { .. })));
    }

    #[test]
    fn rejects_invalid_coverage_ratio_out_of_range() {
        assert_eq!(ZoningRules::new(1.5, 2.0), Err(ZoningError::InvalidCoverageRatio(1.5)));
        assert_eq!(ZoningRules::new(-0.1, 2.0), Err(ZoningError::InvalidCoverageRatio(-0.1)));
    }

    #[test]
    fn rejects_non_positive_floor_area_ratio() {
        assert_eq!(ZoningRules::new(0.6, 0.0), Err(ZoningError::InvalidFloorAreaRatio(0.0)));
        assert_eq!(ZoningRules::new(0.6, -1.0), Err(ZoningError::InvalidFloorAreaRatio(-1.0)));
    }
}
