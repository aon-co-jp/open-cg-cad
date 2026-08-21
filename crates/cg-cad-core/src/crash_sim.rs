//! 自動車・バイク・リニアモーターカーの衝突実験シミュレーション(簡易質点+
//! ばね+ダンパーモデル、MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: 本モジュール
//! が行うのは、実務の衝突安全性評価で使われる本格的な非線形有限要素法
//! (LS-DYNA・PAM-CRASH等の専門衝突解析ソフトウェア相当)ではなく、
//! **車体を複数の質点+ばね+ダンパー要素で近似した1次元多質点モデル
//! (mass-spring-damper system)による運動方程式の数値積分**である。
//! 3次元的な変形・座屈・部材の破断・複合材料の非線形挙動・乗員の
//! 詳細な人体モデル(ダミー人形挙動)は一切考慮していない。出力される
//! 最大加速度(G値)・変形量は、専門の衝突解析ソフトウェア・有資格の
//! 衝突安全性評価技術者によるレビューを経ない限り、実際の車両安全性
//! 認証・法規適合性の根拠には一切使えない。
//!
//! # 数値計算とLLMの役割分担(必読、`structural`モジュールと同じ方針)
//!
//! - **数値計算(運動方程式`M x'' + C x' + K x = 0`の数値積分)は
//!   このモジュールのRustコードが行う**。剛性・減衰行列を用いた
//!   加速度計算に`opencuda_blas::sgemm`(`open-cuda`の既存GEMM実装、
//!   実機検証済み)をpath依存で再利用する。
//! - **LLM(aruaru-llm)は計算を一切行わない**——計算済みの数値結果
//!   (最大加速度・変形量)を受け取り、自然言語の説明文へ変換する役割
//!   のみを担う(`crate::llm_explain`参照)。
//!
//! # 簡略化している点(正直な開示)
//!
//! - 3次元ではなく1次元(衝突方向のみ)の多質点モデル。車体の圧壊特性は
//!   区間ごとに一定のばね定数・減衰係数として近似しており、実際の
//!   材料非線形性(塑性変形・破断)は表現していない。
//! - 陽解法の中心差分法(explicit central difference method)による
//!   時間積分であり、ステップ幅が粗いと数値的に不安定になりうる
//!   (`stable_time_step_s`で理論的な安定条件の目安を提供する)。
//! - 乗員保護の目安値(参考値としての最大加速度)は一般的に引用される
//!   目安(例: 頭部傷害基準等で言及される数十G程度のオーダー)に過ぎず、
//!   本コードは断定的な法規適合判定を一切行わない。
//! - リニアモーターカー・バイクも同一の質点+ばねモデルの延長として
//!   扱っており、車両ごとに異なる構造特性(車体規模・軌道拘束等)の
//!   詳細な違いは反映していない。
//! - [`EnvironmentConditions`]/`apply_environment_conditions`による
//!   「強風の日」「雨の日のスリップ」条件の統合は、`crosswind`/
//!   `tire_wet_grip`モジュールが計算済みの参考指標(転倒安全率・
//!   ウェットグリップ指標)を、衝突条件(初速・剛性)への単純な倍率へ
//!   変換するだけの橋渡しであり、タイヤの摩擦円制約・車体スリップ角の
//!   時々刻々の連成計算を行う本格的な車両運動力学ではない
//!   (詳細は`EnvironmentConditions`のdocコメント参照)。

use crate::crosswind::CrosswindResult;
use crate::tire_wet_grip::WetGripResult;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum CrashSimError {
    #[error("must have at least 2 masses to form a collision chain, got {0}")]
    TooFewMasses(usize),
    #[error("mass at index {0} must be positive, got {1}")]
    NonPositiveMass(usize, f64),
    #[error("time_step_s must be positive, got {0}")]
    NonPositiveTimeStep(f64),
    #[error("num_steps must be at least 1, got {0}")]
    NoSteps(usize),
}

/// 衝突する車両の種別(参考ラベルとしてのみ使用、モデル自体は共通)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleKind {
    Car,
    Motorcycle,
    Maglev,
    Bus,
    Truck,
    Train,
}

impl VehicleKind {
    /// 参考として引用する乗員保護の目安値(G、単位はg=9.80665m/s^2)。
    /// **正直な開示**: 断定的な法規適合基準ではなく、一般的に引用される
    /// 目安のオーダーに過ぎない。Bus/Truck/Trainは車体規模・乗員の
    /// 拘束方式(座席配置・シートベルト有無等)の違いから、Car/Motorcycle
    /// と同様に既存の一般的なオーダー感を参考に設定した目安値であり、
    /// 特定の法規基準(道路運送車両の保安基準等)を直接引用したもの
    /// ではない。
    pub fn reference_survivable_g(&self) -> f64 {
        match self {
            VehicleKind::Car => 60.0,
            VehicleKind::Motorcycle => 50.0,
            VehicleKind::Maglev => 30.0,
            VehicleKind::Bus => 40.0,
            VehicleKind::Truck => 35.0,
            VehicleKind::Train => 25.0,
        }
    }
}

/// 質点+ばね+ダンパーチェーンのパラメータ。
///
/// `n`個の質点が1次元上に並び、隣接する質点間をばね(`spring_constants_n_per_m`、
/// 長さ`n-1`、圧壊特性の簡易近似)とダンパー(`damping_ns_per_m`、長さ`n-1`)で
/// 接続する。質点0は衝突方向の最前部(先頭)を表す。
#[derive(Debug, Clone)]
pub struct CrashModel {
    pub masses_kg: Vec<f64>,
    pub spring_constants_n_per_m: Vec<f64>,
    pub damping_ns_per_m: Vec<f64>,
    pub initial_velocity_mps: f64,
}

impl CrashModel {
    fn validate(&self) -> Result<(), CrashSimError> {
        let n = self.masses_kg.len();
        if n < 2 {
            return Err(CrashSimError::TooFewMasses(n));
        }
        for (i, &m) in self.masses_kg.iter().enumerate() {
            if m <= 0.0 {
                return Err(CrashSimError::NonPositiveMass(i, m));
            }
        }
        Ok(())
    }
}

/// 「強風の日」「雨の日のスリップ」といった環境条件を衝突シミュレーション
/// の入力へ橋渡しするための簡易調整パラメータ。
///
/// **簡易近似(正直な開示、必読)**: 本格的な車両運動力学(タイヤの
/// スリップ角・摩擦円制約・車体スリップ角を時々刻々連成計算する
/// マルチボディダイナミクス)は一切行わない。あくまで
/// [`crate::crosswind::evaluate_crosswind`]の転倒安全率と
/// [`crate::tire_wet_grip::evaluate_wet_grip`]のウェットグリップ参考
/// 指標という、**既に計算済みの2つの参考指標を、衝突条件を悪化させる
/// 方向の単純な倍率へ変換するだけ**の橋渡しである。「環境条件が悪い
/// ほど衝突エネルギーが増える/エアバッグ展開タイミングが早まる」という
/// 参考シナリオ比較ができれば十分という位置づけで、実際の制動距離・
/// 摩擦係数の物理量としての正確さは保証しない。
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentConditions {
    /// 横風の転倒安全率(`CrosswindResult::overturn_safety_factor`)。
    /// 1.0未満で転倒リスク域の目安(値が小さいほど環境条件が厳しい)。
    pub crosswind_overturn_safety_factor: f64,
    /// 雨天時ウェットグリップ参考指標(`WetGripResult::wet_grip_index`、
    /// 0〜100。値が小さいほど路面が滑りやすく環境条件が厳しい)。
    pub wet_grip_index: f64,
}

impl EnvironmentConditions {
    /// [`CrosswindResult`]・[`WetGripResult`]からそのまま組み立てる補助関数。
    pub fn from_results(crosswind: &CrosswindResult, wet_grip: &WetGripResult) -> Self {
        EnvironmentConditions { crosswind_overturn_safety_factor: crosswind.overturn_safety_factor, wet_grip_index: wet_grip.wet_grip_index }
    }

    /// 良好な環境条件(横風なし・乾燥路面)を表す基準値。
    pub fn calm_and_dry() -> Self {
        EnvironmentConditions { crosswind_overturn_safety_factor: f64::INFINITY, wet_grip_index: 100.0 }
    }

    /// ウェットグリップ指標(0〜100、低いほど滑りやすい)を、衝突直前の
    /// 実効速度に掛ける倍率へ変換する。**簡易近似**: 「グリップが低い
    /// ほど制動が効かず、同じ制動操作でも実際の衝突速度が下がりきらない」
    /// という定性的傾向を、指標100(乾燥・良好)で倍率1.0、指標0
    /// (極めて滑りやすい)で倍率1.5(実効速度が最大50%高いまま衝突する)
    /// となる線形補間で表す。この1.5という上限は工学的な実測に基づく
    /// ものではなく、シナリオ比較用の例示的な係数である。
    fn slip_speed_multiplier(&self) -> f64 {
        let grip = self.wet_grip_index.clamp(0.0, 100.0);
        1.0 + 0.5 * (1.0 - grip / 100.0)
    }

    /// 横風の転倒安全率を、衝突剛性(ばね定数)を下げる倍率へ変換する。
    /// **簡易近似**: 「横風で車体が不安定な状態(安全率が低い)ほど、
    /// 衝突時に car body が姿勢を崩し正面からまっすぐ当たらないため、
    /// 前面構造の実効的な圧壊剛性がやや下がる(横方向へ力が逃げる)」
    /// という定性的傾向を、安全率2.0以上(安定)で倍率1.0、安全率1.0
    /// (転倒限界)以下で倍率0.8(剛性が20%下がる)となる線形補間で表す。
    fn crosswind_stiffness_multiplier(&self) -> f64 {
        let safety = self.crosswind_overturn_safety_factor;
        if !safety.is_finite() || safety >= 2.0 {
            1.0
        } else if safety <= 1.0 {
            0.8
        } else {
            // safety in (1.0, 2.0): 線形補間で0.8→1.0
            0.8 + 0.2 * (safety - 1.0)
        }
    }
}

/// [`CrashModel`]に環境条件を適用し、調整済みの新しいモデルを返す。
/// 初速をウェットグリップ倍率で増やし、先頭ばね定数を横風倍率で
/// 減らす(他のパラメータはそのまま)。呼び出し側で`simulate_crash`へ
/// 渡す前に使う想定。
pub fn apply_environment_conditions(model: &CrashModel, env: &EnvironmentConditions) -> CrashModel {
    let speed_mult = env.slip_speed_multiplier();
    let stiffness_mult = env.crosswind_stiffness_multiplier();
    CrashModel {
        masses_kg: model.masses_kg.clone(),
        spring_constants_n_per_m: model.spring_constants_n_per_m.iter().map(|&k| k * stiffness_mult).collect(),
        damping_ns_per_m: model.damping_ns_per_m.clone(),
        initial_velocity_mps: model.initial_velocity_mps * speed_mult,
    }
}

/// 中心差分法(陽解法)の数値安定条件から求める目安の時間刻み幅(s)。
///
/// **簡易近似**: 単一自由度の調和振動子の安定条件`dt < 2/omega`
/// (`omega = sqrt(k/m)`)を、モデル中の最大剛性・最小質量の組み合わせに
/// 適用した最も厳しい(保守的な)目安値。連成系の厳密な安定限界とは
/// 異なりうるため、実際に使う`time_step_s`にはさらに安全率(既定で1/4程度)
/// を掛けることを推奨する。
pub fn stable_time_step_s(model: &CrashModel) -> f64 {
    let max_k = model.spring_constants_n_per_m.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
    let min_m = model.masses_kg.iter().cloned().fold(f64::MAX, f64::min);
    let omega = (max_k / min_m).sqrt();
    (2.0 / omega) * 0.25 // 安全率0.25を掛けた保守的な目安
}

/// 時系列の1ステップ分の状態(全質点分)。
#[derive(Debug, Clone, PartialEq)]
pub struct CrashStep {
    pub time_s: f64,
    pub displacements_m: Vec<f64>,
    pub velocities_mps: Vec<f64>,
    pub accelerations_mps2: Vec<f64>,
}

/// 衝突シミュレーション結果。
#[derive(Debug, Clone)]
pub struct CrashSimResult {
    pub steps: Vec<CrashStep>,
    /// 全時間・全質点を通じた最大加速度の絶対値(m/s^2)。
    pub max_acceleration_mps2: f64,
    /// 上記をG(9.80665 m/s^2単位)に換算した値。
    pub max_acceleration_g: f64,
    /// 先頭質点(質点0)の最大変位(圧壊量、m)。
    pub max_crush_displacement_m: f64,
    pub disclaimer: &'static str,
}

pub(crate) const G_MPS2: f64 = 9.80665;

const DISCLAIMER: &str = "この結果は簡易的な1次元質点+ばね+ダンパーモデルによる数値積分のドラフトであり、\
専門の衝突解析ソフトウェア(LS-DYNA等の非線形FEM)の代替ではない。\
実際の車両安全性認証・法規適合性の根拠には使えず、必ず有資格の衝突安全性評価技術者によるレビューが必要。\
乗員保護の目安値(G値)は一般的に引用される参考値のオーダーに過ぎず、断定的な合否判定ではない。";

/// 剛性・減衰行列を組み立てる(n x n、行優先、対称三重対角、
/// `structural::assemble_global_stiffness`と同じ構造)。
fn assemble_tridiagonal(n: usize, coeffs: &[f64]) -> Vec<f64> {
    let mut mat = vec![0.0_f64; n * n];
    for i in 0..n {
        let left = if i > 0 { coeffs[i - 1] } else { 0.0 };
        let right = if i + 1 < n { coeffs[i] } else { 0.0 };
        mat[i * n + i] = left + right;
        if i + 1 < n {
            mat[i * n + (i + 1)] = -right;
            mat[(i + 1) * n + i] = -right;
        }
    }
    mat
}

/// `mat * vec`を`opencuda_blas::sgemm`(既存の実機検証済みGEMM実装、
/// CPU経路)へpath依存させて計算する(n x n * n x 1の行列-ベクトル積)。
fn matvec_via_gemm(mat_f32: &[f32], vec_f32: &[f32], n: usize) -> Vec<f32> {
    let device = opencuda_cpu::CpuDevice::new(0);
    let mut out = vec![0.0f32; n];
    opencuda_blas::sgemm(&*device, n, n, 1, 1.0, mat_f32, vec_f32, 0.0, &mut out, None)
        .expect("opencuda-blas CpuNaive sgemm must not fail for well-formed inputs");
    out
}

/// 衝突シミュレーションを実行する(陽解法・中心差分法による時間積分)。
///
/// 初期条件: 全質点が`model.initial_velocity_mps`(先頭方向への衝突速度)
/// を持ち、変位0から開始する(壁またはもう一方の車両への衝突を、
/// 質点チェーンの固定端〈質点n-1側、剛体壁近似〉への衝突として扱う簡易化)。
pub fn simulate_crash(model: &CrashModel, time_step_s: f64, num_steps: usize) -> Result<CrashSimResult, CrashSimError> {
    model.validate()?;
    if time_step_s <= 0.0 {
        return Err(CrashSimError::NonPositiveTimeStep(time_step_s));
    }
    if num_steps == 0 {
        return Err(CrashSimError::NoSteps(num_steps));
    }

    let n = model.masses_kg.len();
    let k_mat = assemble_tridiagonal(n, &model.spring_constants_n_per_m);
    let c_mat = assemble_tridiagonal(n, &model.damping_ns_per_m);
    let k_f32: Vec<f32> = k_mat.iter().map(|&v| v as f32).collect();
    let c_f32: Vec<f32> = c_mat.iter().map(|&v| v as f32).collect();

    // 初期状態: 変位0、速度は先頭(質点0)側から質点n-1(固定壁近似、
    // 速度0で拘束)へ向けて衝突する運動を近似するため、質点0のみ初速を
    // 持たせ、他は静止状態から相互作用で動き出す設定にする
    // (先頭車両が停止した後尾車両/壁に衝突する典型的な衝突試験の近似)。
    let mut disp = vec![0.0_f64; n];
    let mut vel = vec![0.0_f64; n];
    vel[0] = -model.initial_velocity_mps; // 質点0が負方向(壁側)へ向かう

    let mut steps = Vec::with_capacity(num_steps + 1);
    let mut max_abs_accel = 0.0_f64;
    let mut max_crush = 0.0_f64;

    let compute_accel = |disp: &[f64], vel: &[f64]| -> Vec<f64> {
        let disp_f32: Vec<f32> = disp.iter().map(|&v| v as f32).collect();
        let vel_f32: Vec<f32> = vel.iter().map(|&v| v as f32).collect();
        let kx = matvec_via_gemm(&k_f32, &disp_f32, n);
        let cv = matvec_via_gemm(&c_f32, &vel_f32, n);
        (0..n)
            .map(|i| {
                let force = -(kx[i] as f64) - (cv[i] as f64);
                force / model.masses_kg[i]
            })
            .collect()
    };

    let mut accel = compute_accel(&disp, &vel);
    steps.push(CrashStep { time_s: 0.0, displacements_m: disp.clone(), velocities_mps: vel.clone(), accelerations_mps2: accel.clone() });

    for step_idx in 1..=num_steps {
        // 半歩速度更新(velocity Verlet/中心差分法の一形態)。
        for i in 0..n {
            vel[i] += accel[i] * time_step_s;
            disp[i] += vel[i] * time_step_s;
        }
        accel = compute_accel(&disp, &vel);

        for &a in &accel {
            max_abs_accel = max_abs_accel.max(a.abs());
        }
        max_crush = max_crush.max(disp[0].abs());

        steps.push(CrashStep {
            time_s: step_idx as f64 * time_step_s,
            displacements_m: disp.clone(),
            velocities_mps: vel.clone(),
            accelerations_mps2: accel.clone(),
        });
    }

    Ok(CrashSimResult {
        steps,
        max_acceleration_mps2: max_abs_accel,
        max_acceleration_g: max_abs_accel / G_MPS2,
        max_crush_displacement_m: max_crush,
        disclaimer: DISCLAIMER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 既知の解析解との比較: 単一の質点-ばね系(質点1個+固定壁への
    /// ばね1本、減衰0)は単振動(単純調和振動)であり、角振動数
    /// `omega = sqrt(k/m)`で振動する。数値積分の結果とこの解析解の
    /// 周期・振幅を比較する(2質点モデルの片方を非常に大きい質量にして
    /// 実質的に固定端として扱う近似)。
    #[test]
    fn two_mass_undamped_matches_single_harmonic_oscillator_period() {
        let m = 1000.0; // 動く質点
        let k = 4.0e5; // ばね定数
        let model = CrashModel {
            masses_kg: vec![m, 1.0e9], // 2つ目は事実上固定端
            spring_constants_n_per_m: vec![k],
            damping_ns_per_m: vec![0.0],
            initial_velocity_mps: 10.0,
        };
        let omega = (k / m).sqrt();
        let period_s = 2.0 * std::f64::consts::PI / omega;
        let stable_dt = stable_time_step_s(&model);
        let dt = stable_dt.min(period_s / 200.0);
        let num_steps = (period_s / dt).ceil() as usize * 2;

        let result = simulate_crash(&model, dt, num_steps).unwrap();

        // 解析解: x(t) = -(v0/omega) * sin(omega t) (質点0の変位、
        // 初速-v0からスタートする単振動)。振幅 = v0/omega。
        let expected_amplitude = model.initial_velocity_mps / omega;
        let got_amplitude = result.steps.iter().map(|s| s.displacements_m[0].abs()).fold(0.0_f64, f64::max);
        assert!(
            (got_amplitude - expected_amplitude).abs() / expected_amplitude < 0.05,
            "got_amplitude={got_amplitude}, expected={expected_amplitude}"
        );

        // 解析解: 最大加速度 = omega^2 * amplitude = v0 * omega
        let expected_max_accel = model.initial_velocity_mps * omega;
        assert!(
            (result.max_acceleration_mps2 - expected_max_accel).abs() / expected_max_accel < 0.05,
            "got_accel={}, expected={expected_max_accel}",
            result.max_acceleration_mps2
        );
    }

    #[test]
    fn rejects_too_few_masses() {
        let model = CrashModel { masses_kg: vec![1000.0], spring_constants_n_per_m: vec![], damping_ns_per_m: vec![], initial_velocity_mps: 10.0 };
        let err = simulate_crash(&model, 1e-4, 10).unwrap_err();
        assert_eq!(err, CrashSimError::TooFewMasses(1));
    }

    #[test]
    fn rejects_non_positive_mass() {
        let model = CrashModel {
            masses_kg: vec![1000.0, 0.0],
            spring_constants_n_per_m: vec![1.0e5],
            damping_ns_per_m: vec![0.0],
            initial_velocity_mps: 10.0,
        };
        let err = simulate_crash(&model, 1e-4, 10).unwrap_err();
        assert_eq!(err, CrashSimError::NonPositiveMass(1, 0.0));
    }

    #[test]
    fn higher_initial_velocity_yields_higher_max_acceleration() {
        let base_model = CrashModel {
            masses_kg: vec![1200.0, 1.0e9],
            spring_constants_n_per_m: vec![3.0e5],
            damping_ns_per_m: vec![500.0],
            initial_velocity_mps: 8.0,
        };
        let fast_model = CrashModel { initial_velocity_mps: 20.0, ..base_model.clone() };

        let dt = stable_time_step_s(&base_model);
        let slow_result = simulate_crash(&base_model, dt, 500).unwrap();
        let fast_result = simulate_crash(&fast_model, dt, 500).unwrap();

        assert!(
            fast_result.max_acceleration_mps2 > slow_result.max_acceleration_mps2,
            "fast={}, slow={}",
            fast_result.max_acceleration_mps2,
            slow_result.max_acceleration_mps2
        );
    }

    #[test]
    fn damping_reduces_peak_crush_displacement_compared_to_undamped() {
        let undamped = CrashModel {
            masses_kg: vec![1200.0, 1.0e9],
            spring_constants_n_per_m: vec![3.0e5],
            damping_ns_per_m: vec![0.0],
            initial_velocity_mps: 10.0,
        };
        let damped = CrashModel { damping_ns_per_m: vec![3000.0], ..undamped.clone() };

        let dt = stable_time_step_s(&undamped);
        let undamped_result = simulate_crash(&undamped, dt, 2000).unwrap();
        let damped_result = simulate_crash(&damped, dt, 2000).unwrap();

        assert!(
            damped_result.max_crush_displacement_m <= undamped_result.max_crush_displacement_m,
            "damped={}, undamped={}",
            damped_result.max_crush_displacement_m,
            undamped_result.max_crush_displacement_m
        );
    }

    /// 「同一の衝突条件で、ウェットグリップ指標が低い場合の方が最大
    /// 加速度が大きい(または同等以上)」ことの検証。
    #[test]
    fn lower_wet_grip_yields_higher_or_equal_max_acceleration() {
        let base_model = CrashModel {
            masses_kg: vec![1200.0, 1.0e9],
            spring_constants_n_per_m: vec![3.0e5],
            damping_ns_per_m: vec![1000.0],
            initial_velocity_mps: 10.0,
        };
        let dry = EnvironmentConditions { crosswind_overturn_safety_factor: f64::INFINITY, wet_grip_index: 100.0 };
        let wet = EnvironmentConditions { crosswind_overturn_safety_factor: f64::INFINITY, wet_grip_index: 10.0 };

        let dry_model = apply_environment_conditions(&base_model, &dry);
        let wet_model = apply_environment_conditions(&base_model, &wet);

        let dt = stable_time_step_s(&base_model);
        let dry_result = simulate_crash(&dry_model, dt, 800).unwrap();
        let wet_result = simulate_crash(&wet_model, dt, 800).unwrap();

        assert!(
            wet_result.max_acceleration_mps2 >= dry_result.max_acceleration_mps2,
            "wet={}, dry={}",
            wet_result.max_acceleration_mps2,
            dry_result.max_acceleration_mps2
        );
    }

    /// 「横風速度(転倒安全率の低下として表現)が大きい場合の方が衝突
    /// 条件が厳しくなる」ことの検証(前面剛性が下がる分、同じ初速でも
    /// 圧壊量〈変位〉が増える方向になることを確認)。
    #[test]
    fn lower_crosswind_safety_factor_increases_crush_displacement() {
        let base_model = CrashModel {
            masses_kg: vec![1200.0, 1.0e9],
            spring_constants_n_per_m: vec![3.0e5],
            damping_ns_per_m: vec![1000.0],
            initial_velocity_mps: 10.0,
        };
        let calm = EnvironmentConditions { crosswind_overturn_safety_factor: f64::INFINITY, wet_grip_index: 100.0 };
        let windy = EnvironmentConditions { crosswind_overturn_safety_factor: 1.0, wet_grip_index: 100.0 };

        let calm_model = apply_environment_conditions(&base_model, &calm);
        let windy_model = apply_environment_conditions(&base_model, &windy);

        let dt = stable_time_step_s(&base_model);
        let calm_result = simulate_crash(&calm_model, dt, 800).unwrap();
        let windy_result = simulate_crash(&windy_model, dt, 800).unwrap();

        assert!(
            windy_result.max_crush_displacement_m >= calm_result.max_crush_displacement_m,
            "windy={}, calm={}",
            windy_result.max_crush_displacement_m,
            calm_result.max_crush_displacement_m
        );
    }

    #[test]
    fn calm_and_dry_conditions_leave_model_unchanged() {
        let base_model = CrashModel {
            masses_kg: vec![1200.0, 1.0e9],
            spring_constants_n_per_m: vec![3.0e5],
            damping_ns_per_m: vec![1000.0],
            initial_velocity_mps: 10.0,
        };
        let calm = EnvironmentConditions::calm_and_dry();
        let adjusted = apply_environment_conditions(&base_model, &calm);
        assert!((adjusted.initial_velocity_mps - base_model.initial_velocity_mps).abs() < 1e-9);
        assert!((adjusted.spring_constants_n_per_m[0] - base_model.spring_constants_n_per_m[0]).abs() < 1e-9);
    }

    #[test]
    fn vehicle_kind_reference_g_values_are_positive() {
        assert!(VehicleKind::Car.reference_survivable_g() > 0.0);
        assert!(VehicleKind::Motorcycle.reference_survivable_g() > 0.0);
        assert!(VehicleKind::Maglev.reference_survivable_g() > 0.0);
        assert!(VehicleKind::Bus.reference_survivable_g() > 0.0);
        assert!(VehicleKind::Truck.reference_survivable_g() > 0.0);
        assert!(VehicleKind::Train.reference_survivable_g() > 0.0);
    }
}
