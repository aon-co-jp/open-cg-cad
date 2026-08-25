//! open-cg-cad: 「AI工務店＆AI建設」機能のためのCADコアロジック。
//!
//! # 免責事項(必読、省略・弱化しないこと)
//!
//! **このクレートが生成するCADデータ(敷地・法規制チェック結果・建築物
//! ボリューム・メッシュ)は、あくまで人間の有資格者(一級建築士・宅地
//! 建物取引士等)による最終確認の一歩手前までを代行するドラフト/サンプル
//! 生成物である。法的に確定した設計・建築確認申請に使える正式な図面を
//! 生成するものでは断じてなく、必ず有資格者のレビューが必要。**
//!
//! - 日照権の実シミュレーションは未実装。耐震性の構造計算は、簡易骨組み
//!   モデル+線形静解析(`structural`モジュール参照)としてMVP実装済みだが、
//!   本格的なFEM・専門の構造計算ソフトウェアの代替ではない
//!   (`structural`モジュールの免責事項を参照、省略しないこと)。
//! - 自動車・バイク・バス・トラック・列車・リニアモーターカーの衝突実験
//!   シミュレーション(`crash_sim`モジュール)は、質点+ばね+ダンパーの
//!   簡易多質点モデルによる1次元運動方程式の数値積分であり、本格的な
//!   非線形FEM衝突解析(LS-DYNA等)の代替ではない。`crash_sim`は
//!   `crosswind`/`tire_wet_grip`が計算した参考指標を衝突条件へ変換する
//!   簡易な橋渡し(`EnvironmentConditions`)も提供するが、これも本格的な
//!   車両運動力学の連成計算ではない。
//! - ハンドル/助手席/カーテン/サイドの4種のエアバッグ展開タイミング・
//!   乗員保護効果の簡易シミュレーション(`airbag`モジュール)は、乗員を
//!   1自由度の質点として近似したばね+ダンパーモデルであり、専門の
//!   ガス発生器熱流体解析・ダミー人形モデルを用いた有限要素解析の
//!   代替ではない。
//! - トンネル微気圧波(騒音)軽減のための列車先頭形状デザイン計算
//!   (`tunnel_noise`モジュール)は、断面積変化率を指標とする1次元
//!   簡易近似であり、本格的な数値流体力学(CFD)の代替ではない。
//! - 建ぺい率・容積率等の法規制パラメータは、Web検索等による最新の
//!   自動取得は行わず、ユーザーが設定する入力値として扱う
//!   (`zoning`モジュール参照)。
//! - 生成される建築物形状は直方体(またはその単純な組み合わせ)に限定
//!   した、意匠検討の叩き台レベルのドラフトである。
//! - `crash_sim`・`tunnel_noise`・`structural`の各簡易モデルは、実在の
//!   検証済みオープンソースCAE/CFDエンジン(CalculiX・OpenFOAM)が
//!   この環境で利用できない場合の**簡易フォールバック**という位置づけ
//!   である。本格的な解析が必要な場合は`solver_bridge`モジュール
//!   (CalculiX/OpenFOAMへの入出力ブリッジ)を使うこと——ただし
//!   `solver_bridge`自体は計算エンジンを持たず、実際のソルバー
//!   バイナリがインストールされていない環境では正直にエラーを返す
//!   設計である(`solver_bridge`モジュールのdocコメント参照)。

pub mod acoustic_material;
pub mod aero;
pub mod airbag;
pub mod building;
pub mod crash_sim;
pub mod crosswind;
pub mod llm_explain;
pub mod manufacturing_export;
pub mod material;
pub mod mesh;
pub mod nwd_control;
pub mod site;
pub mod solver_bridge;
pub mod structural;
pub mod tire_wet_grip;
pub mod tunnel_noise;
pub mod vehicle_config;
pub mod zoning;

pub use aero::{estimate_aero_force, AeroError, AeroForceResult, AeroShape};
pub use airbag::{
    simulate_occupant_protection, AirbagError, AirbagParams, AirbagSimResult, AirbagType, OccupantParams, OccupantStep,
};
pub use building::{generate_building, Building, BuildingType, CadError};
pub use crash_sim::{
    apply_environment_conditions, simulate_crash, CrashModel, CrashSimError, CrashSimResult, CrashStep, EnvironmentConditions, VehicleKind,
};
pub use crosswind::{evaluate_crosswind, CrosswindError, CrosswindResult, CrosswindVehicle};
pub use llm_explain::{
    build_airbag_explanation_prompt, build_crash_explanation_prompt, build_explanation_prompt, build_tunnel_noise_explanation_prompt,
    explain_airbag_via_aruaru_llm, explain_crash_via_aruaru_llm, explain_tunnel_noise_via_aruaru_llm, explain_via_aruaru_llm, LlmExplainError,
};
pub use manufacturing_export::{
    mesh_to_stl_ascii, robot_arm_path_to_json, ManufacturingExportError, RobotArmPath,
};
pub use mesh::{Mesh, MeshVertex};
pub use nwd_control::{
    simulate_torque_distribution, NwdControlError, NwdControlResult, NwdVehicle, WheelSide, WheelTorque,
};
pub use site::{Site, SiteError};
pub use structural::{analyze_structure, StoryDriftResult, StructuralAnalysisResult, StructuralError, StructuralMaterial};
pub use tire_wet_grip::{evaluate_wet_grip, TireWetGripError, TreadPattern, WetGripResult};
pub use tunnel_noise::{
    micro_pressure_wave_index, optimize_nose_shape, NoseOptimizationResult, NoseShape, NoseShapeCandidateResult, TunnelNoiseError,
};
pub use zoning::{check_zoning_compliance, ZoningError, ZoningRules};
