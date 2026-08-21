//! 車両・列車の簡易空力特性計算(ダウンフォース/揚力の概算、MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: 本モジュール
//! が行うのは、実務の空力開発で使われる本格的なCFD(数値流体力学、
//! Navier-Stokes方程式の解法)や風洞実験ではなく、**前面投影面積・
//! 傾斜角(迎え角)から、ベルヌーイの定理・揚力係数の一般公式に基づく
//! 極めて単純化した準静的な力の概算**である。乱流・境界層剥離・
//! 車体表面の複雑な3次元形状・地面効果の詳細・他車両との干渉(コンボイ
//! 走行時の後流影響等)は一切考慮していない。出力される数値は、実際の
//! 車両設計・法規適合性の根拠には一切使えず、専門の空力エンジニア・
//! CFD解析・風洞実験によるレビューを経ない限り、教育的な参考値に過ぎない。
//!
//! # 参照した一般原理(公開情報、特定企業の企業秘密ではない)
//!
//! - ベルヌーイの定理・揚力係数`Cl`の一般公式(`L = 0.5 * rho * v^2 * A * Cl`)は、
//!   航空工学・自動車工学の教科書レベルで広く公開されている一般原理であり、
//!   特定企業の独自技術ではない。
//! - ウイング・ディフューザー・アンダーパネル等がベンチュリ効果により
//!   ダウンフォースを生む、という定性的原理も同様に公開されている一般原理
//!   (Wikipedia「ダウンフォース」等の一般解説記事、Road Vehicle Aerodynamic
//!   Design (Barnard)等の教科書に記載)。
//! - 本コードは、これらの**公開されている一般原理**を単純化した代表式として
//!   実装したものであり、特定企業(BMW等)の実際の翼形状データ・CFD結果・
//!   特許技術を調査・模倣したものではない。
//!
//! # 簡略化している点(正直な開示)
//!
//! - 揚力係数`Cl`は、傾斜角(迎え角)に比例するという線形近似
//!   (`Cl = Cl0 + k * angle_deg`)を用いており、実際には角度・形状に対して
//!   非線形(失速角以降は急減する等)である。
//! - 空気密度は標準大気の代表値(1.225 kg/m^3)を既定とし、高度・気温による
//!   変化は考慮していない。
//! - 地面効果(車体下面と路面の間隔による増幅)は考慮していない。
//! - コンボイ走行時の前走車両による後流(ドラフティング)効果は未対応。

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum AeroError {
    #[error("frontal area must be positive, got {0}")]
    NonPositiveArea(f64),
    #[error("speed must be non-negative, got {0}")]
    NegativeSpeed(f64),
}

/// 空力形状パラメータ(簡易モデル用)。
#[derive(Debug, Clone, Copy)]
pub struct AeroShape {
    /// 前面投影面積(m^2)。
    pub frontal_area_m2: f64,
    /// 車体下面・ウイング等の傾斜角(度)。正の値はダウンフォース方向
    /// (ウイングの迎え角がダウンフォースを生む向き)を表す簡易的な符号規約。
    pub tilt_angle_deg: f64,
    /// 基準揚力係数(傾斜角0度の場合の`Cl`。流線形の乗用車で正の揚力
    /// (浮き上がり)寄りの値を既定とする)。
    pub base_lift_coefficient: f64,
}

impl Default for AeroShape {
    fn default() -> Self {
        AeroShape {
            frontal_area_m2: 2.2,
            tilt_angle_deg: 0.0,
            base_lift_coefficient: 0.3,
        }
    }
}

/// 標準大気の空気密度(kg/m^3、海面高度・15℃の代表値)。
pub const AIR_DENSITY_KG_PER_M3: f64 = 1.225;

/// 傾斜角1度あたりの揚力係数の変化率(簡易線形近似の代表値)。
/// 負の値: 角度が増えるほどダウンフォース方向(Clが減少)に働くという
/// ウイング・ディフューザーの定性的挙動を表す代表係数。
const LIFT_COEFF_PER_DEG: f64 = -0.02;

#[derive(Debug, Clone, Copy)]
pub struct AeroForceResult {
    /// 揚力係数(正: 揚力〈浮き上がり〉方向、負: ダウンフォース方向)。
    pub lift_coefficient: f64,
    /// 揚力(N)。正の値は車体を持ち上げる方向、負の値はダウンフォース。
    pub lift_force_n: f64,
    /// ダウンフォース(N、正の値のみ。揚力が正〈浮き上がり〉の場合は0)。
    pub downforce_n: f64,
}

/// 揚力係数を、傾斜角からの線形近似で求める。
fn lift_coefficient(shape: &AeroShape) -> f64 {
    shape.base_lift_coefficient + LIFT_COEFF_PER_DEG * shape.tilt_angle_deg
}

/// 走行速度・形状パラメータから、簡易ベルヌーイ近似による揚力/ダウンフォースを
/// 概算する。
///
/// `L = 0.5 * rho * v^2 * A * Cl` (Cl>0は揚力、Cl<0はダウンフォース)。
pub fn estimate_aero_force(
    shape: &AeroShape,
    speed_m_per_s: f64,
) -> Result<AeroForceResult, AeroError> {
    if shape.frontal_area_m2 <= 0.0 {
        return Err(AeroError::NonPositiveArea(shape.frontal_area_m2));
    }
    if speed_m_per_s < 0.0 {
        return Err(AeroError::NegativeSpeed(speed_m_per_s));
    }

    let cl = lift_coefficient(shape);
    let lift_force_n =
        0.5 * AIR_DENSITY_KG_PER_M3 * speed_m_per_s * speed_m_per_s * shape.frontal_area_m2 * cl;
    let downforce_n = if lift_force_n < 0.0 { -lift_force_n } else { 0.0 };

    Ok(AeroForceResult {
        lift_coefficient: cl,
        lift_force_n,
        downforce_n,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_positive_area() {
        let shape = AeroShape {
            frontal_area_m2: 0.0,
            ..Default::default()
        };
        let err = estimate_aero_force(&shape, 30.0).unwrap_err();
        assert_eq!(err, AeroError::NonPositiveArea(0.0));
    }

    #[test]
    fn rejects_negative_speed() {
        let shape = AeroShape::default();
        let err = estimate_aero_force(&shape, -1.0).unwrap_err();
        assert_eq!(err, AeroError::NegativeSpeed(-1.0));
    }

    #[test]
    fn zero_speed_yields_zero_force() {
        let shape = AeroShape::default();
        let result = estimate_aero_force(&shape, 0.0).unwrap();
        assert_eq!(result.lift_force_n, 0.0);
        assert_eq!(result.downforce_n, 0.0);
    }

    #[test]
    fn downforce_grows_with_square_of_speed() {
        // 下壓力(ダウンフォース)は速度の2乗に比例する(台湾語検索結果でも
        // 確認した一般原理: 「車速快一倍，下壓力變四倍」)。
        let shape = AeroShape {
            tilt_angle_deg: 30.0, // 十分な角度でCl<0(ダウンフォース方向)にする
            ..Default::default()
        };
        let r1 = estimate_aero_force(&shape, 20.0).unwrap();
        let r2 = estimate_aero_force(&shape, 40.0).unwrap();
        assert!(r1.downforce_n > 0.0, "expected downforce at v=20, got {:?}", r1);
        let ratio = r2.downforce_n / r1.downforce_n;
        assert!((ratio - 4.0).abs() < 1e-6, "expected 4x downforce at 2x speed, got ratio={ratio}");
    }

    #[test]
    fn increasing_tilt_angle_reduces_lift_monotonically() {
        let shape_flat = AeroShape {
            tilt_angle_deg: 0.0,
            ..Default::default()
        };
        let shape_tilted = AeroShape {
            tilt_angle_deg: 20.0,
            ..Default::default()
        };
        let r_flat = estimate_aero_force(&shape_flat, 30.0).unwrap();
        let r_tilted = estimate_aero_force(&shape_tilted, 30.0).unwrap();
        assert!(
            r_tilted.lift_force_n < r_flat.lift_force_n,
            "tilted={}, flat={}",
            r_tilted.lift_force_n,
            r_flat.lift_force_n
        );
    }

    #[test]
    fn large_tilt_angle_produces_downforce_not_lift() {
        let shape = AeroShape {
            tilt_angle_deg: 50.0,
            base_lift_coefficient: 0.3,
            ..Default::default()
        };
        let result = estimate_aero_force(&shape, 25.0).unwrap();
        assert!(result.lift_coefficient < 0.0);
        assert!(result.downforce_n > 0.0);
        assert!(result.lift_force_n < 0.0);
    }
}
