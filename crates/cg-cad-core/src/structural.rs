//! 構造計算・耐震シミュレーション(簡易骨組みモデル+線形静解析、MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: このモジュール
//! が行うのは、実務の構造計算ソフトウェア(構造計算適合性判定を受けられる
//! 専用プログラム)が行うような本格的な有限要素法(FEM)ではなく、
//! **各階を1つの水平方向自由度に集約した「せん断建物(shear building)」
//! 簡易骨組みモデル+線形弾性静解析**である。P-Δ効果・鉛直方向の振動・
//! 非線形挙動(部材の降伏・座屈)・時刻歴応答解析は一切行っていない。
//! 出力される層間変形角・判定結果は、専門の構造計算ソフトウェア・
//! 一級建築士等の有資格者によるレビューを経ない限り、建築確認申請等の
//! 法的な根拠には一切使えない(`lib.rs`冒頭の全体免責事項と同じ)。
//!
//! # 数値計算とLLMの役割分担(必読)
//!
//! - **数値計算(剛性マトリックス法によるKx=Fの求解)はこのモジュールの
//!   Rustコードが行う**。`opencuda_blas::sgemm`(`open-cuda`の既存GEMM
//!   実装、CPU/Vulkan両対応、実機検証済み)へpath依存し、行列-ベクトル積
//!   を伴うヤコビ反復法で連立一次方程式を解く。
//! - **LLM(aruaru-llm)は計算を一切行わない**——計算済みの数値結果
//!   (層間変形角・判定結果)を受け取り、それを自然言語の説明文へ変換する
//!   役割のみを担う(`crate::llm_explain`参照)。この区別を意図的に
//!   明確にしている。LLMが「計算した」かのような誤解を招く設計・文言は
//!   避けること。
//!
//! # 簡略化している点(正直な開示)
//!
//! - 本格的なFEM(3次元ソリッド要素・シェル要素等)ではなく、各階を1
//!   自由度(水平変位のみ)に集約したせん断建物モデル。柱の曲げ剛性を
//!   `12EI/h^3`(両端固定はり)として単純化し、床の面内剛性は無限大
//!   (剛床仮定)とみなす。
//! - 断面(柱の寸法)・ヤング率は、建物種別によらない代表値の定数として
//!   扱う(実際の構造設計では部材ごとに個別に決定される)。
//! - 地震荷重は「震度に応じた水平力」を各階重量に対する一定の係数
//!   (簡易ベースシア係数)として近似しており、Ai分布・応答スペクトル法
//!   等の precise な手法は用いていない。
//! - 層間変形角の目安値(1/200)は、建築基準法施行令第82条の2
//!   (層間変形角の確認)が定める一般的な目安であり、実際の許容値は
//!   建物用途・構造種別等により異なりうる。本コードはこの数値を
//!   ハードコードした目安判定に過ぎず、法的な確認ではない。

use crate::building::Building;
use thiserror::Error;

/// 材料・断面パラメータ(簡易骨組みモデル用の代表値)。
///
/// **正直な開示**: 実際の構造設計では部材ごとに個別の断面・材料が
/// 決定されるが、本MVPでは建物全体で一律の代表値を使う。
#[derive(Debug, Clone, Copy)]
pub struct StructuralMaterial {
    /// ヤング率(Pa)。既定値は鉄筋コンクリートの代表値(約2.1e10 Pa)。
    pub youngs_modulus_pa: f64,
    /// 柱の正方形断面の一辺(m)。既定値0.5m(RC造の中規模建物の代表値)。
    pub column_side_m: f64,
    /// 各階の柱本数(簡易的に矩形平面の四隅+中間柱を想定した代表値)。
    pub columns_per_floor: u32,
    /// 単位床面積あたりの重量(kN/m^2、固定荷重+積載荷重の簡易合算値)。
    pub unit_weight_kn_per_m2: f64,
}

impl Default for StructuralMaterial {
    fn default() -> Self {
        StructuralMaterial {
            youngs_modulus_pa: 2.1e10,
            column_side_m: 0.5,
            columns_per_floor: 4,
            unit_weight_kn_per_m2: 10.0,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum StructuralError {
    #[error("building must have at least 1 floor, got {0}")]
    NoFloors(u32),
    #[error("linear solve did not converge within {0} iterations (residual too large)")]
    SolveDidNotConverge(usize),
}

/// 1階分の柱の水平剛性を求める(両端固定はりの曲げ剛性、`12EI/h^3`)。
fn column_stiffness_n_per_m(mat: &StructuralMaterial, floor_height_m: f64) -> f64 {
    // 正方形断面の断面二次モーメント: I = a^4 / 12
    let a = mat.column_side_m;
    let i = a.powi(4) / 12.0;
    12.0 * mat.youngs_modulus_pa * i / floor_height_m.powi(3)
}

/// せん断建物モデルの階剛性(その階の全柱の水平剛性の合計、並列ばね)。
fn story_stiffness_n_per_m(mat: &StructuralMaterial, floor_height_m: f64) -> f64 {
    column_stiffness_n_per_m(mat, floor_height_m) * mat.columns_per_floor as f64
}

/// 各階の重量(kN)。自重(固定荷重+積載荷重の簡易合算値)のみを考慮する。
fn story_weights_kn(building: &Building, mat: &StructuralMaterial) -> Vec<f64> {
    (0..building.floors).map(|_| building.footprint_m2 * mat.unit_weight_kn_per_m2).collect()
}

/// 震度に応じた水平地震力(kN)を各階に近似的に配分する。
///
/// **簡易近似**: 精密なAi分布(上層ほど大きい係数)ではなく、
/// `seismic_coefficient`(地震層せん断力係数の簡易近似、震度に応じて
/// 呼び出し側が指定する)を各階重量に一律に乗じる最も単純な近似である。
/// 例えば震度6強相当を模擬する場合は`seismic_coefficient`に概ね
/// 0.3〜0.4程度の値を与えることを想定するが、この対応関係自体は
/// 精密な地震動特性評価に基づくものではなく目安に過ぎない。
fn seismic_forces_kn(weights_kn: &[f64], seismic_coefficient: f64) -> Vec<f64> {
    weights_kn.iter().map(|w| w * seismic_coefficient).collect()
}

/// せん断建物モデルの全体剛性マトリックス(n x n、行優先、対称三重対角)。
/// `stiffness[i]`はi階(0始まり、0が最下階)の階剛性。
fn assemble_global_stiffness(stiffness: &[f64]) -> Vec<f64> {
    let n = stiffness.len();
    let mut k = vec![0.0f64; n * n];
    for i in 0..n {
        let k_i = stiffness[i];
        let k_ip1 = if i + 1 < n { stiffness[i + 1] } else { 0.0 };
        k[i * n + i] = k_i + k_ip1;
        if i + 1 < n {
            k[i * n + (i + 1)] = -k_ip1;
            k[(i + 1) * n + i] = -k_ip1;
        }
    }
    k
}

/// `Kx=F`をヤコビ反復法で解く。行列-ベクトル積(`K*x_k`)を
/// `opencuda_blas::sgemm`(既存の実機検証済みGEMM実装、CPU経路)へ
/// path依存させて計算する——大規模な全体剛性行列の行列演算部分を、
/// 手書きの三重ループではなく既存資産のGEMM呼び出しとして実行する
/// という設計方針(タスク指示通り)。
///
/// 三重対角行列のヤコビ法は対角優位であれば収束することが知られており、
/// せん断建物モデルの`K`(各対角成分が非対角成分の和以上)はこの条件を
/// 満たすため、この用途には妥当な反復法である。
fn solve_kx_f_via_gemm(k: &[f64], f: &[f64], max_iterations: usize, tolerance: f32) -> Result<Vec<f64>, StructuralError> {
    let n = f.len();
    // opencuda_blas::sgemm はf32を扱うAPIのため、ここでf64→f32へ変換する。
    // 構造計算の精度としてはf32でも通常十分(既存のopencuda-blas資産を
    // そのまま再利用する設計上のトレードオフとして正直に明記)。
    let k_f32: Vec<f32> = k.iter().map(|&v| v as f32).collect();
    let f_f32: Vec<f32> = f.iter().map(|&v| v as f32).collect();

    let device = opencuda_cpu::CpuDevice::new(0);

    let mut x = vec![0.0f32; n];
    for iteration in 0..max_iterations {
        // kx = K * x_k (m=n, k=n, n=1 の行列-ベクトル積としてsgemmを呼ぶ)。
        let mut kx = vec![0.0f32; n];
        opencuda_blas::sgemm(&*device, n, n, 1, 1.0, &k_f32, &x, 0.0, &mut kx, None)
            .expect("opencuda-blas CpuNaive sgemm must not fail for well-formed inputs");

        let mut x_next = vec![0.0f32; n];
        let mut max_residual = 0.0f32;
        for i in 0..n {
            let k_ii = k_f32[i * n + i];
            debug_assert!(k_ii > 0.0, "diagonal stiffness must be positive");
            // x_{k+1,i} = x_{k,i} + (F_i - (K x_k)_i) / K_ii
            let residual = f_f32[i] - kx[i];
            x_next[i] = x[i] + residual / k_ii;
            max_residual = max_residual.max(residual.abs());
        }
        x = x_next;

        if max_residual < tolerance {
            return Ok(x.iter().map(|&v| v as f64).collect());
        }
        let _ = iteration;
    }

    Err(StructuralError::SolveDidNotConverge(max_iterations))
}

/// 層間変形角の判定結果(1階分)。
#[derive(Debug, Clone, PartialEq)]
pub struct StoryDriftResult {
    /// 階番号(1始まり、1階が最下階)。
    pub story: u32,
    /// 水平変位(m、絶対値、地面基準)。
    pub displacement_m: f64,
    /// 層間変形角(この階とその下の階との相対水平変位/階高)。
    pub drift_angle: f64,
    /// 目安値(既定1/200)を満たすか。
    pub within_limit: bool,
}

/// 構造計算・耐震シミュレーションの結果。
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralAnalysisResult {
    pub story_drifts: Vec<StoryDriftResult>,
    /// 全階が目安値を満たすか。
    pub all_within_limit: bool,
    /// 判定に使った目安値(既定1/200)。
    pub drift_limit: f64,
    /// 出典(建築基準法施行令第82条の2、層間変形角の確認)。
    pub disclaimer: &'static str,
}

/// 建築基準法施行令第82条の2が定める層間変形角の一般的な目安値。
/// **正直な開示**: 実際の許容値は建物用途・構造種別等により異なりうる
/// ため、この定数を法的な確定値として扱わないこと。
pub const DEFAULT_DRIFT_LIMIT: f64 = 1.0 / 200.0;

const DISCLAIMER: &str = "この結果は簡易的な線形静解析(せん断建物モデル)によるドラフトであり、\
専門の構造計算ソフトウェア(構造計算適合性判定を受けられる認定プログラム)の代替ではない。\
建築確認申請等に使える法的な構造計算書ではなく、必ず一級建築士等の有資格者によるレビューが必要。\
層間変形角の目安値(1/200)は建築基準法施行令第82条の2に基づく一般的な目安であり、\
実際の許容値は建物用途・構造種別等により異なりうる。";

/// 建物に対して簡易構造計算・耐震シミュレーションを実行する。
///
/// `seismic_coefficient`: 地震層せん断力係数の簡易近似(震度に応じて
/// 呼び出し側が指定、詳細は[`seismic_forces_kn`]のdocコメント参照)。
/// `material`: `None`の場合は[`StructuralMaterial::default`]を使う。
pub fn analyze_structure(
    building: &Building,
    seismic_coefficient: f64,
    material: Option<StructuralMaterial>,
) -> Result<StructuralAnalysisResult, StructuralError> {
    if building.floors == 0 {
        return Err(StructuralError::NoFloors(building.floors));
    }
    let mat = material.unwrap_or_default();
    let floor_height_m = building.building_type.floor_height_m();
    let n = building.floors as usize;

    let story_stiffness = vec![story_stiffness_n_per_m(&mat, floor_height_m); n];
    let weights_kn = story_weights_kn(building, &mat);
    let forces_kn = seismic_forces_kn(&weights_kn, seismic_coefficient);
    // kN -> N (剛性はN/mで計算しているため単位を揃える)。
    let forces_n: Vec<f64> = forces_kn.iter().map(|&f| f * 1000.0).collect();

    let k = assemble_global_stiffness(&story_stiffness);
    // 収束判定の許容誤差は、荷重ベクトルの最大絶対値に対する相対誤差
    // (1e-5)として与える——荷重の単位(N)は建物規模により桁が大きく
    // 変わるため、絶対誤差の固定値では小規模建物で緩すぎ・大規模建物で
    // 厳しすぎる(収束しない)問題が生じるため。
    let max_force = forces_n.iter().fold(0.0f64, |m, &f| m.max(f.abs())).max(1.0);
    let tolerance = (max_force * 1e-5) as f32;
    let displacements_m = solve_kx_f_via_gemm(&k, &forces_n, 200_000, tolerance)?;

    let mut story_drifts = Vec::with_capacity(n);
    let mut prev_disp = 0.0f64;
    let mut all_within_limit = true;
    for (idx, &disp) in displacements_m.iter().enumerate() {
        let drift_angle = (disp - prev_disp).abs() / floor_height_m;
        let within_limit = drift_angle <= DEFAULT_DRIFT_LIMIT;
        all_within_limit &= within_limit;
        story_drifts.push(StoryDriftResult { story: idx as u32 + 1, displacement_m: disp, drift_angle, within_limit });
        prev_disp = disp;
    }

    Ok(StructuralAnalysisResult { story_drifts, all_within_limit, drift_limit: DEFAULT_DRIFT_LIMIT, disclaimer: DISCLAIMER })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{generate_building, BuildingType};
    use crate::site::Site;
    use crate::zoning::ZoningRules;

    /// 既知の解析解との比較: 単一階(n=1)のせん断建物モデルは、単純な
    /// 1自由度ばね-荷重系に帰着する(`x = F / K`という手計算で検証できる
    /// 最も基本的なケース)。
    #[test]
    fn single_story_matches_hand_calculated_spring_deflection() {
        let mat = StructuralMaterial::default();
        let floor_height_m = 3.0;
        let k = story_stiffness_n_per_m(&mat, floor_height_m);

        let site = Site::rectangle(10.0, 10.0).unwrap();
        let zoning = ZoningRules::new(0.6, 2.0).unwrap();
        let building = generate_building(&site, &zoning, BuildingType::House, 1).unwrap();

        let seismic_coefficient = 0.3;
        let weight_kn = building.footprint_m2 * mat.unit_weight_kn_per_m2;
        let force_n = weight_kn * seismic_coefficient * 1000.0;
        let expected_disp_m = force_n / k;

        let result = analyze_structure(&building, seismic_coefficient, None).unwrap();
        assert_eq!(result.story_drifts.len(), 1);
        let got = result.story_drifts[0].displacement_m;
        // 手計算値との相対誤差が1%以内であることを検証(ヤコビ反復法の
        // 収束誤差込みでも十分小さいはず)。
        assert!(
            (got - expected_disp_m).abs() / expected_disp_m.abs() < 0.01,
            "got={got}, expected={expected_disp_m}"
        );
    }

    /// 既知の解析解との比較(2階建て): 2自由度のせん断建物モデルは、
    /// `K = [[k1+k2, -k2], [-k2, k2]]`という2x2連立一次方程式に帰着する。
    /// この小さい系はCramerの公式で手計算できるため、GEMM経由のヤコビ
    /// 反復解とその手計算解を突き合わせて検証する。
    #[test]
    fn two_story_matches_hand_calculated_2x2_linear_system() {
        let mat = StructuralMaterial::default();
        let floor_height_m = 3.0;
        let k1 = story_stiffness_n_per_m(&mat, floor_height_m);
        let k2 = k1; // 同一断面のため各階の剛性は同じ

        let site = Site::rectangle(10.0, 10.0).unwrap();
        let zoning = ZoningRules::new(0.6, 3.0).unwrap();
        let building = generate_building(&site, &zoning, BuildingType::House, 2).unwrap();

        let seismic_coefficient = 0.3;
        let weight_kn = building.footprint_m2 * mat.unit_weight_kn_per_m2;
        let f1 = weight_kn * seismic_coefficient * 1000.0;
        let f2 = f1;

        // K = [[k1+k2, -k2], [-k2, k2]], F = [f1, f2]
        // 2x2の連立方程式をCramerの公式で解く。
        let a11 = k1 + k2;
        let a12 = -k2;
        let a21 = -k2;
        let a22 = k2;
        let det = a11 * a22 - a12 * a21;
        let x1_expected = (f1 * a22 - a12 * f2) / det;
        let x2_expected = (a11 * f2 - f1 * a21) / det;

        let result = analyze_structure(&building, seismic_coefficient, None).unwrap();
        assert_eq!(result.story_drifts.len(), 2);
        let x1 = result.story_drifts[0].displacement_m;
        let x2 = result.story_drifts[1].displacement_m;

        assert!((x1 - x1_expected).abs() / x1_expected.abs() < 0.01, "x1 got={x1}, expected={x1_expected}");
        assert!((x2 - x2_expected).abs() / x2_expected.abs() < 0.01, "x2 got={x2}, expected={x2_expected}");
    }

    #[test]
    fn rejects_zero_floors() {
        let mat = StructuralMaterial::default();
        let _ = mat;
        let site = Site::rectangle(10.0, 10.0).unwrap();
        let zoning = ZoningRules::new(0.6, 2.0).unwrap();
        // floors=0は generate_building 側で既に拒否されるため、ここでは
        // Building を手動構築せず generate_building のエラーを確認するに
        // 留める(analyze_structure単体でのfloors=0検証は、floors=0の
        // Buildingがそもそも生成できないため到達不能パスに近いが、
        // 念のため analyze_structure 自体の防御ロジックも直接検証する)。
        let building = generate_building(&site, &zoning, BuildingType::House, 1).unwrap();
        let mut zero_floor_building = building;
        zero_floor_building.floors = 0;
        let result = analyze_structure(&zero_floor_building, 0.3, None);
        assert_eq!(result.err(), Some(StructuralError::NoFloors(0)));
    }

    #[test]
    fn taller_buildings_have_larger_top_story_displacement_than_shorter_ones() {
        let site = Site::rectangle(20.0, 20.0).unwrap();
        let zoning = ZoningRules::new(0.5, 10.0).unwrap();
        let short = generate_building(&site, &zoning, BuildingType::House, 2).unwrap();
        let tall = generate_building(&site, &zoning, BuildingType::House, 5).unwrap();

        let short_result = analyze_structure(&short, 0.3, None).unwrap();
        let tall_result = analyze_structure(&tall, 0.3, None).unwrap();

        let short_top = short_result.story_drifts.last().unwrap().displacement_m;
        let tall_top = tall_result.story_drifts.last().unwrap().displacement_m;
        assert!(tall_top > short_top, "tall={tall_top}, short={short_top}");
    }

    #[test]
    fn zero_seismic_coefficient_yields_zero_displacement() {
        let site = Site::rectangle(10.0, 10.0).unwrap();
        let zoning = ZoningRules::new(0.6, 2.0).unwrap();
        let building = generate_building(&site, &zoning, BuildingType::House, 2).unwrap();
        let result = analyze_structure(&building, 0.0, None).unwrap();
        for drift in &result.story_drifts {
            assert!(drift.displacement_m.abs() < 1e-6, "expected ~0, got {}", drift.displacement_m);
            assert!(drift.within_limit);
        }
        assert!(result.all_within_limit);
    }
}
