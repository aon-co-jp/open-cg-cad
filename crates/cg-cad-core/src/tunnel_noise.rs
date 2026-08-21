//! トンネル微気圧波(騒音)軽減のための列車先頭形状(ノーズ)デザイン
//! 計算シミュレーター(1次元圧縮性流れの簡易近似、MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: 本モジュール
//! が行うのは、本格的な数値流体力学(CFD、3次元Navier-Stokes方程式の
//! 解析)ではなく、**列車先頭形状の断面積変化率(`dA/dx`)の最大値を
//! 微気圧波強度の簡易指標として用いる1次元パラメトリック近似**である。
//! 実際の微気圧波は、トンネル内の圧縮波の伝播・トンネル出口での放射・
//! 空気の粘性・トンネル形状(断面積・線形)等、多数の要因に依存する
//! 3次元的な現象であり、本モジュールの指標はそれらを一切考慮していない。
//! 出力される「最適形状」は、実際のトンネル微気圧波測定・専門のCFD
//! 解析ソフトウェア・鉄道工学の有資格技術者によるレビューを経ない限り、
//! 実際の車両設計・法規適合性の根拠には一切使えない。
//!
//! # 工学的背景(簡易近似の妥当性の根拠)
//!
//! 新幹線等の高速鉄道において、先頭形状の**断面積変化率(勾配)**が
//! 微気圧波の強さに影響するという知見は実在する工学的知見であり
//! (先頭形状が長く・断面積がなめらかに変化するほど、トンネル突入時に
//! 発生する圧縮波の立ち上がりが緩やかになり、微気圧波が弱まる)、本
//! モジュールはこの定性的な関係を「`dA/dx`の最大値が小さいほど微気圧
//! 波指標が小さい」という簡易指標として定量化している。ただし、これは
//! 正確な音響解析・圧力波動方程式の解ではなく、工学的に妥当な近似に
//! 留まる。
//!
//! # 数値計算とLLMの役割分担・GPU加速(必読、他モジュールと同じ方針)
//!
//! - **数値計算(断面積分布の評価・グリッドサーチによる形状比較)は
//!   このモジュールのRustコードが行う**。複数の形状候補を同時に評価する
//!   際の行列演算(候補ごとの断面積プロファイル行列と離散微分演算子の
//!   積)に`opencuda_blas::sgemm`(既存GEMM実装)をpath依存で再利用する。
//! - **LLM(aruaru-llm)は計算を一切行わない**——計算済みの指標値を
//!   受け取り、自然言語の説明文へ変換する役割のみを担う。
//!
//! # 簡略化している点(正直な開示)
//!
//! - 断面積分布`A(x)`を滑らかな冪関数(`nose_profile_area_m2`)で
//!   パラメトリック表現しており、実際の複雑な3次元形状(非対称・
//!   凹凸のある形状)は表現できない。
//! - 微気圧波強度指標は`max(dA/dx)`という単純な代理指標であり、
//!   実際の圧力波の伝播・減衰・トンネル出口での放射特性を考慮した
//!   正確な音響解析値ではない。
//! - グリッドサーチによる最適化は、パラメータ空間を離散的に総当たり
//!   評価する最も単純な手法であり、本格的な勾配法・遺伝的アルゴリズム
//!   等の高度な最適化は用いていない。

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum TunnelNoiseError {
    #[error("nose_length_m must be positive, got {0}")]
    NonPositiveLength(f64),
    #[error("max_cross_section_m2 must be positive, got {0}")]
    NonPositiveCrossSection(f64),
    #[error("num_samples must be at least 2, got {0}")]
    TooFewSamples(usize),
}

/// 列車先頭形状(ノーズ)の断面積分布パラメータ。
///
/// 断面積分布は`A(x) = max_cross_section_m2 * (x / nose_length_m)^power`
/// (`x`: 先端からの距離、`0 <= x <= nose_length_m`)という冪関数で近似する。
/// `power`が大きいほど、先端付近で断面積が緩やかに増加し(尖った形状)、
/// `power`が小さい(1に近い)ほど直線的に増加する。
#[derive(Debug, Clone, Copy)]
pub struct NoseShape {
    pub nose_length_m: f64,
    pub max_cross_section_m2: f64,
    /// 断面積変化の冪指数(既定的に1.0〜4.0程度を想定、大きいほど先端が
    /// 鋭く・後方で急激に断面積が増える形状)。
    pub power: f64,
}

impl NoseShape {
    fn validate(&self) -> Result<(), TunnelNoiseError> {
        if self.nose_length_m <= 0.0 {
            return Err(TunnelNoiseError::NonPositiveLength(self.nose_length_m));
        }
        if self.max_cross_section_m2 <= 0.0 {
            return Err(TunnelNoiseError::NonPositiveCrossSection(self.max_cross_section_m2));
        }
        Ok(())
    }

    /// 先端からの距離`x`(m)における断面積(m^2)。
    pub fn area_at(&self, x: f64) -> f64 {
        let t = (x / self.nose_length_m).clamp(0.0, 1.0);
        self.max_cross_section_m2 * t.powf(self.power)
    }
}

/// `num_samples`点で離散化した断面積分布(先端0から`nose_length_m`まで
/// 等間隔)。
fn discretize_area_profile(shape: &NoseShape, num_samples: usize) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let x = shape.nose_length_m * (i as f64) / ((num_samples - 1) as f64);
            shape.area_at(x) as f32
        })
        .collect()
}

/// 離散微分演算子(前進差分、`(num_samples-1) x num_samples`行列、
/// `structural::assemble_global_stiffness`と同様に行優先で組み立てる)。
/// `D * A`が各区間の`dA/dx`近似値ベクトルになる。
fn forward_difference_operator(num_samples: usize, dx: f64) -> Vec<f32> {
    let rows = num_samples - 1;
    let mut d = vec![0.0f32; rows * num_samples];
    let inv_dx = (1.0 / dx) as f32;
    for i in 0..rows {
        d[i * num_samples + i] = -inv_dx;
        d[i * num_samples + (i + 1)] = inv_dx;
    }
    d
}

/// 微気圧波強度の簡易指標(`max(dA/dx)`)を、`opencuda_blas::sgemm`
/// (既存の実機検証済みGEMM実装)による離散微分演算子との行列-ベクトル積
/// として計算する。
pub fn micro_pressure_wave_index(shape: &NoseShape, num_samples: usize) -> Result<f64, TunnelNoiseError> {
    shape.validate()?;
    if num_samples < 2 {
        return Err(TunnelNoiseError::TooFewSamples(num_samples));
    }
    let dx = shape.nose_length_m / (num_samples - 1) as f64;
    let area_profile = discretize_area_profile(shape, num_samples);
    let d_op = forward_difference_operator(num_samples, dx);

    let rows = num_samples - 1;
    let device = opencuda_cpu::CpuDevice::new(0);
    let mut d_area = vec![0.0f32; rows];
    // (rows x num_samples) * (num_samples x 1) の行列-ベクトル積。
    opencuda_blas::sgemm(&*device, rows, num_samples, 1, 1.0, &d_op, &area_profile, 0.0, &mut d_area, None)
        .expect("opencuda-blas CpuNaive sgemm must not fail for well-formed inputs");

    let max_grad = d_area.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    Ok(max_grad as f64)
}

/// 形状候補1件分の評価結果。
#[derive(Debug, Clone, PartialEq)]
pub struct NoseShapeCandidateResult {
    pub shape_power: f64,
    pub micro_pressure_wave_index: f64,
}

/// グリッドサーチによる最適化結果。
#[derive(Debug, Clone)]
pub struct NoseOptimizationResult {
    pub candidates: Vec<NoseShapeCandidateResult>,
    /// 指標が最小になった(=微気圧波が最も小さいと推定される)候補。
    pub best: NoseShapeCandidateResult,
    pub disclaimer: &'static str,
}

const DISCLAIMER: &str = "この結果は、先頭形状の断面積変化率(dA/dx)の最大値を微気圧波強度の簡易指標として用いた\
1次元パラメトリック近似のドラフトであり、専門の数値流体力学(CFD)ソフトウェアによる3次元解析の代替ではない。\
実際のトンネル微気圧波測定・鉄道工学の有資格技術者によるレビューを経ない限り、実際の車両設計・法規適合性の\
根拠には使えない。";

/// `power`パラメータを`power_candidates`で総当たり評価し、微気圧波指標が
/// 最小になる先頭形状を提案する簡易グリッドサーチ最適化。
pub fn optimize_nose_shape(
    nose_length_m: f64,
    max_cross_section_m2: f64,
    power_candidates: &[f64],
    num_samples: usize,
) -> Result<NoseOptimizationResult, TunnelNoiseError> {
    if power_candidates.is_empty() {
        // 空候補は「探索範囲なし」に相当するため、明示的なエラーとして
        // TooFewSamplesの意味を借用せず専用エラーを設けるほどでもないため
        // ここでは1点も評価できない扱いとしてTooFewSamplesを転用しない
        // ——代わりに最小限のバリデーションとしてnose_length_m検証のみ通す。
    }
    let mut candidates = Vec::with_capacity(power_candidates.len());
    for &power in power_candidates {
        let shape = NoseShape { nose_length_m, max_cross_section_m2, power };
        let index = micro_pressure_wave_index(&shape, num_samples)?;
        candidates.push(NoseShapeCandidateResult { shape_power: power, micro_pressure_wave_index: index });
    }

    let best = candidates
        .iter()
        .cloned()
        .min_by(|a, b| a.micro_pressure_wave_index.partial_cmp(&b.micro_pressure_wave_index).expect("indices are finite f64"))
        .ok_or(TunnelNoiseError::TooFewSamples(0))?; // power_candidatesが空の場合のみここに到達

    Ok(NoseOptimizationResult { candidates, best, disclaimer: DISCLAIMER })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 既知の解析解との比較: `A(x) = A_max * (x/L)^power`の解析的な導関数は
    /// `dA/dx = A_max * power / L * (x/L)^(power-1)`であり、`power >= 1`の
    /// 場合`x=L`(後端)で最大値`A_max * power / L`を取る。この解析解と
    /// GEMM経由の離散微分の結果を比較する。
    #[test]
    fn linear_profile_matches_analytical_gradient() {
        let shape = NoseShape { nose_length_m: 10.0, max_cross_section_m2: 8.0, power: 1.0 };
        // power=1(線形)の場合、dA/dx は定数 = A_max / L
        let expected = shape.max_cross_section_m2 / shape.nose_length_m;
        let got = micro_pressure_wave_index(&shape, 500).unwrap();
        assert!((got - expected).abs() / expected < 0.01, "got={got}, expected={expected}");
    }

    #[test]
    fn quadratic_profile_matches_analytical_max_gradient_at_tail() {
        let shape = NoseShape { nose_length_m: 10.0, max_cross_section_m2: 8.0, power: 2.0 };
        // dA/dx = A_max * 2 / L * (x/L)、x=Lで最大値 = 2*A_max/L
        let expected = 2.0 * shape.max_cross_section_m2 / shape.nose_length_m;
        let got = micro_pressure_wave_index(&shape, 2000).unwrap();
        assert!((got - expected).abs() / expected < 0.02, "got={got}, expected={expected}");
    }

    #[test]
    fn higher_power_yields_larger_micro_pressure_wave_index_for_same_dimensions() {
        // powerが大きいほど後端付近での立ち上がりが急峻になり、
        // dA/dxの最大値(=指標)も大きくなるはず(工学的に妥当な単調性)。
        let low = NoseShape { nose_length_m: 10.0, max_cross_section_m2: 8.0, power: 1.0 };
        let high = NoseShape { nose_length_m: 10.0, max_cross_section_m2: 8.0, power: 3.0 };
        let low_index = micro_pressure_wave_index(&low, 1000).unwrap();
        let high_index = micro_pressure_wave_index(&high, 1000).unwrap();
        assert!(high_index > low_index, "high={high_index}, low={low_index}");
    }

    #[test]
    fn longer_nose_reduces_micro_pressure_wave_index_for_same_cross_section() {
        // 同じ断面積・同じpowerでも、先頭形状が長いほど断面積変化が
        // 緩やかになり指標が小さくなるはず(工学的に妥当な単調性、
        // 「先頭形状が長いほど微気圧波が弱まる」という実在する知見に対応)。
        let short = NoseShape { nose_length_m: 5.0, max_cross_section_m2: 8.0, power: 2.0 };
        let long = NoseShape { nose_length_m: 15.0, max_cross_section_m2: 8.0, power: 2.0 };
        let short_index = micro_pressure_wave_index(&short, 1000).unwrap();
        let long_index = micro_pressure_wave_index(&long, 1000).unwrap();
        assert!(long_index < short_index, "long={long_index}, short={short_index}");
    }

    #[test]
    fn optimize_nose_shape_picks_the_lowest_index_candidate() {
        let result = optimize_nose_shape(10.0, 8.0, &[1.0, 1.5, 2.0, 2.5, 3.0], 500).unwrap();
        assert_eq!(result.candidates.len(), 5);
        // power=1.0(最も緩やかな線形変化)が最小指標になるはず。
        assert_eq!(result.best.shape_power, 1.0);
        let min_among_candidates =
            result.candidates.iter().map(|c| c.micro_pressure_wave_index).fold(f64::MAX, f64::min);
        assert_eq!(result.best.micro_pressure_wave_index, min_among_candidates);
    }

    #[test]
    fn rejects_non_positive_length() {
        let shape = NoseShape { nose_length_m: 0.0, max_cross_section_m2: 8.0, power: 2.0 };
        let err = micro_pressure_wave_index(&shape, 100).unwrap_err();
        assert_eq!(err, TunnelNoiseError::NonPositiveLength(0.0));
    }

    #[test]
    fn rejects_too_few_samples() {
        let shape = NoseShape { nose_length_m: 10.0, max_cross_section_m2: 8.0, power: 2.0 };
        let err = micro_pressure_wave_index(&shape, 1).unwrap_err();
        assert_eq!(err, TunnelNoiseError::TooFewSamples(1));
    }
}
