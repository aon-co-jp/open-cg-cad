//! 強風(横風)時の転倒モーメント・横滑りリスクの簡易指標計算(MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: 本モジュール
//! が行うのは、実務の車両・鉄道車両の横風安全性評価で使われる本格的な
//! CFD・風洞実験・多体動力学シミュレーション(ADAMS等)ではなく、
//! **側面投影面積・重心高さ・輪距(トレッド幅)から求めた、静的な力の
//! モーメント釣り合いによる転倒限界風速の概算**である。車体のロール
//! 剛性・サスペンション特性・路面の傾き・車両速度との合成風向・突風の
//! 時間変動は一切考慮していない。出力される数値は教育的な参考値に過ぎず、
//! 実際の車両・鉄道の運行規制根拠には一切使えない。
//!
//! # 参照した一般原理(公開情報)
//!
//! - 高速鉄道の横風転倒事故解析における「転倒モーメント = 側面風荷重 ×
//!   重心高さ、復元モーメント = 車両重量 × (輪距/2)」という静的モーメント
//!   釣り合いの考え方は、鉄道工学・車両力学の教科書レベルで広く公開されて
//!   いる一般原理(国内外の高速鉄道横風安全性に関する学術論文でも同様の
//!   簡易評価指標が言及される)。
//! - 本コードはこの公開されている一般原理の単純化した実装であり、
//!   特定企業・特定車両形式の実際の転倒限界風速データを再現するものでは
//!   ない。

use thiserror::Error;

use crate::aero::AIR_DENSITY_KG_PER_M3;

#[derive(Debug, Error, PartialEq)]
pub enum CrosswindError {
    #[error("side area must be positive, got {0}")]
    NonPositiveSideArea(f64),
    #[error("mass must be positive, got {0}")]
    NonPositiveMass(f64),
    #[error("track/wheel gauge must be positive, got {0}")]
    NonPositiveTrackWidth(f64),
    #[error("center of gravity height must be positive, got {0}")]
    NonPositiveCogHeight(f64),
}

/// 横風安全性評価用の車両パラメータ。
#[derive(Debug, Clone, Copy)]
pub struct CrosswindVehicle {
    /// 側面投影面積(m^2)。
    pub side_area_m2: f64,
    /// 車両質量(kg)。
    pub mass_kg: f64,
    /// 輪距/軌間(トレッド幅、m)。転倒軸(片側車輪接地点)までの距離の基準。
    pub track_width_m: f64,
    /// 重心高さ(m、地上/レール面から)。
    pub center_of_gravity_height_m: f64,
    /// 側面風圧を受ける形状の抗力係数(簡易代表値、平板〜流線形で0.8〜1.2程度)。
    pub side_drag_coefficient: f64,
}

impl Default for CrosswindVehicle {
    fn default() -> Self {
        CrosswindVehicle {
            side_area_m2: 8.0,
            mass_kg: 2000.0,
            track_width_m: 1.6,
            center_of_gravity_height_m: 0.6,
            side_drag_coefficient: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CrosswindResult {
    /// 側面風荷重(N)。
    pub side_force_n: f64,
    /// 転倒モーメント(N・m、重心高さ周り)。
    pub overturning_moment_n_m: f64,
    /// 復元モーメント(N・m、車両自重×輪距半分)。
    pub restoring_moment_n_m: f64,
    /// 転倒安全率(復元モーメント/転倒モーメント)。1.0未満で転倒リスク域の目安。
    pub overturn_safety_factor: f64,
    /// 転倒限界とみなす簡易目安風速(m/s、安全率が1.0になる風速)。
    pub critical_wind_speed_m_per_s: f64,
}

/// 横風速度・車両パラメータから転倒モーメント/安全率を概算する。
pub fn evaluate_crosswind(
    vehicle: &CrosswindVehicle,
    wind_speed_m_per_s: f64,
) -> Result<CrosswindResult, CrosswindError> {
    if vehicle.side_area_m2 <= 0.0 {
        return Err(CrosswindError::NonPositiveSideArea(vehicle.side_area_m2));
    }
    if vehicle.mass_kg <= 0.0 {
        return Err(CrosswindError::NonPositiveMass(vehicle.mass_kg));
    }
    if vehicle.track_width_m <= 0.0 {
        return Err(CrosswindError::NonPositiveTrackWidth(vehicle.track_width_m));
    }
    if vehicle.center_of_gravity_height_m <= 0.0 {
        return Err(CrosswindError::NonPositiveCogHeight(
            vehicle.center_of_gravity_height_m,
        ));
    }

    let wind_speed_m_per_s = wind_speed_m_per_s.abs();
    let side_force_n = 0.5
        * AIR_DENSITY_KG_PER_M3
        * wind_speed_m_per_s
        * wind_speed_m_per_s
        * vehicle.side_area_m2
        * vehicle.side_drag_coefficient;

    let overturning_moment_n_m = side_force_n * vehicle.center_of_gravity_height_m;
    const G_M_PER_S2: f64 = 9.80665;
    let restoring_moment_n_m =
        vehicle.mass_kg * G_M_PER_S2 * (vehicle.track_width_m / 2.0);

    let overturn_safety_factor = if overturning_moment_n_m > 0.0 {
        restoring_moment_n_m / overturning_moment_n_m
    } else {
        f64::INFINITY
    };

    // side_force_n = k * v^2 なので、restoring = k*v_c^2*cog より
    // v_c = sqrt(restoring / (0.5*rho*A*Cd*cog))
    let k = 0.5 * AIR_DENSITY_KG_PER_M3 * vehicle.side_area_m2 * vehicle.side_drag_coefficient
        * vehicle.center_of_gravity_height_m;
    let critical_wind_speed_m_per_s = if k > 0.0 {
        (restoring_moment_n_m / k).sqrt()
    } else {
        f64::INFINITY
    };

    Ok(CrosswindResult {
        side_force_n,
        overturning_moment_n_m,
        restoring_moment_n_m,
        overturn_safety_factor,
        critical_wind_speed_m_per_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_positive_side_area() {
        let v = CrosswindVehicle {
            side_area_m2: 0.0,
            ..Default::default()
        };
        assert_eq!(
            evaluate_crosswind(&v, 20.0).unwrap_err(),
            CrosswindError::NonPositiveSideArea(0.0)
        );
    }

    #[test]
    fn zero_wind_yields_zero_moment_and_infinite_safety() {
        let v = CrosswindVehicle::default();
        let r = evaluate_crosswind(&v, 0.0).unwrap();
        assert_eq!(r.side_force_n, 0.0);
        assert_eq!(r.overturning_moment_n_m, 0.0);
        assert!(r.overturn_safety_factor.is_infinite());
    }

    #[test]
    fn safety_factor_decreases_as_wind_speed_increases() {
        let v = CrosswindVehicle::default();
        let low = evaluate_crosswind(&v, 10.0).unwrap();
        let high = evaluate_crosswind(&v, 30.0).unwrap();
        assert!(
            high.overturn_safety_factor < low.overturn_safety_factor,
            "high={}, low={}",
            high.overturn_safety_factor,
            low.overturn_safety_factor
        );
    }

    #[test]
    fn at_critical_wind_speed_safety_factor_is_approximately_one() {
        let v = CrosswindVehicle::default();
        let r = evaluate_crosswind(&v, 0.0).unwrap();
        let critical = r.critical_wind_speed_m_per_s;
        let at_critical = evaluate_crosswind(&v, critical).unwrap();
        assert!(
            (at_critical.overturn_safety_factor - 1.0).abs() < 1e-6,
            "got {}",
            at_critical.overturn_safety_factor
        );
    }

    #[test]
    fn higher_center_of_gravity_reduces_critical_wind_speed() {
        let low_cog = CrosswindVehicle {
            center_of_gravity_height_m: 0.5,
            ..Default::default()
        };
        let high_cog = CrosswindVehicle {
            center_of_gravity_height_m: 2.0,
            ..Default::default()
        };
        let r_low = evaluate_crosswind(&low_cog, 0.0).unwrap();
        let r_high = evaluate_crosswind(&high_cog, 0.0).unwrap();
        assert!(
            r_high.critical_wind_speed_m_per_s < r_low.critical_wind_speed_m_per_s,
            "high_cog_critical={}, low_cog_critical={}",
            r_high.critical_wind_speed_m_per_s,
            r_low.critical_wind_speed_m_per_s
        );
    }
}
