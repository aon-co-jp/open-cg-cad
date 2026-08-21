//! nWD(2WD/4WD/6WD/8WD以上)駆動方式の簡易トルク配分シミュレーション
//! (公開されているトルクベクタリング理論に基づく教育的参考モデル、MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: 本モジュール
//! が行うのは、実務の車両ECUが行う本格的な電子制御(実際のセンサ融合・
//! リアルタイムの車輪スリップ推定・実車パラメータへのチューニング)では
//! なく、**要求ヨーモーメントを各輪の駆動力配分へ変換する、教科書レベルの
//! 静的な最適トラクション配分の簡易近似**である。実際のブレーキ協調制御
//! (ブレーキベクタリング)・タイヤの非線形摩擦円(friction circle)・
//! 路面μの実測・車両の実際の運動方程式(6自由度)は一切考慮していない。
//! 出力される各輪トルク値は、実際の車両制御システムの設計・実装の根拠
//! には一切使えない教育的な参考値に過ぎない。
//!
//! # 参照した一般原理(公開情報、特定企業の企業秘密ではない)
//!
//! - トルクベクタリングの基本原理(要求ヨーモーメントを求め、左右輪の
//!   トルク差配分によって実現する)は、三菱のランサーエボリューションの
//!   AYC(1996年、公開特許・広報資料)以来、学術論文・工学教科書レベルで
//!   広く公開されている一般原理(独: "Torque Vectoring", 英語文献多数)。
//! - 2自由度平面(2-DOF planar bicycle model)による目標ヨーレート算出、
//!   左右輪トルク差によるヨーモーメント制御則も公開された学術論文
//!   (IEEE Xplore・ScienceDirect等)で広く扱われる一般的な制御理論。
//! - 本コードはこれらの**公開されている一般原理**を単純化した実装であり、
//!   特定企業(BMW xDrive等)の実際の制御ソフトウェア・適合パラメータ・
//!   特許アルゴリズムを調査・模倣したものではない。

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum NwdControlError {
    #[error("wheel count must be even and >= 2, got {0}")]
    InvalidWheelCount(u32),
    #[error("total requested torque must be non-negative, got {0}")]
    NegativeTorque(f64),
    #[error("track width must be positive, got {0}")]
    NonPositiveTrackWidth(f64),
}

/// 車輪位置(簡易モデル: 左右のみ区別。前後の軸は`axle_index`で表す)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelSide {
    Left,
    Right,
}

/// 1輪分のトルク配分結果。
#[derive(Debug, Clone, Copy)]
pub struct WheelTorque {
    pub axle_index: u32,
    pub side: WheelSide,
    /// 駆動トルク配分(N・m、正の値。ブレーキ側〈負トルク〉は本MVPでは
    /// 未対応、駆動トルクの左右差のみで表現する簡易モデル)。
    pub drive_torque_n_m: f64,
}

/// nWD駆動方式の入力パラメータ。
#[derive(Debug, Clone, Copy)]
pub struct NwdVehicle {
    /// 駆動輪の総数(2/4/6/8以上、偶数のみ対応: 左右対称配置を仮定)。
    pub wheel_count: u32,
    /// 輪距(トレッド幅、m)。
    pub track_width_m: f64,
    /// ホイールベース概算(m、軸間距離。ヨーモーメント配分の簡易計算に使用)。
    pub wheelbase_m: f64,
}

impl Default for NwdVehicle {
    fn default() -> Self {
        NwdVehicle {
            wheel_count: 4,
            track_width_m: 1.6,
            wheelbase_m: 2.7,
        }
    }
}

/// nWDトルク配分シミュレーション結果。
#[derive(Debug, Clone)]
pub struct NwdControlResult {
    pub wheel_torques: Vec<WheelTorque>,
    /// 実現された(近似的な)左右トルク差の合計(N・m、ヨーモーメント生成に寄与)。
    pub total_left_right_delta_n_m: f64,
}

/// 総要求トルク・要求ヨーモーメント(旋回時の左右差要求)から、各輪への
/// 簡易トルク配分を計算する。
///
/// 各軸へは総トルクを均等配分し、各軸内で左右輪へ`requested_yaw_moment_n_m`
/// に比例した差分(トルクベクタリング)を加減する、という単純化した
/// 最適配分の近似(実際にはタイヤの摩擦円制約・軸重配分等を考慮した
/// 最適化問題として解かれることが多いが、本MVPでは線形近似に留める)。
///
/// **正直な開示(総トルク非保存のケース)**: 各輪の駆動トルクは0未満に
/// ならないようクランプされる。`requested_yaw_moment_n_m`が大きすぎて
/// このクランプが発動した場合、返される`wheel_torques`の合計は
/// `total_drive_torque_n_m`と一致しなくなる(実際にはブレーキ
/// ベクタリングで補うべきだが本MVPでは未対応)。
pub fn simulate_torque_distribution(
    vehicle: &NwdVehicle,
    total_drive_torque_n_m: f64,
    requested_yaw_moment_n_m: f64,
) -> Result<NwdControlResult, NwdControlError> {
    if vehicle.wheel_count < 2 || !vehicle.wheel_count.is_multiple_of(2) {
        return Err(NwdControlError::InvalidWheelCount(vehicle.wheel_count));
    }
    if total_drive_torque_n_m < 0.0 {
        return Err(NwdControlError::NegativeTorque(total_drive_torque_n_m));
    }
    if vehicle.track_width_m <= 0.0 {
        return Err(NwdControlError::NonPositiveTrackWidth(
            vehicle.track_width_m,
        ));
    }

    let axle_count = vehicle.wheel_count / 2;
    let torque_per_axle = total_drive_torque_n_m / axle_count as f64;
    let torque_per_wheel_base = torque_per_axle / 2.0;

    // 要求ヨーモーメントを、各輪への左右差トルクへ変換する。
    // M_yaw ≈ delta_F * track_width  (delta_F: 左右輪の駆動力差、簡易近似)
    // delta_torque(1輪あたり) = delta_F * wheel_radius だが、本MVPでは
    // 「トルク差×輪距 ≈ ヨーモーメント」という単純化した比例関係で近似する
    // (実際には駆動力=トルク/タイヤ半径の換算が必要だが、代表的なタイヤ
    // 半径0.3mを仮定して比例定数に織り込む簡易モデルとする)。
    const ASSUMED_WHEEL_RADIUS_M: f64 = 0.3;
    let delta_torque_per_axle_n_m = if vehicle.track_width_m > 0.0 {
        requested_yaw_moment_n_m * ASSUMED_WHEEL_RADIUS_M / vehicle.track_width_m
            / axle_count as f64
    } else {
        0.0
    };

    let mut wheel_torques = Vec::with_capacity(vehicle.wheel_count as usize);
    let mut total_left_right_delta_n_m = 0.0;

    for axle_index in 0..axle_count {
        let left_torque = torque_per_wheel_base + delta_torque_per_axle_n_m / 2.0;
        let right_torque = torque_per_wheel_base - delta_torque_per_axle_n_m / 2.0;

        // 駆動トルクは負にならないようクランプ(実際には反対側の輪への
        // ブレーキベクタリングで負のヨーモーメント要求を満たすが、本MVPの
        // スコープ外として明記する)。**正直な開示**: このクランプが
        // 発動するほど大きい`requested_yaw_moment_n_m`を渡した場合、
        // 結果の左右輪トルクの合計は`total_drive_torque_n_m`と一致
        // しなくなる(クランプされた分がどこにも再配分されない)。
        // つまり本関数は総駆動トルクの保存を無条件には保証しない——
        // 呼び出し側は、必要に応じて`wheel_torques`の合計を検算すること。
        let left_torque = left_torque.max(0.0);
        let right_torque = right_torque.max(0.0);

        total_left_right_delta_n_m += (left_torque - right_torque).abs();

        wheel_torques.push(WheelTorque {
            axle_index,
            side: WheelSide::Left,
            drive_torque_n_m: left_torque,
        });
        wheel_torques.push(WheelTorque {
            axle_index,
            side: WheelSide::Right,
            drive_torque_n_m: right_torque,
        });
    }

    Ok(NwdControlResult {
        wheel_torques,
        total_left_right_delta_n_m,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_odd_wheel_count() {
        let v = NwdVehicle {
            wheel_count: 3,
            ..Default::default()
        };
        assert_eq!(
            simulate_torque_distribution(&v, 100.0, 0.0).unwrap_err(),
            NwdControlError::InvalidWheelCount(3)
        );
    }

    #[test]
    fn straight_line_zero_yaw_splits_torque_evenly() {
        let v = NwdVehicle::default(); // 4WD
        let result = simulate_torque_distribution(&v, 400.0, 0.0).unwrap();
        assert_eq!(result.wheel_torques.len(), 4);
        for wt in &result.wheel_torques {
            assert!(
                (wt.drive_torque_n_m - 100.0).abs() < 1e-9,
                "expected 100 N・m per wheel, got {}",
                wt.drive_torque_n_m
            );
        }
        assert!(result.total_left_right_delta_n_m.abs() < 1e-9);
    }

    #[test]
    fn nonzero_yaw_moment_creates_left_right_torque_difference() {
        let v = NwdVehicle::default();
        let result = simulate_torque_distribution(&v, 400.0, 500.0).unwrap();
        assert!(
            result.total_left_right_delta_n_m > 0.0,
            "expected nonzero left/right delta, got {}",
            result.total_left_right_delta_n_m
        );
    }

    #[test]
    fn more_wheels_yields_more_torque_entries() {
        let v4 = NwdVehicle {
            wheel_count: 4,
            ..Default::default()
        };
        let v8 = NwdVehicle {
            wheel_count: 8,
            ..Default::default()
        };
        let r4 = simulate_torque_distribution(&v4, 800.0, 0.0).unwrap();
        let r8 = simulate_torque_distribution(&v8, 800.0, 0.0).unwrap();
        assert_eq!(r4.wheel_torques.len(), 4);
        assert_eq!(r8.wheel_torques.len(), 8);
        // 総トルクは同じでも、車輪数が倍になれば1輪あたりのトルクは半分。
        let per_wheel_4 = r4.wheel_torques[0].drive_torque_n_m;
        let per_wheel_8 = r8.wheel_torques[0].drive_torque_n_m;
        assert!((per_wheel_4 - 2.0 * per_wheel_8).abs() < 1e-9);
    }

    #[test]
    fn moderate_yaw_request_conserves_total_torque() {
        // クランプが発動しない範囲では、総駆動トルクは保存される。
        let v = NwdVehicle::default();
        let total = 400.0;
        let result = simulate_torque_distribution(&v, total, 50.0).unwrap();
        let sum: f64 = result.wheel_torques.iter().map(|w| w.drive_torque_n_m).sum();
        assert!(
            (sum - total).abs() < 1e-6,
            "expected total torque {total} to be conserved, got {sum}"
        );
    }

    #[test]
    fn extreme_yaw_request_clamps_and_breaks_total_torque_conservation() {
        // ドキュメントに明記した通り、要求ヨーモーメントが大きすぎて
        // 片側輪のトルクが0未満になる場合はクランプされる。反対側の輪は
        // クランプされないため、合計はtotal_drive_torque_n_mから
        // 乖離する(本ケースでは反対側が増える方向に、非保存)ことを検証する。
        let v = NwdVehicle::default();
        let total = 400.0;
        let result = simulate_torque_distribution(&v, total, 1_000_000.0).unwrap();
        let sum: f64 = result.wheel_torques.iter().map(|w| w.drive_torque_n_m).sum();
        assert!(
            (sum - total).abs() > 1.0,
            "expected clamping to break total-torque conservation away from {total}, got {sum}"
        );
        // クランプされた輪は厳密に0であるはず。
        assert!(result.wheel_torques.iter().any(|w| w.drive_torque_n_m == 0.0));
    }

    #[test]
    fn rejects_negative_total_torque() {
        let v = NwdVehicle::default();
        assert_eq!(
            simulate_torque_distribution(&v, -1.0, 0.0).unwrap_err(),
            NwdControlError::NegativeTorque(-1.0)
        );
    }
}
