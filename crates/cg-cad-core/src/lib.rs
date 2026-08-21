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
//! - 日照権の実シミュレーション・耐震性の構造計算は未実装
//!   (`structural`モジュール参照、専門ソフトウェアとの連携が必要)。
//! - 建ぺい率・容積率等の法規制パラメータは、Web検索等による最新の
//!   自動取得は行わず、ユーザーが設定する入力値として扱う
//!   (`zoning`モジュール参照)。
//! - 生成される建築物形状は直方体(またはその単純な組み合わせ)に限定
//!   した、意匠検討の叩き台レベルのドラフトである。

pub mod building;
pub mod material;
pub mod mesh;
pub mod site;
pub mod structural;
pub mod zoning;

pub use building::{generate_building, Building, BuildingType, CadError};
pub use mesh::{Mesh, MeshVertex};
pub use site::{Site, SiteError};
pub use structural::{check_structural_safety, StructuralCheckResult};
pub use zoning::{check_zoning_compliance, ZoningError, ZoningRules};
