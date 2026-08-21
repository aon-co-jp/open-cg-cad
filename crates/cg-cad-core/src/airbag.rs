//! エアバッグ(ハンドル/助手席/カーテン/サイド)の展開タイミング・乗員
//! 保護効果の簡易シミュレーション(MVP)。
//!
//! **免責事項(必読、`crash_sim.rs`と同水準で省略・弱化しないこと)**:
//! 本モジュールが行うのは、実務のエアバッグ開発で使われる本格的な
//! ガス発生器の熱流体シミュレーション(化学反応・ガス展開の流体構造連成
//! 解析、CFD+FEM連成、専門のダミー人形モデル〈Hybrid III等〉を用いた
//! 有限要素衝突解析)ではなく、**[`crate::crash_sim`]が計算した車体側の
//! 加速度時系列を入力として、乗員を1自由度の質点として近似し、
//! 拘束具(シートベルト)・エアバッグ・ハード接触(ステアリングホイール/
//! ダッシュボード等の剛体構造)をそれぞれ単純なばね+ダンパー要素で
//! モデル化した運動方程式の数値積分**である。
//!
//! # 数値計算とLLMの役割分担(`crash_sim`・`structural`と同じ方針)
//!
//! - **数値計算はこのモジュールのRustコードが行う**。ただし本モジュール
//!   自体は新規にGEMM呼び出しコードを書かない——車体側の加速度時系列
//!   ([`crate::crash_sim::CrashSimResult`])は、既に`opencuda_blas::sgemm`
//!   をpath依存で用いた[`crate::crash_sim::simulate_crash`]によって
//!   計算済みのものをそのまま入力として受け取る設計にした。乗員は
//!   1自由度(スカラー)のみの追加DOFであり、ここで改めて1x1の行列積を
//!   GEMM経由にしても意味のあるGPU計算量にはならず、既存資産の
//!   無意味な水増し呼び出しになるため、あえて素の浮動小数点演算とした
//!   (正直な設計判断の記録)。
//! - **LLM(aruaru-llm)は計算を一切行わない**——計算済みの数値結果を
//!   受け取り、自然言語の説明文へ変換する役割のみ
//!   (`crate::llm_explain::build_airbag_explanation_prompt`参照)。
//!
//! # モデルの考え方
//!
//! 乗員を1質点として、車体側の基準質点(`vehicle_mass_index`で指定する
//! [`crate::crash_sim::CrashStep`]中の質点、例: 運転席なら計器盤/
//! ステアリングホイールに近い先頭側の質点)からの**相対変位**として
//! 運動を追う。以下の3つの力学要素を(該当する条件下でのみ)合算する:
//!
//! 1. **拘束具(シートベルト/シート)**: 常時有効、弱いばね+ダンパー
//!    (`OccupantParams::restraint_*`)。
//! 2. **エアバッグ**: [`AirbagParams::trigger_g`]を超える車体加速度を
//!    検出した時刻から`deploy_delay_s`後に展開したとみなし、それ以降
//!    かつ相対変位が`AirbagParams::engage_clearance_m`(エアバッグが
//!    実際に乗員に接触し始めるクリアランス)を超えた範囲でのみ、
//!    比較的柔らかいばね+ダンパー(`deploy_spring_n_per_m`/
//!    `deploy_damping_ns_per_m`)が働く。
//! 3. **ハード接触(ステアリングホイール/ダッシュボード等の剛体構造)**:
//!    エアバッグの有無によらず常に存在し、相対変位が
//!    `OccupantParams::hard_contact_clearance_m`を超えた範囲でのみ、
//!    非常に硬いばね+ダンパー(`hard_contact_spring_n_per_m`/
//!    `hard_contact_damping_ns_per_m`)が働く。
//!
//! `engage_clearance_m < hard_contact_clearance_m`となるよう設定する
//! ことで、エアバッグが先に乗員を減速させ、より柔らかい相互作用の
//! 距離を稼ぐことで剛体構造への高速衝突を防ぐ(または衝突速度を
//! 下げる)という、実際のエアバッグの働き(拘束距離を延ばして
//! 衝撃を緩和する)を簡易的に模している。
//!
//! # 簡略化している点(正直な開示)
//!
//! - 乗員は1自由度の質点のみ(頭部・胸部・骨盤等を区別する多質点
//!   ダミー人形モデルではない)。頭部傷害基準(HIC)・胸部傷害基準
//!   (胸部たわみ量等)のような専門的な傷害基準値の算出は行わない。
//! - エアバッグのガス発生器の熱化学反応・展開時の空気力学(布地の
//!   展開形状変化)は一切モデル化していない。展開を「ある時刻以降、
//!   ばね+ダンパーが有効になる」という単純なスイッチとして近似。
//! - 展開トリガー閾値([`AirbagType::typical_trigger_g`])は特定の
//!   実在エアバッグECUの較正値ではなく、一般的に引用されるオーダー感
//!   を参考にした例示値に過ぎない。
//! - 車両1次元衝突方向のみを扱い、側突(サイド/カーテンエアバッグが
//!   本来主眼とする横方向の衝撃)を専用の横方向モデルとして区別しては
//!   いない——`vehicle_mass_index`に側突方向の車体加速度時系列を渡す
//!   運用を想定した簡易的な流用に留まる(正直な開示)。

use crate::crash_sim::{CrashSimResult, G_MPS2};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum AirbagError {
    #[error("crash_result.steps must have at least 2 steps, got {0}")]
    TooFewSteps(usize),
    #[error("vehicle_mass_index {0} is out of range for {1} masses")]
    MassIndexOutOfRange(usize, usize),
    #[error("occupant mass_kg must be positive, got {0}")]
    NonPositiveMass(f64),
    #[error("airbag engage_clearance_m ({0}) must be less than hard_contact_clearance_m ({1})")]
    EngageClearanceNotBeforeHardContact(f64, f64),
}

/// エアバッグの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AirbagType {
    /// ハンドル(運転席)エアバッグ。
    DriverFrontal,
    /// 助手席エアバッグ。
    PassengerFrontal,
    /// カーテンエアバッグ(ルーフ側から展開するサイドカーテンタイプ)。
    Curtain,
    /// サイドエアバッグ(ドア/シート側面から展開)。
    Side,
}

impl AirbagType {
    /// 参考として引用する展開トリガー閾値(G)。
    /// **正直な開示**: 特定の実在エアバッグECUの較正値ではなく、
    /// 一般的に引用されるオーダー感(正面衝突用は側突用よりやや高い
    /// 閾値・側突/カーテン用はより短時間での判定が必要、というよく
    /// 知られる一般原理)を参考にした例示値に過ぎない。
    pub fn typical_trigger_g(&self) -> f64 {
        match self {
            AirbagType::DriverFrontal => 8.0,
            AirbagType::PassengerFrontal => 8.0,
            AirbagType::Curtain => 5.0,
            AirbagType::Side => 4.0,
        }
    }

    /// 参考として引用する展開遅延時間(トリガー検出から実際に乗員へ
    /// 力が働き始めるまでのインフレータ展開時間、秒)。
    /// **正直な開示**: 側突/カーテン用はより高速に展開する必要がある
    /// という一般原理を反映した例示値であり、実測値ではない。
    pub fn typical_deploy_delay_s(&self) -> f64 {
        match self {
            AirbagType::DriverFrontal => 0.020,
            AirbagType::PassengerFrontal => 0.025,
            AirbagType::Curtain => 0.010,
            AirbagType::Side => 0.008,
        }
    }
}

/// 乗員(質点)のパラメータ。
#[derive(Debug, Clone)]
pub struct OccupantParams {
    pub mass_kg: f64,
    /// シートベルト/シートによる常時有効な弱い拘束(ばね定数、N/m)。
    pub restraint_spring_n_per_m: f64,
    pub restraint_damping_ns_per_m: f64,
    /// ハード接触(ステアリングホイール/ダッシュボード等)が始まる
    /// 相対変位のクリアランス(m)。エアバッグの有無によらず常に存在。
    pub hard_contact_clearance_m: f64,
    pub hard_contact_spring_n_per_m: f64,
    pub hard_contact_damping_ns_per_m: f64,
}

/// エアバッグ展開パラメータ。
#[derive(Debug, Clone)]
pub struct AirbagParams {
    pub airbag_type: AirbagType,
    /// 展開トリガー閾値(G)。
    pub trigger_g: f64,
    /// トリガー検出から実際に力が働き始めるまでの遅延(秒)。
    pub deploy_delay_s: f64,
    /// エアバッグが乗員に接触し始める相対変位のクリアランス(m)。
    /// `OccupantParams::hard_contact_clearance_m`より小さい値にする
    /// ことで、ハード接触より先にエアバッグが乗員を減速させる設計。
    pub engage_clearance_m: f64,
    pub deploy_spring_n_per_m: f64,
    pub deploy_damping_ns_per_m: f64,
}

impl AirbagParams {
    /// [`AirbagType`]の参考値(`typical_trigger_g`/`typical_deploy_delay_s`)
    /// を使い、クリアランス・ばね定数のみ指定して組み立てる補助関数。
    pub fn with_typical_trigger(
        airbag_type: AirbagType,
        engage_clearance_m: f64,
        deploy_spring_n_per_m: f64,
        deploy_damping_ns_per_m: f64,
    ) -> Self {
        AirbagParams {
            airbag_type,
            trigger_g: airbag_type.typical_trigger_g(),
            deploy_delay_s: airbag_type.typical_deploy_delay_s(),
            engage_clearance_m,
            deploy_spring_n_per_m,
            deploy_damping_ns_per_m,
        }
    }
}

/// 乗員シミュレーションの1ステップ分の状態。
#[derive(Debug, Clone, PartialEq)]
pub struct OccupantStep {
    pub time_s: f64,
    /// 車体基準質点に対する乗員の相対変位(m)。
    pub relative_displacement_m: f64,
    /// 乗員の絶対加速度(m/s^2、傷害の目安として用いる)。
    pub occupant_acceleration_mps2: f64,
}

/// エアバッグ保護シミュレーション結果。
#[derive(Debug, Clone)]
pub struct AirbagSimResult {
    pub steps: Vec<OccupantStep>,
    /// エアバッグが実際に(トリガー検出+展開遅延を経て)展開したと
    /// みなした時刻(s)。展開条件に一度も達しなかった場合、または
    /// エアバッグ無しの条件の場合は`None`。
    pub deployment_time_s: Option<f64>,
    /// 乗員の最大絶対加速度の絶対値(m/s^2)。
    pub occupant_max_acceleration_mps2: f64,
    pub occupant_max_acceleration_g: f64,
    pub disclaimer: &'static str,
}

const DISCLAIMER: &str = "この結果は、crash_simが計算した車体側加速度時系列を入力とした、\
乗員1質点+ばね+ダンパーによる簡易近似モデルのドラフトである。専門の衝突解析ソフトウェア・\
ダミー人形モデルを用いた有限要素解析の代替ではなく、実際のエアバッグ設計・車両安全性認証の\
根拠には使えない。必ず有資格の衝突安全性評価技術者によるレビューが必要。";

/// 乗員(+エアバッグ有無)のシミュレーションを実行する。
///
/// `crash_result`: [`crate::crash_sim::simulate_crash`]が既に計算した
/// 車体側の時系列(数値積分の入力として再利用、ここでは計算し直さない)。
/// `vehicle_mass_index`: 乗員位置に対応する車体基準質点のインデックス
/// (例: 運転席なら先頭側の質点)。
/// `airbag`: `Some`ならエアバッグ有りの条件、`None`ならエアバッグ無し
/// (拘束具+ハード接触のみ)の条件でシミュレートする。
pub fn simulate_occupant_protection(
    crash_result: &CrashSimResult,
    vehicle_mass_index: usize,
    occupant: &OccupantParams,
    airbag: Option<&AirbagParams>,
) -> Result<AirbagSimResult, AirbagError> {
    let n_steps = crash_result.steps.len();
    if n_steps < 2 {
        return Err(AirbagError::TooFewSteps(n_steps));
    }
    let n_masses = crash_result.steps[0].displacements_m.len();
    if vehicle_mass_index >= n_masses {
        return Err(AirbagError::MassIndexOutOfRange(vehicle_mass_index, n_masses));
    }
    if occupant.mass_kg <= 0.0 {
        return Err(AirbagError::NonPositiveMass(occupant.mass_kg));
    }
    if let Some(ab) = airbag {
        if ab.engage_clearance_m >= occupant.hard_contact_clearance_m {
            return Err(AirbagError::EngageClearanceNotBeforeHardContact(ab.engage_clearance_m, occupant.hard_contact_clearance_m));
        }
    }

    // トリガー検出: 車体基準質点の加速度がtrigger_gを最初に超える時刻。
    let deployment_time_s = airbag.and_then(|ab| {
        crash_result
            .steps
            .iter()
            .find(|s| (s.accelerations_mps2[vehicle_mass_index]).abs() / G_MPS2 >= ab.trigger_g)
            .map(|s| s.time_s + ab.deploy_delay_s)
    });

    // 乗員は初期状態で車体と同じ位置・速度から開始(相対変位0)。
    let mut rel_disp = 0.0_f64;
    let mut occupant_vel = crash_result.steps[0].velocities_mps[vehicle_mass_index];
    let mut steps = Vec::with_capacity(n_steps);
    let mut max_abs_accel = 0.0_f64;

    // 相対変位・相対速度から乗員加速度を求める。`airbag_active`が真の
    // 場合のみ、エアバッグのばね+ダンパー項を追加する(展開前は
    // `false`のまま呼ぶことで、拘束具+ハード接触のみの力になる)。
    let compute_occupant_accel = |rel_disp: f64, rel_vel: f64, airbag_active: bool| -> f64 {
        let mut force = -occupant.restraint_spring_n_per_m * rel_disp - occupant.restraint_damping_ns_per_m * rel_vel;
        if rel_disp > occupant.hard_contact_clearance_m {
            let excess = rel_disp - occupant.hard_contact_clearance_m;
            force -= occupant.hard_contact_spring_n_per_m * excess + occupant.hard_contact_damping_ns_per_m * rel_vel;
        }
        if airbag_active {
            if let Some(ab) = airbag {
                if rel_disp > ab.engage_clearance_m {
                    let excess = rel_disp - ab.engage_clearance_m;
                    force -= ab.deploy_spring_n_per_m * excess + ab.deploy_damping_ns_per_m * rel_vel;
                }
            }
        }
        force / occupant.mass_kg
    };
    let is_deployed_at = |t: f64| -> bool { deployment_time_s.is_some_and(|t_deploy| t >= t_deploy) };

    // 乗員側の自由振動角振動数から求めた安定な時間刻み幅(`crash_sim::
    // stable_time_step_s`と同じ考え方)。ハード接触・エアバッグのばね
    // 定数は`crash_sim`側の車体剛性よりずっと硬く(短いクリアランスで
    // 大きな減速力を出すため)、`crash_result.steps`側の時間刻み幅
    // (車体側の剛性から決めたもの)をそのまま使うと数値的に不安定に
    // なりうる。そのためcrash_resultの各ステップ間隔を、乗員側の
    // 安定条件を満たすようさらに細かく分割してから積分する
    // (線形補間ではなく、区間内は車体速度を区間開始時点の値で一定と
    // みなす簡易近似——`crash_sim`のステップ幅に対して十分細かく
    // 分割すれば実用上問題ない精度になる)。
    let max_occupant_k = [
        occupant.restraint_spring_n_per_m,
        occupant.hard_contact_spring_n_per_m,
        airbag.map(|ab| ab.deploy_spring_n_per_m).unwrap_or(0.0),
    ]
    .into_iter()
    .fold(0.0_f64, f64::max)
    .max(1.0);
    let occupant_omega = (max_occupant_k / occupant.mass_kg).sqrt();
    let occupant_stable_dt = (2.0 / occupant_omega) * 0.1;

    // t=0
    let v_vehicle0 = crash_result.steps[0].velocities_mps[vehicle_mass_index];
    let t0 = crash_result.steps[0].time_s;
    let mut accel = compute_occupant_accel(rel_disp, occupant_vel - v_vehicle0, is_deployed_at(t0));
    max_abs_accel = max_abs_accel.max(accel.abs());
    steps.push(OccupantStep { time_s: t0, relative_displacement_m: rel_disp, occupant_acceleration_mps2: accel });

    for i in 1..n_steps {
        let crash_dt = crash_result.steps[i].time_s - crash_result.steps[i - 1].time_s;
        let v_vehicle = crash_result.steps[i - 1].velocities_mps[vehicle_mass_index];
        let deployed = is_deployed_at(crash_result.steps[i - 1].time_s);

        let n_sub = ((crash_dt / occupant_stable_dt).ceil() as usize).max(1);
        let sub_dt = crash_dt / n_sub as f64;
        for _ in 0..n_sub {
            let rel_vel = occupant_vel - v_vehicle;
            accel = compute_occupant_accel(rel_disp, rel_vel, deployed);
            occupant_vel += accel * sub_dt;
            rel_disp += (occupant_vel - v_vehicle) * sub_dt;
        }

        max_abs_accel = max_abs_accel.max(accel.abs());
        steps.push(OccupantStep {
            time_s: crash_result.steps[i].time_s,
            relative_displacement_m: rel_disp,
            occupant_acceleration_mps2: accel,
        });
    }

    Ok(AirbagSimResult {
        steps,
        deployment_time_s,
        occupant_max_acceleration_mps2: max_abs_accel,
        occupant_max_acceleration_g: max_abs_accel / G_MPS2,
        disclaimer: DISCLAIMER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crash_sim::{simulate_crash, stable_time_step_s, CrashModel};

    fn severe_crash_result() -> CrashSimResult {
        // 十分に厳しい(高速・高剛性)衝突条件——エアバッグ無しでは乗員の
        // 相対変位がハード接触クリアランスへ確実に達する設定。
        let model = CrashModel {
            masses_kg: vec![1200.0, 1.0e9],
            spring_constants_n_per_m: vec![5.0e5],
            damping_ns_per_m: vec![2000.0],
            initial_velocity_mps: 15.0,
        };
        let dt = stable_time_step_s(&model);
        simulate_crash(&model, dt, 1500).unwrap()
    }

    fn baseline_occupant() -> OccupantParams {
        OccupantParams {
            mass_kg: 75.0,
            restraint_spring_n_per_m: 2000.0,
            restraint_damping_ns_per_m: 50.0,
            hard_contact_clearance_m: 0.30,
            hard_contact_spring_n_per_m: 2.0e6,
            hard_contact_damping_ns_per_m: 5000.0,
        }
    }

    fn driver_airbag() -> AirbagParams {
        AirbagParams::with_typical_trigger(AirbagType::DriverFrontal, 0.10, 5.0e4, 800.0)
    }

    #[test]
    fn rejects_too_few_steps() {
        let result = CrashSimResult { steps: vec![], max_acceleration_mps2: 0.0, max_acceleration_g: 0.0, max_crush_displacement_m: 0.0, disclaimer: "test" };
        let err = simulate_occupant_protection(&result, 0, &baseline_occupant(), None).unwrap_err();
        assert_eq!(err, AirbagError::TooFewSteps(0));
    }

    #[test]
    fn rejects_mass_index_out_of_range() {
        let crash = severe_crash_result();
        let err = simulate_occupant_protection(&crash, 5, &baseline_occupant(), None).unwrap_err();
        assert_eq!(err, AirbagError::MassIndexOutOfRange(5, 2));
    }

    #[test]
    fn rejects_engage_clearance_not_before_hard_contact() {
        let crash = severe_crash_result();
        let occupant = baseline_occupant();
        let mut airbag = driver_airbag();
        airbag.engage_clearance_m = occupant.hard_contact_clearance_m + 0.01;
        let err = simulate_occupant_protection(&crash, 0, &occupant, Some(&airbag)).unwrap_err();
        assert_eq!(err, AirbagError::EngageClearanceNotBeforeHardContact(airbag.engage_clearance_m, occupant.hard_contact_clearance_m));
    }

    /// 核心の検証: 同一の衝突条件・同一の乗員パラメータで、エアバッグ
    /// 展開ありの場合、乗員質点の最大加速度が展開なしの場合より小さい
    /// (エアバッグがハード接触より先に乗員を減速させるため)。
    #[test]
    fn airbag_reduces_occupant_max_acceleration_compared_to_no_airbag() {
        let crash = severe_crash_result();
        let occupant = baseline_occupant();
        let airbag = driver_airbag();

        let without = simulate_occupant_protection(&crash, 0, &occupant, None).unwrap();
        let with = simulate_occupant_protection(&crash, 0, &occupant, Some(&airbag)).unwrap();

        assert!(with.deployment_time_s.is_some(), "airbag should trigger for a severe crash");
        assert!(
            with.occupant_max_acceleration_mps2 < without.occupant_max_acceleration_mps2,
            "with_airbag={}, without_airbag={}",
            with.occupant_max_acceleration_mps2,
            without.occupant_max_acceleration_mps2
        );
    }

    #[test]
    fn all_four_airbag_types_reduce_occupant_max_acceleration() {
        let crash = severe_crash_result();
        let occupant = baseline_occupant();
        let without = simulate_occupant_protection(&crash, 0, &occupant, None).unwrap();

        for airbag_type in [AirbagType::DriverFrontal, AirbagType::PassengerFrontal, AirbagType::Curtain, AirbagType::Side] {
            let airbag = AirbagParams::with_typical_trigger(airbag_type, 0.10, 5.0e4, 800.0);
            let with = simulate_occupant_protection(&crash, 0, &occupant, Some(&airbag)).unwrap();
            assert!(
                with.occupant_max_acceleration_mps2 < without.occupant_max_acceleration_mps2,
                "{airbag_type:?}: with={}, without={}",
                with.occupant_max_acceleration_mps2,
                without.occupant_max_acceleration_mps2
            );
        }
    }

    #[test]
    fn no_deployment_when_crash_never_exceeds_trigger_threshold() {
        // 非常に穏やかな衝突条件(低速・低剛性)——トリガー閾値に届かない。
        let mild_model = CrashModel {
            masses_kg: vec![1200.0, 1.0e9],
            spring_constants_n_per_m: vec![500.0],
            damping_ns_per_m: vec![50.0],
            initial_velocity_mps: 0.2,
        };
        let dt = stable_time_step_s(&mild_model);
        let mild_crash = simulate_crash(&mild_model, dt, 500).unwrap();

        let occupant = baseline_occupant();
        let airbag = driver_airbag();
        let result = simulate_occupant_protection(&mild_crash, 0, &occupant, Some(&airbag)).unwrap();
        assert!(result.deployment_time_s.is_none());
    }

    #[test]
    fn vehicle_kind_reference_g_and_airbag_trigger_g_are_all_positive() {
        for t in [AirbagType::DriverFrontal, AirbagType::PassengerFrontal, AirbagType::Curtain, AirbagType::Side] {
            assert!(t.typical_trigger_g() > 0.0);
            assert!(t.typical_deploy_delay_s() > 0.0);
        }
    }
}
